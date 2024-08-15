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

use log::{error, info};
use fuser::{Filesystem, Request};
use mlua::{FromLua, Function, Lua, String as LuaString, Table};
use std::{ffi::OsStr, fs, path::{Path, PathBuf}, time::{Duration, SystemTime}
};
use nix::errno::Errno::{EIO, ENOENT};
use crate::output::{Output, OutputContent, OutputEntry};

fn load_fn(table: &Table, name: &str) -> mlua::Result<Option<Function>> {
  Ok(
    if table.contains_key(name)? {
      Some(table.get::<_, Function>(name)?)
    } else {
      None
    }
  )
}

struct UserFn {
  transform: Function
}

impl FromLua for UserFn {
  fn from_lua(value: mlua::Value, _lua: &Lua) -> mlua::Result<Self> {
    let mlua::Value::Table(table) = &value else {
      return Err(mlua::Error::runtime("User script must export a Lua table"));
    };
    Ok(UserFn {
      transform: load_fn(table, "transform")?.ok_or(
        mlua::Error::runtime("transform not defined in user module")
      )?
    })
  }
}

pub struct Config {
  pub timeout: Duration,
}

pub struct TransformFs {
  inputs: Vec<PathBuf>,
  config: Config,

  /// Lua state with loaded script
  lua: Lua,
  /// Loaded user-defined function
  user_fn: UserFn,
  /// Last updated time
  last_updated: SystemTime,

  /// Output
  output: Output,

  default_attr: fuser::FileAttr
}

impl TransformFs {
  pub fn init(inputs: Vec<PathBuf>, script: PathBuf, config: Config) -> anyhow::Result<Self> {
    let lua = Lua::new();
    let user_fn: UserFn = lua.load(fs::read_to_string(script)?).eval()?;
    let output = Output::init(&lua, &user_fn.transform, &inputs)?;
    let cur_time = SystemTime::now();
    Ok(Self {
      inputs,
      config,
      lua,
      user_fn,
      last_updated: cur_time,
      output,
      default_attr: fuser::FileAttr {
        // must be overwritten
        ino: 0,
        size: 0,
        blocks: 0,
        kind: fuser::FileType::RegularFile,
        perm: 0,

        // default
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
        blksize: 512,
        nlink: 1,
        atime: cur_time,
        mtime: cur_time,
        ctime: cur_time,
        crtime: cur_time,
        rdev: 0,
        flags: 0
      }
    })
  }

  /// Update output when timeout
  pub fn update(&mut self) {
    match self.last_updated.elapsed() {
      Ok(elapsed) => {
        if elapsed <= self.config.timeout {
          return;
        }
      },
      Err(err) => {
        error!("Failed to get system time: {}", err);
        return;
      }
    };

    info!("Update output on timeout");
    match Output::init(&self.lua, &self.user_fn.transform, &self.inputs) {
      Ok(output) => self.output = output,
      Err(err) => error!("{}", err)
    };
  }

  pub fn read_metadata(&self, ino: u64, entry: &OutputEntry) -> anyhow::Result<fuser::FileAttr> {
    Ok(match &entry.content {
      OutputContent::File(f) => {
        let size = f.metadata.size;
        let blksize = f.metadata.block_size.unwrap_or(self.default_attr.blksize);
        fuser::FileAttr {
          ino,
          kind: fuser::FileType::RegularFile,
          size,
          blksize,
          perm: 0o644,
          blocks: (size + blksize as u64 - 1) / blksize as u64,
          ..self.default_attr
        }
      },
      OutputContent::Dir(_) => {
        // TODO: calculate size
        fuser::FileAttr {
          ino,
          kind: fuser::FileType::Directory,
          perm: 0o755,
          ..self.default_attr
        }
      }
    })
  }
}

impl Filesystem for TransformFs {
  fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEntry) {
    self.update();

    let Some(parent_entry) = self.output.inode_map.get(&parent) else {
      reply.error(ENOENT as i32);
      return;
    };

    let path = Path::new(&parent_entry.path).join(name).into_os_string();
    let Some((ino, entry)) = self.output.lookup_path(&path) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.read_metadata(ino, entry) {
      Ok(attr) => {
        reply.entry(&self.config.timeout, &attr, 0);
      },
      Err(err) => {
        error!("Error reading metadata of file {:?}: {}", entry.path, err);
        reply.error(EIO as i32);
      }
    };
  }

  fn getattr(&mut self, _req: &Request, ino: u64, reply: fuser::ReplyAttr) {
    self.update();

    let Some(entry) = self.output.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match self.read_metadata(ino, entry) {
      Ok(attr) => {
        reply.attr(&self.config.timeout, &attr);
      },
      Err(err) => {
        error!("Error reading metadata of file {:?}: {}", entry.path, err);
        reply.error(EIO as i32);
      }
    };
  }

  fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
    let Some(entry) = self.output.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match &entry.content {
      OutputContent::File(f) => {
        if let Some(open) = &f.open {
          if let Err(err) = open.call::<_, ()>(()) {
            error!("Error opening file {:?}: {}", entry.path, err);
            reply.error(EIO as i32);
            return;
          }
        }
        // return dummy fh and flags as we only use ino in read
        reply.opened(0, 0);
      },
      OutputContent::Dir(_) => {
        error!("Trying to open a dir {:?}", entry.path);
        reply.error(EIO as i32);
      }
    }
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
    let Some(entry) = self.output.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match &entry.content {
      OutputContent::File(f) => {
        if let Some(close) = &f.close {
          if let Err(err) = close.call::<_, ()>(()) {
            error!("Error closing file {:?}: {}", entry.path, err);
            reply.error(EIO as i32);
            return;
          }
        }
        reply.ok();
      },
      OutputContent::Dir(_) => {
        error!("Calling close on a dir {:?}", entry.path);
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
    mut reply: fuser::ReplyDirectory,
  ) {
    assert!(offset >= 0);

    self.update();

    let Some(entry) = self.output.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match &entry.content {
      OutputContent::Dir(entries) => {
        // offset is used by kernel for future readdir calls (should be next entry)
        for (i, e) in entries.iter().enumerate().skip(offset as usize) {
          // return true when buffer full
          if reply.add(e.ino, (i+1) as i64, e.kind, &e.name) {
            break;
          }
        }
        reply.ok();
      },
      OutputContent::File(_) => {
        error!("Calling readdir on a file: {:?}", entry.path);
        reply.error(EIO as i32);
      }
    }
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

    let Some(entry) = self.output.inode_map.get(&ino) else {
      reply.error(ENOENT as i32);
      return;
    };

    match &entry.content {
      OutputContent::File(f) => {
        match f.read.call::<_, LuaString>((offset, size)) {
          Ok(data) => {
            // HACK: as_bytes not available yet
            reply.data(&data.as_bytes().to_vec());
          },
          Err(err) => {
            error!("Error reading file {:?}: {}", entry.path, err);
            reply.error(EIO as i32);
          },
        };
      },
      OutputContent::Dir(_) => {
        error!("Calling read on a dir {:?}", entry.path);
        reply.error(EIO as i32);
      }
    };
  }

  fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
    reply.statfs(
      0,
      0,
      0,
      self.output.inode_map.len() as u64,
      0,
      512,
      255,
      512
    )
  }
}

