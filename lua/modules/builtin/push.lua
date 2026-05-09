local lib = require("wali.builtin.lib")

local function resolved_controller_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function validate_metadata(args)
	local err = lib.validate_mode_owner(args, { mode = "mode", owner = "owner" })
	if err ~= nil then
		return err
	end
	if not args.recursive then
		return nil
	end
	for _, spec in ipairs({
		{ mode = "dir_mode", owner = "dir_owner" },
		{ mode = "file_mode", owner = "file_owner" },
	}) do
		err = lib.validate_mode_owner(args, spec)
		if err ~= nil then
			return err
		end
	end
	return nil
end

local function source_metadata(ctx, args, follow)
	if args.src == "" then
		return nil, "src must not be empty"
	end
	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, args.src, { follow = follow })
	if not ok then
		return nil, metadata_or_error
	end
	local metadata = metadata_or_error
	if metadata == nil then
		return nil, "push source does not exist: " .. resolved_controller_path(ctx, args.src)
	end
	return metadata, nil
end

local function validate_recursive_roots(ctx, args)
	local resolved = resolved_controller_path(ctx, args.src)
	local ok_parent, parent = pcall(ctx.controller.path.parent, resolved)
	if ok_parent and parent == nil then
		return lib.validation_error("refusing to use controller filesystem root as push source")
	end
	if ctx.host.path.normalize(args.dest) == "/" then
		return lib.validation_error("refusing to use / as push destination")
	end
	return nil
end

local function check_recursive_dest(ctx, args, entry)
	local path = lib.tree_destination(ctx, args.dest, entry.relative_path)
	if entry.kind == "dir" then
		local current = ctx.host.fs.lstat(path)
		if current == nil or current.kind == "dir" then
			return nil
		end
		if not args.replace then
			return "destination already exists and replace is false: " .. path
		end
		error("tree destination path must be a directory: " .. path .. " is " .. current.kind)
	end

	if entry.kind == "symlink" and args.symlinks ~= "preserve" then
		return nil
	end
	if entry.kind ~= "file" and entry.kind ~= "symlink" then
		return nil
	end

	local current = ctx.host.fs.lstat(path)
	if current == nil then
		return nil
	end
	if not args.replace then
		return "destination already exists and replace is false: " .. path
	end
	if current.kind == "dir" then
		error("refusing to replace directory during push: " .. path)
	end
	if current.kind ~= "file" and current.kind ~= "symlink" then
		error("refusing to replace special filesystem entry during push: " .. path)
	end
	return nil
end

return {
	name = "builtin push",
	description = "Transfer a file or directory tree from the controller to the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			recursive = { type = "boolean", default = false },
			preserve_mode = { type = "boolean", default = true },
			symlinks = { type = "enum", values = { "preserve", "skip", "error" }, default = "preserve" },
			skip_special = { type = "boolean", default = false },
			max_depth = { type = "integer" },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
			dir_mode = lib.schema.mode(),
			file_mode = lib.schema.mode(),
			dir_owner = lib.schema.owner(),
			file_owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local dest_error = lib.validate_absolute_path(ctx, args.dest, "dest")
		if dest_error ~= nil then
			return dest_error
		end

		local metadata_error = validate_metadata(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		if args.recursive then
			local max_depth_error = lib.validate_max_depth(args.max_depth)
			if max_depth_error ~= nil then
				return max_depth_error
			end
			local root_error = validate_recursive_roots(ctx, args)
			if root_error ~= nil then
				return root_error
			end
		end

		local metadata, source_error = source_metadata(ctx, args, not args.recursive)
		if source_error ~= nil then
			return lib.validation_error(source_error)
		end

		if args.recursive then
			if metadata and metadata.kind ~= "dir" then
				return lib.validation_error(
					"push recursive source must be a directory: " .. resolved_controller_path(ctx, args.src)
				)
			end
		else
			if metadata and metadata.kind ~= "file" then
				return lib.validation_error(
					"push source must be a regular file unless recursive=true: "
						.. resolved_controller_path(ctx, args.src)
				)
			end
		end

		return nil
	end,

	apply = function(ctx, args)
		local metadata, source_error = source_metadata(ctx, args, not args.recursive)
		if source_error ~= nil then
			error(source_error)
		end

		if not args.recursive then
			if metadata and metadata.kind ~= "file" then
				error(
					"push source must be a regular file unless recursive=true: "
						.. resolved_controller_path(ctx, args.src)
				)
			end
			local skipped = lib.skip_if_replace_false_and_exists(ctx, args.dest, args.replace, "destination")
			if skipped ~= nil then
				return skipped
			end
			return ctx.transfer.push_file(args.src, args.dest, lib.write_file_opts(args))
		end

		if metadata and metadata.kind ~= "dir" then
			error("push recursive source must be a directory: " .. resolved_controller_path(ctx, args.src))
		end
		local root = ctx.host.fs.lstat(args.dest)
		if root ~= nil and root.kind ~= "dir" then
			if not args.replace then
				return lib.skip("destination already exists and replace is false: " .. args.dest)
			end
			error("push destination root must be a directory: " .. args.dest .. " is " .. root.kind)
		end

		local entries = ctx.controller.fs.walk(args.src, {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		})
		for _, entry in ipairs(entries) do
			local reason = check_recursive_dest(ctx, args, entry)
			if reason ~= nil then
				return lib.skip(reason)
			end
		end

		return ctx.transfer.push_tree(args.src, args.dest, lib.push_tree_opts(args))
	end,
}
