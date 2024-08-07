local M = {}

local data = "hello, world"

function M.read_metadata(filename)
  print("read_metadata", filename)
  return {
     size = string.len(data)
  }
end

function M.read_data(filename, offset, size)
  print("read_data", filename, offset, size)
  return string.sub(data, offset, size)
end

return M

