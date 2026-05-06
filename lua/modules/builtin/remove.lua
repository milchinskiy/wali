local lib = require("wali.builtin.lib")

return {
	name = "builtin remove",
	description = "Ensure a filesystem path is absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			recursive = { type = "boolean", default = false },
		},
	},

	validate = function(ctx, args)
		return lib.validate_safe_remove_path(ctx, args.path)
	end,

	apply = function(ctx, args)
		local current = ctx.host.fs.lstat(args.path)
		if current == nil then
			return lib.result.apply():unchanged(args.path, "path is already absent"):build()
		end

		if current.kind == "dir" then
			return ctx.host.fs.remove_dir(args.path, { recursive = args.recursive })
		end

		if current.kind == "file" or current.kind == "symlink" then
			return ctx.host.fs.remove_file(args.path)
		end

		error("refusing to remove special filesystem entry: " .. args.path)
	end,
}
