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

use std::{
  ffi::OsString, fs, io, os::unix::fs::{DirEntryExt, FileTypeExt, MetadataExt}, path::Path, time::{Duration, SystemTime, UNIX_EPOCH}
};
use log::warn;
use fuser;

fn system_time_from_time(secs: i64, nsecs: u32) -> SystemTime {
  if secs >= 0 {
    UNIX_EPOCH + Duration::new(secs as u64, nsecs)
  } else {
    UNIX_EPOCH - Duration::new((-secs) as u64, nsecs)
  }
}

/// Convert fs::FileType -> fuser::FileType
pub fn convert_file_type(file_type: fs::FileType) -> io::Result<fuser::FileType> {
  Ok(
    if file_type.is_dir() {
      fuser::FileType::Directory
    } else if file_type.is_file() {
      fuser::FileType::RegularFile
    } else if file_type.is_symlink() {
      fuser::FileType::Symlink
    } else if file_type.is_socket() {
      fuser::FileType::Socket
    } else if file_type.is_char_device() {
      fuser::FileType::CharDevice
    } else if file_type.is_block_device() {
      fuser::FileType::BlockDevice
    } else if file_type.is_fifo() {
      fuser::FileType::NamedPipe
    } else {
      return Err(io::Error::new(io::ErrorKind::Unsupported, "Unsupported file type"))
    }
  )
}

pub fn read_attr(path: impl AsRef<Path>) -> io::Result<fuser::FileAttr> {
  let attr = fs::metadata(path)?;
  Ok(fuser::FileAttr {
    ino: attr.ino(),
    size: attr.size(),
    blocks: (attr.size() + attr.blksize() - 1) / attr.blksize(),
    atime: system_time_from_time(attr.atime(), attr.atime_nsec() as u32),
    mtime: system_time_from_time(attr.mtime(), attr.mtime_nsec() as u32),
    ctime: system_time_from_time(attr.ctime(), attr.ctime_nsec() as u32),
    crtime: UNIX_EPOCH,
    kind: convert_file_type(attr.file_type())?,
    perm: attr.mode() as u16,
    nlink: attr.nlink() as u32,
    uid: attr.uid(),
    gid: attr.gid(),
    rdev: 0,
    blksize: attr.blksize() as u32,
    flags: 0,
  })
}

pub fn read_dir(dir: impl AsRef<Path>) -> io::Result<impl Iterator<Item = (u64, fuser::FileType, OsString)>> {
	// Ignore files that can't be read
  Ok(
    fs::read_dir(dir)?
    .filter_map(|res| {
			match res {
				Ok(e) => {
          match e.file_type().and_then(convert_file_type) {
            Ok(file_type) => Some((e.ino(), file_type, e.file_name())),
            Err(err) => {
              warn!("error parsing file type of {:?}: {}", e.file_name(), err);
              None
            }
          }
        }
				Err(err) => {
					warn!("error reading entry: {}", err);
					None
				}
			}
    })
  )
}


