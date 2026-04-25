local res = require("wali.api").result.apply

return {
	name = "test module",
	description = "test module description",

	schema = {
		type = "object",
		required = true,
		props = {
			source = { type = "string", default = "." },
			target = { type = "string", required = true },
		},
	},

	validate = function(ctx, args)
		if ctx.rand.ratio(1, 20) then
			return { ok = false, message = "some error has occured during validation" }
		end
	end,

	apply = function(ctx, args)
		ctx.sleep_ms(ctx.rand.irange(100, 2000))
		if ctx.rand.ratio(1, 20) then
			error("some error has occured during execution")
		end
		-- print("os", ctx.host.facts.os())
		local result = res():with("some apply message")
		for _, v in ipairs({ args.source, args.target }) do
			result:created(v):updated(v):removed(v):unchanged(v)
		end

		return result:build()
		-- return { changes = result.changes, message = "some message" }
	end,
}
