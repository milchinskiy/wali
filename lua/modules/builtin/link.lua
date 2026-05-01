local lib = require("wali.builtin.lib")

return {
	name = "builtin symlink",
	description = "Ensure a symbolic link path is present or absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			target = { type = "string" },
			state = { type = "enum", values = { "present", "absent" }, default = "present" },
			replace = { type = "boolean", default = false },
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_path(ctx, args.path, "path")
		if path_error ~= nil then
			return path_error
		end
		if args.state == "present" and args.target == nil then
			return lib.validation_error("target is required when state is present")
		end
		if args.target == "" then
			return lib.validation_error("target must not be empty")
		end
		return nil
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_file(args.path)
		end

		local result = lib.result.apply()
		lib.ensure_symlink(ctx, result, args.path, args.target, args.replace)
		return result:build()
	end,
}
