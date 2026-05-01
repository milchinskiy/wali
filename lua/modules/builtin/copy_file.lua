local lib = require("wali.builtin.lib")

return {
	name = "builtin copy file",
	description = "Copy a regular file on the target host without routing file bytes through Lua.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			preserve_mode = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_paths(ctx, args, { "src", "dest" })
		if path_error ~= nil then
			return path_error
		end

		return lib.validate_mode_owner(args)
	end,

	apply = function(ctx, args)
		return ctx.host.fs.copy_file(args.src, args.dest, lib.copy_file_opts(args))
	end,
}
