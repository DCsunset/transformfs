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

The Lua script must return a module with the following functions:
- `open(filename)`: (optional) Called when opening a file if defined. Useful to open the file in advance for performance
- `close(filename)`: (optional) Called when closing a file if defined. Useful to reclaim resources
- `read_metadata(filename)`: Return the metadata of the file as a table. `size` can be set if a user wants to change the size.
- `read_data(filename, offset, size)`: Return the content of the file as string at a specific position.

Transformfs uses LuaJIT for performance reason as Lua code is executed very frequently for large files.
Thus it may not support new features in Lua 5.3 or 5.4 at the time of writing.

See example scripts in `examples` directory.


## License

AGPL-3.0

