# transformfs

[![Crates.io Version](https://img.shields.io/crates/v/transformfs)](https://crates.io/crates/transformfs)

A read-only FUSE filesystem to transform input files to output files with Lua script.

In transformfs, the input files can be transformed on demand by the user-defined Lua script.
The inputs are a list of files passed to the user script,
and the user script returns a list of files to generate dynamically.

This filesystem is useful to transform data without duplicating or modifying the original files.

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
transformfs -s <lua_script> [-i <input1>...] <mnt_point>

# umount
fusermount -u <mnt_point>
```

The inputs can be zero, one, or multiple files or directories (directories are resolved to individual files).

The user Lua script must return a module (table) with the following functions as its fields:
- `transform(inputs)`: Function to transform inputs (a list of file paths) to outputs. It should return a list of `Output`.

Each `Output` is table with the following fields:
- `path`: Path of the file (parent directories are auto created if path contains them)
- `metadata`: Return the metadata of the file as `FileMetadata`.
- `open()`: (optional) Called when opening a file if defined. Useful to open the file in advance for performance
- `close()`: (optional) Called when closing a file if defined. Useful to reclaim resources
- `read(offset, size)`: Return the content of the file as string at a specific position.

`FileMetadata` fields:
- `size`: Size of the file
- `block_size`: (optional) Block size of the file (default: 512)


Transformfs uses LuaJIT for performance reason as Lua code is executed very frequently for large files.
Thus it may not support new features in Lua 5.3 or 5.4 at the time of writing.

See example scripts in `examples` directory for more details and `transformfs --help`.


## License

AGPL-3.0

