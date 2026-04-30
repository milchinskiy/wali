local lib = require("wali.builtin.lib")

return {
	name = "builtin push file",
	description = "Transfer a regular file from the wali controller to the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		if args.src == "" then
			return lib.validation_error("src must not be empty")
		end
		if args.dest == "" then
			return lib.validation_error("dest must not be empty")
		end

		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		local source = ctx.transfer.check_push_file_source(args.src)
		if not source.ok then
			return lib.validation_error(source.message)
		end

		return nil
	end,

	apply = function(ctx, args)
		return ctx.transfer.push_file(args.src, args.dest, lib.write_file_opts(args))
	end,
}
