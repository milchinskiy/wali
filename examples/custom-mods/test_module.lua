local apply_result = require("wali.api").result.apply
local validation_result = require("wali.api").result.validation

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
		return validation_result():ok():build()
	end,

	apply = function(ctx, args)
		local result = apply_result()
		result:created(args.target)
		result:unchanged(args.source)
		return result:build()
	end,
}
