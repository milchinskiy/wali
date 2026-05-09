local lib = require("wali.builtin.lib")

local function resolved_controller_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function validate_source(ctx, src)
	if src == "" then
		return lib.validation_error("src must not be empty")
	end

	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, src, { follow = false })
	if not ok then
		return lib.validation_error(metadata_or_error)
	end
	local metadata = metadata_or_error
	if metadata == nil then
		return lib.validation_error("transfer source does not exist: " .. resolved_controller_path(ctx, src))
	end
	if metadata.kind ~= "dir" then
		return lib.validation_error("transfer source must be a directory: " .. resolved_controller_path(ctx, src))
	end

	local resolved = resolved_controller_path(ctx, src)
	local ok_parent, parent = pcall(ctx.controller.path.parent, resolved)
	if ok_parent and parent == nil then
		return lib.validation_error("refusing to use controller filesystem root as transfer source")
	end
	return nil
end

local function validate_destination(ctx, dest)
	local dest_error = lib.validate_absolute_path(ctx, dest, "dest")
	if dest_error ~= nil then
		return dest_error
	end
	if ctx.host.path.normalize(dest) == "/" then
		return lib.validation_error("refusing to use / as tree destination")
	end
	return nil
end

return {
	name = "builtin push tree",
	description = "Transfer a directory tree from the wali controller to the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			replace = { type = "boolean", default = true },
			preserve_mode = { type = "boolean", default = true },
			symlinks = { type = "enum", values = { "preserve", "skip", "error" }, default = "preserve" },
			skip_special = { type = "boolean", default = false },
			max_depth = { type = "integer" },
			dir_mode = lib.schema.mode(),
			file_mode = lib.schema.mode(),
			dir_owner = lib.schema.owner(),
			file_owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local max_depth_error = lib.validate_max_depth(args.max_depth)
		if max_depth_error ~= nil then
			return max_depth_error
		end

		local dest_error = validate_destination(ctx, args.dest)
		if dest_error ~= nil then
			return dest_error
		end

		local source_error = validate_source(ctx, args.src)
		if source_error ~= nil then
			return source_error
		end

		local dir_metadata_error = lib.validate_mode_owner(args, { mode = "dir_mode", owner = "dir_owner" })
		if dir_metadata_error ~= nil then
			return dir_metadata_error
		end

		return lib.validate_mode_owner(args, { mode = "file_mode", owner = "file_owner" })
	end,

	apply = function(ctx, args)
		return ctx.transfer.push_tree(args.src, args.dest, lib.push_tree_opts(args))
	end,
}
