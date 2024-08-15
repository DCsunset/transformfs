local M = {
  state = {
    inputs = {},
    open_count = 0,
  }
}

local function open_inputs()
  if M.state.open_count == 0 then
    for i = 1, #M.state.inputs do
      M.state.inputs[i].file = assert(io.open(M.state.inputs[i].path, "r"))
    end
  end
  M.state.open_count = M.state.open_count + 1
end

local function close_inputs()
  if M.state.open_count == 0 then
    return
  end
  M.state.open_count = M.state.open_count - 1
  if M.state.open_count == 0 then
    for i = 1, #M.state.inputs do
      M.state.inputs[i].file:close()
      M.state.inputs[i].file = nil
    end
  end
end

local function reset_state()
  if M.state.open_count > 0 then
    M.state.open_count = 1
  end
  close_inputs()
  M.state.inputs = {}
end

local function find_input(offset)
  local inputs = M.state.inputs
  local l = 1
  local r = #inputs
  while true do
    if l >= r then
      return l
    end

    local m = math.floor((l + r + 1) / 2)
    if inputs[m].offset == offset then
      return m
    elseif inputs[m].offset > offset then
      r = m - 1
    else
      l = m
    end
  end
end


local function read_inputs(offset, size)
  local inputs = M.state.inputs
  local i = find_input(offset)
  local input = inputs[i]
  assert(input.offset <= offset)

  local off = offset - input.offset
  if off >= input.size then
    -- offset out of bound
    return nil
  end

  local data = "";
  while size > 0 and i <= #inputs do
    local s = input.size - off
    local size_to_read = math.min(s, size)

    input.file:seek("set", off)
    data = data .. input.file:read(size)
    size = size - size_to_read
    off = 0
    i = i + 1
    input = inputs[i]
  end

  return data

end

function M.transform(inputs)
  -- reset state when reloading
  reset_state()

  local output = {}
  local output_size = 0
  M.state.inputs = inputs

  for i = 1, #inputs do
    local size = io.open(inputs[i]):seek("end")
    M.state.inputs[i] = {
      path = inputs[i],
      size = size,
      offset = output_size,
      file = nil
    }
    output_size = output_size + size
  end

  output = {
    path = "output",
    metadata = { size = output_size },
    open = open_inputs,
    close = close_inputs,
    read = read_inputs
  }
  return { output }
end

return M

