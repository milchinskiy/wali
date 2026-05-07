local lib = require("wali.builtin.lib")

return {
	name = "test module",
	description = "deterministic example custom module",

	requires = {
		all = {
			{ path = "/tmp" },
			{ command = "sh" },
		},
	},

	schema = {
		type = "object",
		required = true,
		props = {
			source = { type = "string", default = "." },
			target = { type = "string", required = true },
		},
	},

	validate = function(ctx, args)
		return lib.validate_absolute_path(ctx, args.target, "target")
	end,

	apply = function(ctx, args)
		return ctx.host.fs.write(args.target, "source=" .. args.source .. "\n", {
			create_parents = true,
		})
	end,
}
