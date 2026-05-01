local lib = require("wali.builtin.lib")

return {
	name = "builtin pull file",
	description = "Transfer a regular file from the target host to the wali controller.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			mode = lib.schema.mode(),
		},
	},

	validate = function(ctx, args)
		local src_error = lib.validate_absolute_path(ctx, args.src, "src")
		if src_error ~= nil then
			return src_error
		end
		if args.dest == "" then
			return lib.validation_error("dest must not be empty")
		end
		return lib.validate_mode(args.mode)
	end,

	apply = function(ctx, args)
		return ctx.transfer.pull_file(args.src, args.dest, lib.pull_file_opts(args))
	end,
}
