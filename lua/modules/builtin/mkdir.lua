local lib = require("wali.builtin.lib")

return {
	name = "builtin mkdir",
	description = "Create a directory on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			parents = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_path(ctx, args.path, "path")
		if path_error ~= nil then
			return path_error
		end

		return lib.validate_mode_owner(args)
	end,

	apply = function(ctx, args)
		return ctx.host.fs.create_dir(args.path, lib.create_dir_opts(args))
	end,
}
