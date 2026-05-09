local lib = require("wali.builtin.lib")

local function resolved_controller_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function controller_exists(ctx, path)
	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, path, { follow = false })
	if not ok then
		error(metadata_or_error)
	end
	return metadata_or_error ~= nil, metadata_or_error
end

local function validate_destination(ctx, dest)
	if dest == "" then
		return lib.validation_error("dest must not be empty")
	end

	local resolved = resolved_controller_path(ctx, dest)
	local ok_parent, parent = pcall(ctx.controller.path.parent, resolved)
	if ok_parent and parent == nil then
		return lib.validation_error("refusing to use controller filesystem root as transfer destination")
	end
	return nil
end

local function validate_modes(args)
	local err = lib.validate_mode(args.mode)
	if err ~= nil then
		return err
	end
	if not args.recursive then
		return nil
	end
	for _, field in ipairs({ "dir_mode", "file_mode" }) do
		err = lib.validate_mode(args[field])
		if err ~= nil then
			return err
		end
	end
	return nil
end

local function check_recursive_dest(ctx, args, entry)
	local path = ctx.controller.path.join(args.dest, entry.relative_path)
	local exists, metadata = controller_exists(ctx, path)
	if entry.kind == "dir" then
		if not exists or metadata.kind == "dir" then
			return nil
		end
		if not args.replace then
			return "destination already exists and replace is false: " .. resolved_controller_path(ctx, path)
		end
		error(
			"tree destination path must be a directory: "
				.. resolved_controller_path(ctx, path)
				.. " is "
				.. metadata.kind
		)
	end

	if entry.kind == "symlink" and args.symlinks ~= "preserve" then
		return nil
	end
	if entry.kind ~= "file" and entry.kind ~= "symlink" then
		return nil
	end
	if exists and not args.replace then
		return "destination already exists and replace is false: " .. resolved_controller_path(ctx, path)
	end
	if exists and metadata.kind == "dir" then
		error("refusing to replace directory during pull: " .. resolved_controller_path(ctx, path))
	end
	return nil
end

return {
	name = "builtin pull",
	description = "Transfer a file or directory tree from the target host to the controller.",

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
			dir_mode = lib.schema.mode(),
			file_mode = lib.schema.mode(),
		},
	},

	validate = function(ctx, args)
		local src_error = lib.validate_absolute_path(ctx, args.src, "src")
		if src_error ~= nil then
			return src_error
		end
		local dest_error = validate_destination(ctx, args.dest)
		if dest_error ~= nil then
			return dest_error
		end
		local mode_error = validate_modes(args)
		if mode_error ~= nil then
			return mode_error
		end
		if args.recursive then
			local max_depth_error = lib.validate_max_depth(args.max_depth)
			if max_depth_error ~= nil then
				return max_depth_error
			end
		end
		return nil
	end,

	apply = function(ctx, args)
		local source = ctx.host.fs.lstat(args.src)
		if source == nil then
			error("pull source does not exist: " .. args.src)
		end

		if not args.recursive then
			if source.kind ~= "file" then
				error("pull source must be a regular file unless recursive=true: " .. args.src)
			end
			local exists = controller_exists(ctx, args.dest)
			if exists and not args.replace then
				return lib.skip(
					"destination already exists and replace is false: " .. resolved_controller_path(ctx, args.dest)
				)
			end
			return ctx.transfer.pull_file(args.src, args.dest, lib.pull_file_opts(args))
		end

		if source.kind ~= "dir" then
			error("pull recursive source must be a directory: " .. args.src)
		end
		if ctx.host.path.normalize(args.src) == "/" then
			error("refusing to use / as pull source")
		end

		local exists, root = controller_exists(ctx, args.dest)
		if exists and root.kind ~= "dir" then
			if not args.replace then
				return lib.skip(
					"destination already exists and replace is false: " .. resolved_controller_path(ctx, args.dest)
				)
			end
			error(
				"pull destination root must be a directory: "
					.. resolved_controller_path(ctx, args.dest)
					.. " is "
					.. root.kind
			)
		end

		local entries = ctx.host.fs.walk(args.src, {
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

		return ctx.transfer.pull_tree(args.src, args.dest, lib.pull_tree_opts(args))
	end,
}
