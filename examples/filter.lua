local M = {}

local filter_pattern = "^test/include"

function M.transform(inputs)
  local outputs = {}
  for i = 1, #inputs do
    local path = inputs[i]
    print(path)
    if not string.find(path, filter_pattern) then
      goto continue
    end

    local state = {
      file = nil,
      file_handles = 0
    }

    outputs[#outputs + 1] = {
      path = path,
      metadata = {
        size = io.open(path):seek("end")
      },

      open = function()
        if state.file_handles == 0 then
          state.file = assert(io.open(path, "r"))
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

