local lib = require("wali.builtin.lib")

local function resolved_controller_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function validate_source(ctx, src)
	local src_error = lib.validate_absolute_path(ctx, src, "src")
	if src_error ~= nil then
		return src_error
	end
	if ctx.host.path.normalize(src) == "/" then
		return lib.validation_error("refusing to use / as tree source")
	end
	return nil
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

	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, dest, { follow = false })
	if not ok then
		return lib.validation_error(metadata_or_error)
	end
	local metadata = metadata_or_error
	if metadata ~= nil and metadata.kind ~= "dir" then
		return lib.validation_error("transfer destination must be a directory: " .. resolved)
	end
	return nil
end

return {
	name = "builtin pull tree",
	description = "Transfer a directory tree from the target host to the wali controller.",

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
		},
	},

	validate = function(ctx, args)
		local max_depth_error = lib.validate_max_depth(args.max_depth)
		if max_depth_error ~= nil then
			return max_depth_error
		end

		local source_error = validate_source(ctx, args.src)
		if source_error ~= nil then
			return source_error
		end

		local dest_error = validate_destination(ctx, args.dest)
		if dest_error ~= nil then
			return dest_error
		end

		local dir_mode_error = lib.validate_mode(args.dir_mode)
		if dir_mode_error ~= nil then
			return dir_mode_error
		end

		return lib.validate_mode(args.file_mode)
	end,

	apply = function(ctx, args)
		return ctx.transfer.pull_tree(args.src, args.dest, lib.pull_tree_opts(args))
	end,
}
