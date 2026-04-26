local api = require("wali.api")

local lib = {}

lib.result = api.result

function lib.validation_error(message)
	return api.result.validation():fail(message):build()
end

function lib.mode_bits(value)
	if value == nil then
		return nil
	end

	if type(value) ~= "string" then
		error("mode must be an octal string, for example \"0644\"")
	end

	local text = value
	if text:sub(1, 2) == "0o" or text:sub(1, 2) == "0O" then
		text = text:sub(3)
	elseif text:sub(1, 1) == "0" then
		text = text:sub(2)
	end

	if text == "" then
		error("mode must not be empty")
	end

	local bits = 0
	for idx = 1, #text do
		local byte = string.byte(text, idx)
		if byte < string.byte("0") or byte > string.byte("7") then
			error("mode must be an octal string")
		end
		bits = bits * 8 + (byte - string.byte("0"))
	end

	if bits < 0 or bits > 4095 then
		error("mode must be between 0 and 07777")
	end

	return bits
end

function lib.validate_mode(value)
	local ok, err = pcall(lib.mode_bits, value)
	if ok then
		return nil
	end
	return lib.validation_error(err)
end

function lib.owner(value)
	if value == nil then
		return nil
	end
	if type(value) ~= "table" then
		error("owner must be an object")
	end
	if value.user == nil and value.group == nil then
		return nil
	end
	return value
end

function lib.validate_safe_remove_path(ctx, path)
	if path == nil or path == "" then
		return lib.validation_error("path must not be empty")
	end

	local normalized = ctx.host.path.normalize(path)
	if normalized == "" or normalized == "/" or normalized == "." or normalized == ".." then
		return lib.validation_error("refusing to remove unsafe path: " .. tostring(path))
	end

	return nil
end

function lib.fs_opts(args)
	local opts = {}
	if args.mode ~= nil then
		opts.mode = lib.mode_bits(args.mode)
	end
	local owner = lib.owner(args.owner)
	if owner ~= nil then
		opts.owner = owner
	end
	return opts
end

function lib.merge_opts(base, extra)
	local out = {}
	for key, value in pairs(base or {}) do
		out[key] = value
	end
	for key, value in pairs(extra or {}) do
		out[key] = value
	end
	return out
end

function lib.output_text(output)
	if output.stderr ~= nil and #output.stderr > 0 then
		return output.stderr
	end
	if output.output ~= nil and #output.output > 0 then
		return output.output
	end
	if output.stdout ~= nil and #output.stdout > 0 then
		return output.stdout
	end
	return nil
end

function lib.status_text(status)
	if status == nil then
		return "unknown status"
	end
	if status.kind == "exited" then
		return "exit status " .. tostring(status.code)
	end
	if status.kind == "signaled" then
		return "terminated by signal " .. tostring(status.signal)
	end
	return "unknown status"
end

function lib.command_detail(kind, value)
	if kind == "shell" then
		return value
	end
	if #value.args == 0 then
		return value.program
	end
	return value.program .. " " .. table.concat(value.args, " ")
end

return lib
