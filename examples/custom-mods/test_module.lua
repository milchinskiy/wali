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
			return false, "some error has occured during validation"
		end
		return true
	end,

	apply = function(ctx, args)
		ctx.sleep_ms(ctx.rand.irange(100, 2000))
		if ctx.rand.ratio(1, 20) then
			return false, "some error has occured during execution"
		end
		-- print("os", ctx.host.facts.os())
		return true
	end,
}
