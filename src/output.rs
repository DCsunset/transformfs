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

use std::{collections::HashMap, ffi::OsString, os::unix::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}};

use fuser::FUSE_ROOT_ID;
use log::{debug, error, info};
use mlua::{FromLua, Function, Lua, String as LuaString};

use crate::utils;

pub struct OutputFileMetadata {
  pub size: u64,
  pub block_size: Option<u32>
}

impl FromLua for OutputFileMetadata {
  fn from_lua(value: mlua::Value, _lua: &Lua) -> mlua::Result<Self> {
    let mlua::Value::Table(table) = &value else {
      return Err(mlua::Error::runtime("OutputFileMetadata must be a Lua table"));
    };
    Ok(OutputFileMetadata {
      size: table.get("size")?,
      block_size: table.get("block_size")?
    })
  }
}

pub struct OutputFile {
  pub metadata: OutputFileMetadata,
  pub open: Option<Function>,
  pub close: Option<Function>,
  pub read: Function
}

pub struct OutputDirEntry {
  pub ino: u64,
  pub name: OsString,
  pub kind: fuser::FileType
}

pub enum OutputContent {
  File(OutputFile),
  Dir(Vec<OutputDirEntry>)
}

pub struct OutputEntry {
  pub path: OsString,
  pub content: OutputContent
}

impl FromLua for OutputEntry {
  fn from_lua(value: mlua::Value, _lua: &Lua) -> mlua::Result<Self> {
    let mlua::Value::Table(table) = &value else {
      return Err(mlua::Error::runtime("File must be a Lua table"));
    };
    let path = OsString::from_vec(
      table.get::<_, LuaString>("path")?.as_bytes().to_vec()
    );
    // normalize path
    let path = Path::new(&path).as_os_str().to_os_string();
    Ok(OutputEntry {
      path,
      content: OutputContent::File(OutputFile {
        metadata: table.get("metadata")?,
        open: table.get("open")?,
        close: table.get("close")?,
        read: table.get("read")?
      })
    })
  }
}

pub struct Output {
  /// Map inode to output
  pub inode_map: HashMap<u64, OutputEntry>,
  /// Map file path to inode
  pub path_map: HashMap<OsString, u64>,
}

impl Output {
  fn lookup_path_with_map<'a>(inode_map: &'a HashMap<u64, OutputEntry>, path_map: &'a HashMap<OsString, u64>, path: &OsString) -> Option<(u64, &'a OutputEntry)> {
    path_map.get(path)
      .map(|ino| (ino.clone(), inode_map.get(ino).expect("Path in path_map but ino not in inode_map")))
  }

  fn lookup_path_with_map_mut<'a>(inode_map: &'a mut HashMap<u64, OutputEntry>, path_map: &'a HashMap<OsString, u64>, path: &OsString) -> Option<(u64, &'a mut OutputEntry)> {
    path_map.get(path)
      .map(|ino| (ino.clone(), inode_map.get_mut(ino).expect("Path in path_map but ino not in inode_map")))
  }

  fn append_dir_entry(inode_map: &mut HashMap<u64, OutputEntry>, path_map: &HashMap<OsString, u64>, dir_path: &OsString, entry: OutputDirEntry) -> bool {
    let Some((_, parent_entry)) = Output::lookup_path_with_map_mut(inode_map, &path_map, &dir_path) else {
      error!("Appending to non-existent dir: {:?}", dir_path);
      return false;
    };
    let OutputContent::Dir(parent_dir) = &mut parent_entry.content else {
      panic!("Appending to a file: {:?}", dir_path);
    };
    parent_dir.push(entry);
    return true;
  }

  pub fn lookup_path(&self, path: &OsString) -> Option<(u64, &OutputEntry)> {
    Output::lookup_path_with_map(&self.inode_map, &self.path_map, path)
  }

  // transform input to output
  pub fn init(lua: &Lua, function: &Function, input: &Vec<PathBuf>) -> anyhow::Result<Output> {
    let mut inode_map: HashMap<u64, OutputEntry> = HashMap::new();
    let mut path_map = HashMap::new();

    // Expand input to input files as Lua doesn't support dir
    let input_files = input.iter()
      .flat_map(utils::read_files)
      .map(|v| lua.create_string(v.as_bytes()))
      .collect::<Result<Vec<LuaString>, _>>()?;

    let output_files: Vec<OutputEntry> = function.call(input_files).map_err(
      |e| anyhow::anyhow!("Invalid Output from transform: {}", e)
    )?;
    // root entry
    inode_map.insert(FUSE_ROOT_ID, OutputEntry {
      path: OsString::from("/"),
      content: OutputContent::Dir(Vec::new())
    });
    path_map.insert(OsString::from("/"), FUSE_ROOT_ID);
    info!("Output {} file(s)", output_files.len());

    // current inode
    let mut ino = FUSE_ROOT_ID + 1;
    for f in output_files {
      debug!("Processing output file: {:?}", f.path);
      let mut path = PathBuf::new();
      // normallize
      path.push("/");
      path.push(f.path.clone());
      let mut it = path.components().peekable();
      let mut cur_path = PathBuf::new();

      while let Some(c) = it.next() {
        let parent_str = cur_path.clone().into_os_string();
        cur_path.push(c);
        let cur_path_str = cur_path.as_os_str().to_os_string();
        if c == std::path::Component::RootDir {
          continue;
        }

        if it.peek().is_none() {
          let ok = Output::append_dir_entry(
            &mut inode_map,
            &path_map,
            &parent_str,
            OutputDirEntry {
              ino,
              kind: fuser::FileType::RegularFile,
              name: c.as_os_str().to_os_string()
            }
          );
          if !ok {
            break;
          }

          inode_map.insert(ino, f);
          path_map.insert(cur_path_str, ino);
          ino += 1;
          break;
        }
        else {
          let entry_ino = match path_map.get(&cur_path_str) {
            Some(i) => i.clone(),
            None => {
              path_map.insert(cur_path_str.clone(), ino);
              let ok = Output::append_dir_entry(
                &mut inode_map,
                &path_map,
                &parent_str,
                OutputDirEntry {
                  ino,
                  kind: fuser::FileType::Directory,
                  name: c.as_os_str().to_os_string()
                }
              );
              if !ok {
                break;
              }

              ino += 1;
              ino - 1
            }
          };
          let entry = inode_map.entry(entry_ino).or_insert_with(|| OutputEntry {
            path: cur_path_str.clone(),
            content: OutputContent::Dir(Vec::new())
          });
          if let OutputContent::File(_) = entry.content {
            error!("Failed to add dir {:?}: used by a file", cur_path);
            break;
          }
        }
      }
    }

    Ok(Output {
      inode_map,
      path_map
    })
  }
}

