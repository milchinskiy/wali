local lib = require("wali.builtin.lib")

local function resolved_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function validate_source(ctx, src)
	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, src)
	if not ok then
		return lib.validation_error(metadata_or_error)
	end
	local metadata = metadata_or_error
	if metadata == nil then
		return lib.validation_error("transfer source does not exist: " .. resolved_path(ctx, src))
	end
	if metadata.kind ~= "file" then
		return lib.validation_error("transfer source must be a regular file: " .. resolved_path(ctx, src))
	end
	return nil
end

return {
	name = "builtin push file",
	description = "Transfer a regular file from the wali controller to the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		if args.src == "" then
			return lib.validation_error("src must not be empty")
		end
		local dest_error = lib.validate_absolute_path(ctx, args.dest, "dest")
		if dest_error ~= nil then
			return dest_error
		end

		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		return validate_source(ctx, args.src)
	end,

	apply = function(ctx, args)
		return ctx.transfer.push_file(args.src, args.dest, lib.write_file_opts(args))
	end,
}
