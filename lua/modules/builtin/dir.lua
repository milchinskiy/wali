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
			mode = { type = "string" },
			owner = {
				type = "object",
				props = {
					user = { type = "any" },
					group = { type = "any" },
				},
			},
		},
	},

	validate = function(_, args)
		return lib.validate_mode(args.mode)
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_dir(args.path, { recursive = args.recursive })
		end

		local opts = lib.fs_opts(args)
		opts.recursive = args.parents
		return ctx.host.fs.create_dir(args.path, opts)
	end,
}
