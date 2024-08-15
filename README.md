# transformfs

[![Crates.io Version](https://img.shields.io/crates/v/transformfs)](https://crates.io/crates/transformfs)

A read-only FUSE filesystem to transform the content of files with Lua.

In transformfs, the content of files can be transformed on demand by user-defined Lua scripts
while preserving the same directory structure.

This filesystem is useful to transform data without duplicating the original files.

## Installation

### Cargo

```shell
cargo install transformfs
```

### Nix

Transformfs is also packaged as an NUR package `nur.repos.dcsunset.transformfs`.
You can install it by including it in your nix config.



## Usage

``` shell
# mount transformfs
transformfs -s <lua_script> <src_dir> <mnt_point>

# umount
fusermount -u <mnt_point>
```

The Lua script must return a module (table) with the following functions as its fields:
- `transform(input_files)`: Function to transform inputs (a list of strings) to outputs. It should return `Output`.

`Output` is a list of tables with the following fields:
- `path`: Path of the file
- `metadata`: Return the metadata of the file as `FileMetadata`.
- `open()`: (optional) Called when opening a file if defined. Useful to open the file in advance for performance
- `close()`: (optional) Called when closing a file if defined. Useful to reclaim resources
- `read(offset, size)`: Return the content of the file as string at a specific position.

`FileMetadata` fields:
- `size`: Size of the file
- `block_size`: (optional) Block size of the file. (default: 512)


Transformfs uses LuaJIT for performance reason as Lua code is executed very frequently for large files.
Thus it may not support new features in Lua 5.3 or 5.4 at the time of writing.

See example scripts in `examples` directory.


## License

AGPL-3.0

