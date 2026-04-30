local api = require("wali.api")

local lib = {}

lib.result = api.result

lib.schema = {}

local function deepcopy(value)
	if type(value) ~= "table" then
		return value
	end

	local out = {}
	for key, item in pairs(value) do
		out[deepcopy(key)] = deepcopy(item)
	end
	return out
end

function lib.schema.mode(default)
	local schema = { type = "string" }
	if default ~= nil then
		schema.default = default
	end
	return schema
end

function lib.schema.owner(default)
	local schema = {
		type = "object",
		props = {
			user = { type = "any" },
			group = { type = "any" },
		},
	}
	if default ~= nil then
		schema.default = default
	end
	return schema
end

function lib.schema.owner_props()
	return deepcopy(lib.schema.owner().props)
end

function lib.validation_ok(message)
	return api.result.validation():ok(message):build()
end

function lib.validation_error(message)
	return api.result.validation():fail(tostring(message or "validation failed")):build()
end

function lib.require_apply(ctx, helper)
	if ctx == nil or ctx.phase ~= "apply" then
		error((helper or "helper") .. " requires apply phase")
	end
end

function lib.mode_bits(value)
	if value == nil then
		return nil
	end

	if type(value) ~= "string" then
		error('mode must be an octal string, for example "0644"')
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

local function validate_owner_part(value, field)
	if value == nil then
		return nil
	end

	local kind = type(value)
	if kind == "string" then
		if value == "" then
			error(field .. " must not be empty")
		end
		if value:find(":", 1, true) ~= nil then
			error(field .. " must not contain ':'")
		end
		return value
	end

	if kind == "number" then
		if value < 0 or value % 1 ~= 0 then
			error(field .. " numeric id must be a non-negative integer")
		end
		return value
	end

	error(field .. " must be a user/group name string or numeric id")
end

function lib.owner(value)
	if value == nil then
		return nil
	end
	if type(value) ~= "table" then
		error("owner must be an object")
	end

	local owner = {
		user = validate_owner_part(value.user, "owner.user"),
		group = validate_owner_part(value.group, "owner.group"),
	}

	if owner.user == nil and owner.group == nil then
		return nil
	end
	return owner
end

function lib.validate_owner(value, field)
	local ok, err = pcall(lib.owner, value)
	if ok then
		return nil
	end
	return lib.validation_error((field or "owner") .. ": " .. err)
end

local function mode_owner_fields(spec)
	spec = spec or {}
	return spec.mode or "mode", spec.owner or "owner"
end

function lib.validate_mode_owner(args, spec)
	local mode_field, owner_field = mode_owner_fields(spec)

	local mode_error = lib.validate_mode(args[mode_field])
	if mode_error ~= nil then
		return mode_error
	end

	return lib.validate_owner(args[owner_field], owner_field)
end

function lib.has_mode_owner(args, spec)
	local mode_field, owner_field = mode_owner_fields(spec)
	return args[mode_field] ~= nil or lib.owner(args[owner_field]) ~= nil
end

function lib.mode_owner_opts(args, spec)
	local mode_field, owner_field = mode_owner_fields(spec)
	local opts = {}

	if args[mode_field] ~= nil then
		opts.mode = lib.mode_bits(args[mode_field])
	end

	local owner = lib.owner(args[owner_field])
	if owner ~= nil then
		opts.owner = owner
	end

	return opts
end

function lib.apply_mode_owner(ctx, result, path, args, spec)
	lib.require_apply(ctx, "apply_mode_owner")
	local mode_field, owner_field = mode_owner_fields(spec)

	if args[mode_field] ~= nil then
		result:merge(ctx.host.fs.chmod(path, lib.mode_bits(args[mode_field])))
	end

	local owner = lib.owner(args[owner_field])
	if owner ~= nil then
		result:merge(ctx.host.fs.chown(path, owner))
	end

	return result
end

function lib.write_file_opts(args)
	local opts = lib.mode_owner_opts(args)
	opts.create_parents = args.create_parents
	opts.replace = args.replace
	return opts
end

function lib.create_dir_opts(args)
	local opts = lib.mode_owner_opts(args)
	opts.recursive = args.parents
	return opts
end

function lib.copy_file_opts(args)
	local opts = lib.mode_owner_opts(args)
	opts.create_parents = args.create_parents
	opts.replace = args.replace
	opts.preserve_mode = args.preserve_mode
	return opts
end

function lib.pull_file_opts(args)
	local opts = {
		create_parents = args.create_parents,
		replace = args.replace,
	}
	if args.mode ~= nil then
		opts.mode = lib.mode_bits(args.mode)
	end
	return opts
end

function lib.owner_from_metadata(metadata)
	if metadata == nil then
		return nil
	end
	return {
		user = metadata.uid,
		group = metadata.gid,
	}
end

function lib.owner_or_preserved(explicit_owner, preserve_owner, metadata)
	local owner = lib.owner(explicit_owner)
	if owner ~= nil then
		return owner
	end
	if preserve_owner then
		return lib.owner_from_metadata(metadata)
	end
	return nil
end

