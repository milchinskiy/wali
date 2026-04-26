local apply_result = require("wali.api").result.apply
local validation_result = require("wali.api").result.validation

return {
	name = "test module",
	description = "test module description",

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
		ctx.sleep_ms(ctx.rand.irange(100, 1000))
		if ctx.rand.ratio(1, 20) then
			error("some error has occured during apply")
		end
		-- print("os", ctx.host.facts.os())
		for _, v in ipairs({ args.source, args.target }) do
			result:created(v):updated(v):removed(v):unchanged(v)
		end

		return result:build()
		-- return { changes = result.changes, message = "some message" }
	end,
}
