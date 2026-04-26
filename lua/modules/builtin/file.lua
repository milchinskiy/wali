local lib = require("wali.builtin.lib")

return {
	name = "builtin file",
	description = "Ensure a regular file with literal content is present or absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			state = { type = "enum", values = { "present", "absent" }, default = "present" },
			content = { type = "string" },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
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
		local mode_error = lib.validate_mode(args.mode)
		if mode_error ~= nil then
			return mode_error
		end
		if args.state == "present" and args.content == nil then
			return lib.validation_error("content is required when state is present")
		end
		return nil
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_file(args.path)
		end

		local opts = lib.fs_opts(args)
		opts.create_parents = args.create_parents
		opts.replace = args.replace
		return ctx.host.fs.write(args.path, args.content, opts)
	end,
}