function lib.tree_dir_opts(args, metadata)
	local opts = { recursive = true }
	if args.dir_mode ~= nil then
		opts.mode = lib.mode_bits(args.dir_mode)
	elseif args.preserve_mode then
		opts.mode = metadata.mode
	end
	opts.owner = lib.owner_or_preserved(args.dir_owner, args.preserve_owner, metadata)
	return opts
end

function lib.tree_copy_file_opts(args, metadata)
	local opts = {
		create_parents = true,
		replace = args.replace,
		preserve_mode = args.preserve_mode,
	}
	if args.file_mode ~= nil then
		opts.mode = lib.mode_bits(args.file_mode)
	end
	opts.owner = lib.owner_or_preserved(args.file_owner, args.preserve_owner, metadata)
	return opts
end

function lib.link_tree_dir_opts(args)
	local opts = lib.mode_owner_opts(args, { mode = "dir_mode", owner = "dir_owner" })
	opts.recursive = true
	return opts
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

function lib.command_error(output, detail)
	local message = lib.output_text(output) or lib.status_text(output and output.status)
	if detail ~= nil and detail ~= "" then
		return detail .. ": " .. message
	end
	return message
end

function lib.assert_command_ok(output, detail)
	if output ~= nil and output.ok then
		return output
	end
	error(lib.command_error(output, detail))
end

function lib.command_detail(kind, value)
	if kind == "shell" then
		return value
	end
	if value.args == nil or #value.args == 0 then
		return value.program
	end
	return value.program .. " " .. table.concat(value.args, " ")
end

function lib.validate_absolute_path(ctx, path, field)
	field = field or "path"
	if path == nil or path == "" then
		return lib.validation_error(field .. " must not be empty")
	end
	if not ctx.host.path.is_absolute(path) then
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

function lib.validate_tree_roots(ctx, src, dest)
	local src_error = lib.validate_absolute_path(ctx, src, "src")
	if src_error ~= nil then
		return src_error
	end
	local dest_error = lib.validate_absolute_path(ctx, dest, "dest")
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
	if ctx.host.path.strip_prefix(normalized_src, normalized_dest) ~= nil then
		return lib.validation_error("tree destination must not be inside source")
	end
	if ctx.host.path.strip_prefix(normalized_dest, normalized_src) ~= nil then
		return lib.validation_error("tree source must not be inside destination")
	end
	return nil
end

function lib.is_file(metadata)
	return metadata ~= nil and metadata.kind == "file"
end

function lib.is_dir(metadata)
	return metadata ~= nil and metadata.kind == "dir"
end

function lib.is_symlink(metadata)
	return metadata ~= nil and metadata.kind == "symlink"
end

local function assert_expected_dir(path, current, label)
	if current ~= nil and current.kind ~= "dir" then
		error(label .. " must be a directory: " .. path .. " is " .. current.kind)
	end
end

local function assert_expected_file(ctx, path, current, label)
	if current == nil then
		return
	end
	if current.kind == "dir" then
		error(label .. " is a directory where a file is expected: " .. path)
	end
	if current.kind == "symlink" then
		local target = ctx.host.fs.stat(path)
		if target == nil then
			return
		end
		if target.kind == "file" then
			return
		end
		if target.kind == "dir" then
			error(label .. " is a symlink to a directory where a file is expected: " .. path)
		end
		error(label .. " is a symlink to a special filesystem entry where a file is expected: " .. path)
	end
	if current.kind ~= "file" then
		error(label .. " is a special filesystem entry where a file is expected: " .. path)
	end
end

local function assert_expected_symlink(ctx, path, current, policy, label)
	if current == nil then
		return
	end

	if current.kind == "symlink" then
		local current_target = ctx.host.fs.read_link(path)
		if current_target == policy.target then
			return
		end
	end

	if not policy.replace then
		error(label .. " already exists and replace is false: " .. path)
	end
	if current.kind == "dir" then
		error("refusing to replace directory with symlink during tree operation: " .. path)
	end
	if current.kind ~= "file" and current.kind ~= "symlink" then
		error("refusing to replace special filesystem entry with symlink during tree operation: " .. path)
	end
end

function lib.assert_tree_destination(ctx, path, policy)
	policy = policy or {}
	local expect = policy.expect
	local label = policy.label or "tree destination path"
	local current = ctx.host.fs.lstat(path)

	if expect == "dir" then
		assert_expected_dir(path, current, label)
		return
	end
	if expect == "file" then
		assert_expected_file(ctx, path, current, label)
		return
	end
	if expect == "symlink" then
		assert_expected_symlink(ctx, path, current, policy, label)
		return
	end

	error("unknown tree destination expectation: " .. tostring(expect))
end

function lib.tree_destination(ctx, dest_root, relative_path)
	if relative_path == nil or relative_path == "" then
		return dest_root
	end
	return ctx.host.path.join(dest_root, relative_path)
end

function lib.ensure_dir(ctx, result, path, opts)
	lib.require_apply(ctx, "ensure_dir")
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
	lib.require_apply(ctx, "ensure_symlink")
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
