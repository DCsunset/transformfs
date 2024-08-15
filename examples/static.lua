local M = {}

local data = "hello, world"

function M.transform(inputs)
  local outputs = {}
  for i = 1, #inputs do
    print(inputs[i])
    outputs[#outputs+1] = {
      path = inputs[i] .. ".static",
      metadata = {
        size = string.len(data)
      },
      read = function(offset, size)
        return string.sub(data, offset, size)
      end
    }
  end
  return outputs
end

return M

