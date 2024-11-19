local M = {}

local filter_patterns = {
  "^test/include",
  "^test/hello"
}

function M.transform(inputs)
  local outputs = {}
  for _, input in ipairs(inputs) do
    local matched = false
    -- match any pattern
    for _, pattern in ipairs(filter_patterns) do
      if string.find(input, pattern) then
        matched = true
        break
      end
    end
    if not matched then
      goto continue
    end

    local state = {
      file = nil,
      file_handles = 0
    }

    outputs[#outputs + 1] = {
      path = input,
      metadata = {
        size = io.open(input):seek("end")
      },

      open = function()
        if state.file_handles == 0 then
          state.file = assert(io.open(input, "r"))
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
        state.file:seek("set", offset)
        return state.file:read(size)
      end
    }
    ::continue::
  end
  return outputs
end

return M

