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

function lib.validate_owner(value, field)
	local ok, err = pcall(lib.owner, value)
	if ok then
		return nil
	end
	return lib.validation_error((field or "owner") .. ": " .. err)
end

function lib.is_absolute_path(path)
	return type(path) == "string" and path:sub(1, 1) == "/"
end

function lib.validate_absolute_path(path, field)
	field = field or "path"
	if path == nil or path == "" then
		return lib.validation_error(field .. " must not be empty")
	end
	if not lib.is_absolute_path(path) then
		return lib.validation_error(field .. " must be absolute")
	end
	return nil
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

function lib.dir_opts(args)
	local opts = {}
	if args.dir_mode ~= nil then
		opts.mode = lib.mode_bits(args.dir_mode)
	end
	local owner = lib.owner(args.dir_owner)
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

function lib.is_same_or_child(parent, path)
	if parent == path then
		return true
	end
	if parent == "/" then
		return path:sub(1, 1) == "/"
	end
	return path:sub(1, #parent + 1) == parent .. "/"
end

function lib.validate_tree_roots(ctx, src, dest)
	local src_error = lib.validate_absolute_path(src, "src")
	if src_error ~= nil then
		return src_error
	end
	local dest_error = lib.validate_absolute_path(dest, "dest")
	if dest_error ~= nil then
		return dest_error
	end

	local normalized_src = ctx.host.path.normalize(src)
	local normalized_dest = ctx.host.path.normalize(dest)
	if normalized_src == "/" then
		return lib.validation_error("refusing to use / as a tree source")
	end
	if normalized_dest == "/" then
		return lib.validation_error("refusing to use / as a tree destination")
	end
	if lib.is_same_or_child(normalized_src, normalized_dest) then
		return lib.validation_error("tree destination must not be inside source")
	end
	if lib.is_same_or_child(normalized_dest, normalized_src) then
		return lib.validation_error("tree source must not be inside destination")
	end
	return nil
end

function lib.tree_destination(ctx, dest_root, relative_path)
	if relative_path == nil or relative_path == "" then
		return dest_root
	end
	return ctx.host.path.join(dest_root, relative_path)
end

function lib.ensure_dir(ctx, result, path, opts)
	local current = ctx.host.fs.lstat(path)
	if current == nil then
		result:merge(ctx.host.fs.create_dir(path, opts or { recursive = true }))
		return
	end
	if current.kind ~= "dir" then
		error("expected directory at " .. path .. ", got " .. current.kind)
	end
	result:merge(ctx.host.fs.create_dir(path, opts or { recursive = true }))
end

function lib.ensure_symlink(ctx, result, link_path, target_path, replace)
	local current = ctx.host.fs.lstat(link_path)
	if current == nil then
		result:merge(ctx.host.fs.symlink(target_path, link_path))
		return
	end

	if current.kind == "symlink" then
		local current_target = ctx.host.fs.read_link(link_path)
		if current_target == target_path then
			result:unchanged(link_path, "symlink already points to target")
			return
		end
	end

	if not replace then
		error("path already exists and replace is false: " .. link_path)
	end
	if current.kind == "dir" then
		error("refusing to replace directory with symlink: " .. link_path)
	end
	if current.kind ~= "file" and current.kind ~= "symlink" then
		error("refusing to replace special filesystem entry with symlink: " .. link_path)
	end

	result:merge(ctx.host.fs.remove_file(link_path))
	result:merge(ctx.host.fs.symlink(target_path, link_path))
end

return lib
