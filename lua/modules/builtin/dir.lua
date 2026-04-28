local lib = require("wali.builtin.lib")

return {
	name = "builtin directory",
	description = "Ensure a directory is present or absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			state = { type = "enum", values = { "present", "absent" }, default = "present" },
			parents = { type = "boolean", default = true },
			recursive = { type = "boolean", default = false },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(_, args)
		return lib.validate_mode_owner(args)
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_dir(args.path, { recursive = args.recursive })
		end

		return ctx.host.fs.create_dir(args.path, lib.create_dir_opts(args))
	end,
}
