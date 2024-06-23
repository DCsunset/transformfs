// Copyright (C) 2024  DCsunset

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use fuser::{
  Filesystem,
  Request,
  ReplyDirectory,
  FUSE_ROOT_ID
};
use mlua::{Function, Lua, String as LuaString, Table, OwnedTable};
use std::{
  collections::HashMap, ffi::{OsStr, OsString}, fs, os::unix::ffi::OsStrExt, path::{Path, PathBuf}, time::Duration
};
use log::{error, warn};
use nix::{
  errno::Errno::{EIO, ENOENT}, fcntl::{self, OFlag}, sys::{self, statfs::statfs}
};
use crate::utils;

pub struct TransformFs {
  dir: PathBuf,
  ttl: Duration,
  /// Lua state with loaded script
  lua: Lua,
  /// Loaded user-defined table
  table: OwnedTable,

  /// Map inode to file name
  inode_map: HashMap<u64, OsString>
}

impl TransformFs {
  pub fn init(dir: PathBuf, script: PathBuf, ttl: Duration) -> anyhow::Result<Self> {
    let lua = Lua::new();
    let table: OwnedTable = lua.load(fs::read_to_string(script)?).eval()?;
    let mut inode_map = HashMap::new();
    inode_map.insert(FUSE_ROOT_ID, dir.clone().into());
    Ok(Self {
      dir,
      ttl,
      lua,
      table,
      inode_map
    })
  }

  pub fn read_data(&self, name: &OsStr, offset: i64, size: u32) -> mlua::Result<LuaString> {
    let read_data: Function = self.table.to_ref().get("read_data")?;
    let data = read_data.call::<_, LuaString>(
      ( self.table.to_ref(), self.lua.create_string(name.as_bytes())?, offset, size)
    )?;
    Ok(data)
  }
  pub fn read_metadata(&self, name: impl AsRef<Path>) -> anyhow::Result<fuser::FileAttr> {
    let mut attr = utils::read_attr(name.as_ref())?;
    if attr.kind == fuser::FileType::RegularFile {
      let read_metadata: Function = self.table.to_ref().get("read_metadata")?;
      let data = read_metadata.call::<_, Table>(
        (self.table.to_ref(), self.lua.create_string(name.as_ref().as_os_str().as_bytes())?)
      )?;
      let size: Option<u64> = data.get("size")?;
      if let Some(size) = size {
        attr.size = size;
        attr.blocks = (size + attr.blksize as u64 - 1) / attr.blksize as u64;
      }
    }
    Ok(attr)
  }
}

impl Filesystem for TransformFs {
  fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEntry) {
    let Some(parent_name) = self.inode_map.get(&parent) else {
      reply.error(ENOENT as i32);
      return;
    };

    let full_name = Path::new(parent_name).join(name);
    match self.read_metadata(&full_name) {
      Ok(attr) => {
        self.inode_map.insert(attr.ino, full_name.into_os_string());
        reply.entry(&self.ttl, &attr, 0);
      },
      Err(err) => {
        warn!("Error reading file {:?}: {}", name, err);
        reply.error(EIO as i32);
      }
    };
  }

  fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match fcntl::open(name.as_os_str(), OFlag::from_bits_truncate(flags), sys::stat::Mode::empty()) {
      Ok(fd) => {
        reply.opened(fd as u64, flags as u32);
      },
      Err(errno) => {
        reply.error(errno as i32);
      }
    };
  }

  fn getattr(&mut self, _req: &Request, ino: u64, reply: fuser::ReplyAttr) {
    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.read_metadata(name) {
      Ok(attr) => {
        reply.attr(&self.ttl, &attr);
      },
      Err(err) => {
        error!("Error reading file {:?}: {}", name, err);
        reply.error(EIO as i32);
      }
    }
  }

  fn readdir(
    &mut self,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
  ) {
    assert!(offset >= 0);

    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match utils::read_dir(name) {
      Ok(it) => {
        // special entries
        let mut entries = vec![
          // (ino, FileType::Directory, OsString::from("."))
          // (FUSE_ROOT_ID, FileType::Directory, OsString::from("..")),
        ];
        entries.extend(it);
        for (i, e) in entries.iter().enumerate().skip(offset as usize) {
          // offset is used by kernel for future readdir calls (should be next entry)
          if reply.add(e.0, (i+1) as i64, e.1, &e.2) {
            // return true when buffer full
            break;
          }
        }
        reply.ok();
      },
      Err(err) => {
        error!("Error reading dir {:?}: {}", name, err);
        reply.error(EIO as i32);
      }
    };
  }

  fn read(
    &mut self,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    size: u32,
    _flags: i32,
    _lock_owner: Option<u64>,
    reply: fuser::ReplyData,
  ) {
    assert!(offset >= 0);

    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.read_data(name, offset, size) {
      Ok(data) => {
        reply.data(data.as_bytes());
      },
      Err(err) => {
        error!("Error reading file {:?}: {}", name, err);
        reply.error(EIO as i32);
      }
    }
  }

  fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
    match statfs(&self.dir) {
      Ok(stat) => {
        reply.statfs(
          stat.blocks(),
          stat.blocks_free(),
          stat.blocks_available(),
          stat.files(),
          stat.files_free(),
          stat.block_size() as u32,
          stat.maximum_name_length() as u32,
          stat.block_size() as u32
        )
      },
      Err(err) => {
        reply.error(err as i32);
      }
    };
  }
}

