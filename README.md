# transformfs

A read-only FUSE filesystem to transform the content of files with Lua.

In transformfs, the content of files can be transformed by user-defined Lua scripts
while preserving the same directory structure.

## Usage

``` shell
# mount transformfs
transformfs -s <lua_script> <src_dir> <mnt_point>

# umount
fusermount -u <mnt_point>
```

The Lua script must define the following functions:
- `read_metadata(filename)`: Return the metadata of the file as a table. `size` can be set if a user wants to change the size.
- `read_data(filename, offset, size)`: Return the content of the file as string at a specific position.


## License

AGPL-3.0

