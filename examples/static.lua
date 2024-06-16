local data = "hello, world"

function read_metadata(name)
  return {
     size = string.len(data)
  }
end

function read_data(name, offset, size)
  return string.sub(data, offset, size)
end

