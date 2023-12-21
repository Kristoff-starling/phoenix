local socket = require("socket")
math.randomseed(socket.gettime() * 1000)

local url = "http://h2:7878"

local function apple_banana()
    local request = "/apple"
    if math.random() > 0.95 then 
        request = "/banana"
    end

    local method = "GET"
    local path = url .. request

    local headers = {}
    return wrk.format(method, path, headers, nil)
end

request = function()
    return apple_banana(url)
end