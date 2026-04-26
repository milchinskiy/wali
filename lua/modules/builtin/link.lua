local lib = require("wali.builtin.lib")

return {
	name = "builtin symlink",
	description = "Ensure a symbolic link is present or absent on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			target = { type = "string" },
			state = { type = "enum", values = { "present", "absent" }, default = "present" },
			replace = { type = "boolean", default = false },
		},
	},

	validate = function(_, args)
		if args.state == "present" and args.target == nil then
			return lib.validation_error("target is required when state is present")
		end
		return nil
	end,

	apply = function(ctx, args)
		if args.state == "absent" then
			return ctx.host.fs.remove_file(args.path)
		end

		local current = ctx.host.fs.stat(args.path)
		if current == nil then
			return ctx.host.fs.symlink(args.target, args.path)
		end

		if current.kind == "symlink" then
			local current_target = ctx.host.fs.read_link(args.path)
			if current_target == args.target then
				return lib.result.apply():unchanged(args.path, "symlink already points to target"):build()
			end
		end

		if not args.replace then
			error("path already exists and replace is false: " .. args.path)
		end
		if current.kind == "dir" then
			error("refusing to replace directory with symlink: " .. args.path)
		end

		local result = lib.result.apply()
		result:merge(ctx.host.fs.remove_file(args.path))
		result:merge(ctx.host.fs.symlink(args.target, args.path))
		return result:message("replaced existing path with symlink"):build()
	end,
}
