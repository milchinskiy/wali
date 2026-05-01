local lib = require("wali.builtin.lib")

return {
	name = "builtin file",
	description = "Ensure a regular file with literal content is present or absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			state = { type = "enum", values = { "present", "absent" }, default = "present" },
			content = { type = "string" },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_path(ctx, args.path, "path")
		if path_error ~= nil then
			return path_error
		end

		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end
		if args.state == "present" and args.content == nil then
			return lib.validation_error("content is required when state is present")
		end
		return nil
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_file(args.path)
		end

		return ctx.host.fs.write(args.path, args.content, lib.write_file_opts(args))
	end,
}
