local M = {
  states = {}
};

local function find_block(blocks, offset)
  local l = 1
  local r = #blocks
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


local function read_from_blocks(file, blocks, offset, size)
  local i = find_block(blocks, offset)
  local b = blocks[i]
  assert(b.offset <= offset)

  local off = offset - b.offset
  if off >= b.size then
    -- offset out of bound
    return nil
  end

  local data = "";
  while size > 0 and i <= #blocks do
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

function M.transform(inputs)
  local output = {}
  for i = 1, #inputs do
    local name = inputs[i]
    local state = record_line_offset(name)
    M.states[name] = state

    output[#output+1] = {
      path = name .. ".txt",
      metadata = {
        size = state.file_size
      },

      open = function()
        if state.file_handles == 0 then
          state.file = assert(io.open(name, "r"))
        end
        state.file_handles = state.file_handles + 1
      end,

      close = function()
        state.file_handles = state.file_handles - 1
        if state.file_handles == 0 then
          state.file:close()
          state.file = nil
        end
      end,

      read = function(offset, size)
        return read_from_blocks(state.file, state.blocks, offset, size)
      end
    }
  end
  return output
end

return M
