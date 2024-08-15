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

use std::{ffi::OsString, path::Path};
use log::warn;
use walkdir::WalkDir;

// read all files under a path
pub fn read_files(root: impl AsRef<Path>) -> impl Iterator<Item = OsString> {
  WalkDir::new(root)
    .into_iter()
    .filter_map(|r| {
      match r {
        Ok(e) => {
          if e.path().is_dir() {
            None
          } else {
            Some(e.path().as_os_str().to_os_string())
          }
        },
        Err(err) => {
          warn!("error reading entry: {}", err);
          None
        }
      }
    })
}
