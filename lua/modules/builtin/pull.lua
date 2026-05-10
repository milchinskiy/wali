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

local function controller_file_content_matches(ctx, path, expected)
	local ok, actual = pcall(ctx.controller.fs.read, path)
	return ok and actual == expected
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
		local max_depth_error = lib.validate_max_depth(args.max_depth)
		if max_depth_error ~= nil then
			return max_depth_error
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
			local exists, dest_metadata = controller_exists(ctx, args.dest)
			if exists and not args.replace then
				if dest_metadata.kind == "file" then
					local source_bytes = ctx.host.fs.read(args.src)
					if not controller_file_content_matches(ctx, args.dest, source_bytes) then
						return lib.skip(
							"destination already exists and replace is false: "
								.. resolved_controller_path(ctx, args.dest)
						)
					end
				elseif dest_metadata.kind == "symlink" then
					return lib.skip(
						"destination already exists and replace is false: " .. resolved_controller_path(ctx, args.dest)
					)
				elseif dest_metadata.kind == "dir" then
					error("pull destination is a directory: " .. resolved_controller_path(ctx, args.dest))
				else
					error(
						"pull destination is a special filesystem entry: " .. resolved_controller_path(ctx, args.dest)
					)
				end
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

		return ctx.transfer.pull_tree(args.src, args.dest, lib.pull_tree_opts(args))
	end,
}
