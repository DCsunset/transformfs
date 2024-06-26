local M = {
  states = {}
};

local function find_block(blocks, blocks_len, offset)
  local l = 1
  local r = blocks_len
  while true do
    if l >= r then
      return l
    end

    local m = math.floor((l + r + 1) / 2)
    if blocks[m].offset == offset then
      return m
    elseif blocks[m].offset > offset then
      r = m - 1
    else
      l = m
    end
  end
end

local function record_line_offset(filename)
   local line_n = 1
   local i = 1
   local src_off = 0
   local off = 0
   local blocks = {}
   for line in io.lines(filename) do
      -- block of the line number wit a space (src_offset is nil)
      blocks[i] = {
        kind = "line_num",
        offset = off,
        line_num = line_n,
        size = string.len(tostring(line_n)) + 1
      }
      off = off + blocks[i].size
      i = i + 1

      -- block of the actual line
      local len = string.len(line) + 1
      -- add length of the line number
      blocks[i] = {
        kind = "file",
        src_offset = src_off,
        offset = off,
        line_num = line_n,
        size = len
      }
      src_off = src_off + len
      off = off + len
      i = i + 1

      line_n = line_n + 1
   end
   return {
     blocks = blocks;
     blocks_len = i - 1;
     file_size = off - 1;
     -- file handle
     file = nil;
     -- num of opened handles
     file_handles = 0;
   }
end

local function read_block(file, block, offset, size)
  if block.kind == "line_num" then
    local data = tostring(block.line_num) .. " "
    return string.sub(data, offset + 1, offset + size)
  elseif block.kind == "file" then
    file:seek("set", block.src_offset)
    return file:read(size)
  else
    error("Invalid block kind: ".. block.kind)
  end
end


local function read_from_blocks(file, blocks, blocks_len, offset, size)
  local i = find_block(blocks, blocks_len, offset)
  local b = blocks[i]
  assert(b.offset <= offset)

  local off = offset - b.offset
  if off >= b.size then
    -- offset out of bound
    return nil
  end

  local data = "";
  while size > 0 and i <= blocks_len do
    local s = b.size - off
    local size_to_read = math.min(s, size)
    data = data .. read_block(file, b, off, size_to_read)
    size = size - size_to_read
    off = 0
    i = i + 1
    b = blocks[i]
  end

  return data
end

function M:map_filename(_, filename)
  return filename .. ".txt"
end

function M:unmap_filename(_, filename)
  return string.sub(filename, 0, -5)
end

function M:open(filename)
  local state = self.states[filename]
  if state.file_handles == 0 then
    state.file = assert(io.open(filename, "r"))
  end
  state.file_handles = state.file_handles + 1
end

function M:close(filename)
  local state = self.states[filename]
  state.file_handles = state.file_handles - 1
  if state.file_handles == 0 then
    state.file:close()
    state.file = nil
  end
end

function M:read_metadata(filename)
  local state = record_line_offset(filename)
  self.states[filename] = state
  return {
    size = state.file_size
  }
end

function M:read_data(filename, offset, size)
  local state = self.states[filename]
  local data = read_from_blocks(state.file, state.blocks, state.blocks_len, offset, size)
  return data
end

return M
