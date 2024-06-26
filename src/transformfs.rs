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
use mlua::{Function, Lua, OwnedFunction, OwnedTable, String as LuaString, Table};
use std::{
  collections::HashMap, ffi::{OsStr, OsString}, fs, os::unix::ffi::OsStrExt, path::{Path, PathBuf}, time::Duration
};
use log::{error, warn};
use nix::{
  errno::Errno::{EIO, ENOENT}, sys::statfs::statfs
};
use anyhow::anyhow;
use crate::utils;

pub struct UserFn {
  filter_file: Option<OwnedFunction>,
  open: Option<OwnedFunction>,
  close: Option<OwnedFunction>,
  read_data: OwnedFunction,
  read_metadata: OwnedFunction
}

fn load_user_fn(table: &Table, name: &str) -> mlua::Result<Option<OwnedFunction>> {
  Ok(
    if table.contains_key(name)? {
      // Bind the table itself as first arg (self) for method
      Some(table.get::<_, Function>(name)?.bind(table)?.into_owned())
    } else {
      None
    }
  )
}

pub struct TransformFs {
  dir: PathBuf,
  ttl: Duration,
  /// Lua state with loaded script
  lua: Lua,
  /// Loaded user-defined function
  user_fn: UserFn,

  /// Map inode to file name
  inode_map: HashMap<u64, OsString>,
  /// Map user-defined filename to original name
  /// (parent, new_name) -> orig_name
  filename_map: HashMap<(OsString, OsString), OsString>
}

impl TransformFs {
  pub fn init(dir: PathBuf, script: PathBuf, ttl: Duration) -> anyhow::Result<Self> {
    let lua = Lua::new();
    let table: OwnedTable = lua.load(fs::read_to_string(script)?).eval()?;
    let table_ref = table.to_ref();
    let user_fn = UserFn {
      filter_file: load_user_fn(&table_ref, "filter_file")?,
      open: load_user_fn(&table_ref, "open")?,
      close: load_user_fn(&table_ref, "close")?,
      read_data: load_user_fn(&table_ref, "read_data")?.ok_or(
        anyhow!("read_data not defined in user script")
      )?,
      read_metadata: load_user_fn(&table_ref, "read_metadata")?.ok_or(
        anyhow!("read_metadata not defined in user script")
      )?
    };

    let mut inode_map = HashMap::new();
    inode_map.insert(FUSE_ROOT_ID, dir.clone().into());
    Ok(Self {
      dir,
      ttl,
      lua,
      user_fn,
      inode_map,
      filename_map: HashMap::new()
    })
  }

  /// Return (exclude, filename)
  pub fn filter_file(&mut self, parent: &OsStr, filename: &OsStr, file_type: &fuser::FileType) -> anyhow::Result<(bool, Option<LuaString>)> {
    let Some(f) = &self.user_fn.filter_file else {
      return Ok((false, None));
    };

    let table = f.call::<_, Table>(
      (self.lua.create_string(parent.as_bytes())?, self.lua.create_string(filename.as_bytes())?, serde_json::to_value(file_type)?.as_str().unwrap())
    )?;
    let exclude = table.get::<_, Option<bool>>("exclude")?.unwrap_or(false);
    let name: Option<LuaString> = table.get("filename")?;
    if let Some(n) = &name {
      if !exclude {
        self.filename_map.insert(
          (parent.to_os_string(), OsStr::from_bytes(n.as_bytes()).to_os_string()),
          filename.to_os_string()
        );
      }
    }
    Ok((exclude, name))
  }

  pub fn unmap_filename(&self, parent: &OsStr, filename: &OsStr) -> Option<&OsString> {
    self.filename_map.get(&(parent.into(), filename.into()))
  }

  pub fn read_data(&self, name: &OsStr, offset: i64, size: u32) -> mlua::Result<LuaString> {
    let data = self.user_fn.read_data.call::<_, LuaString>(
      (self.lua.create_string(name.as_bytes())?, offset, size)
    )?;
    Ok(data)
  }
  pub fn read_metadata(&self, name: &OsStr) -> anyhow::Result<fuser::FileAttr> {
    let mut attr = utils::read_attr(name)?;
    if attr.kind == fuser::FileType::RegularFile {
      let data = self.user_fn.read_metadata.call::<_, Table>(
        self.lua.create_string(name.as_bytes())?
      )?;
      let size: Option<u64> = data.get("size")?;
      if let Some(size) = size {
        attr.size = size;
        attr.blocks = (size + attr.blksize as u64 - 1) / attr.blksize as u64;
      }
    }
    Ok(attr)
  }
  pub fn open_file(&self, name: &OsStr) -> mlua::Result<()> {
    if let Some(f) = &self.user_fn.open {
      f.call::<_, bool>(
        self.lua.create_string(name.as_bytes())?
      )?;
    }
    Ok(())
  }
  pub fn close_file(&self, name: &OsStr) -> mlua::Result<()> {
    if let Some(f) = &self.user_fn.close {
      f.call::<_, bool>(
        self.lua.create_string(name.as_bytes())?
      )?;
    }
    Ok(())
  }
}

impl Filesystem for TransformFs {
  fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEntry) {
    let Some(parent_name) = self.inode_map.get(&parent) else {
      reply.error(ENOENT as i32);
      return;
    };

    // map back to original name
    // (must convert to OsString as LuaString borrows self, which conflicts with the following borrow)
    let orig_name = self.unmap_filename(parent_name, name);

    let full_name = Path::new(parent_name).join(
      orig_name.as_ref().map(|v| v.as_os_str())
        .unwrap_or(name)
    );
    match self.read_metadata(full_name.as_os_str()) {
      Ok(attr) => {
        self.inode_map.insert(attr.ino, full_name.into_os_string());
        reply.entry(&self.ttl, &attr, 0);
      },
      Err(err) => {
        warn!("Error reading file {:?}: {}", name, err);
        reply.error(EIO as i32);
        return;
      }
    };
  }

  fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.open_file(name) {
      Ok(_) => {
        // Return dummy fh and flags as we only use ino in read
        reply.opened(0, 0);
        return;
      },
      Err(err) => {
        warn!("Error opening file {:?}: {}", name, err);
      }
    }
    reply.error(EIO as i32);
  }

  fn release(
    &mut self,
    _req: &Request<'_>,
    ino: u64,
    _fh: u64,
    _flags: i32,
    _lock_owner: Option<u64>,
    _flush: bool,
    reply: fuser::ReplyEmpty,
  ) {
    let Some(name) = self.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.close_file(name) {
      Ok(_) => {
        reply.ok();
        return;
      },
      Err(err) => {
        warn!("Error closing file {:?}: {}", name, err);
        reply.error(EIO as i32);
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

    let Some(name) = self.inode_map.get(&ino).map(|v| v.to_os_string()) else {
      reply.error(ENOENT as i32);
      return;
    };

    match utils::read_dir(&name) {
      Ok(it) => {
        for (i, e) in it.enumerate().skip(offset as usize) {
          match self.filter_file(&name, &e.2, &e.1) {
            Ok((exclude, n)) => {
              if exclude {
                continue;
              }
              // offset is used by kernel for future readdir calls (should be next entry)
              if reply.add(
                e.0,
                (i+1) as i64,
                e.1,
                n.as_ref().map(|v| OsStr::from_bytes(v.as_bytes()))
                  .unwrap_or(&e.2)
              ) {
                // return true when buffer full
                break;
              }
            },
            Err(err) => {
              warn!("Error reading dir entry {:?}/{:?}: {}", name, e, err);
            }
          };
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

