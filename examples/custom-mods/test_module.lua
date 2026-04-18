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
        print("arch", ctx.host.facts.arch())
		return true
	end,

	apply = function(ctx, args)
        print("os", ctx.host.facts.os())
		return true
	end,
}
