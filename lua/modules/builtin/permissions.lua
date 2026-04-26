local lib = require("wali.builtin.lib")

return {
	name = "builtin permissions",
	description = "Ensure mode and/or owner metadata on an existing filesystem entry.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			follow = { type = "boolean", default = true },
			expect = { type = "enum", values = { "any", "file", "dir" }, default = "any" },
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
		if args.mode == nil and lib.owner(args.owner) == nil then
			return lib.validation_error("mode or owner is required")
		end
		return nil
	end,

	apply = function(ctx, args)
		local current
		if args.follow then
			current = ctx.host.fs.stat(args.path)
		else
			current = ctx.host.fs.lstat(args.path)
		end

		if current == nil then
			error("permissions target does not exist: " .. args.path)
		end
		if current.kind == "symlink" then
			error("refusing to manage symlink permissions with follow=false: " .. args.path)
		end
		if current.kind == "other" then
			error("refusing to manage special filesystem entry permissions: " .. args.path)
		end
		if args.expect ~= "any" and current.kind ~= args.expect then
			error("permissions target kind mismatch for " .. args.path .. ": expected " .. args.expect .. ", got " .. current.kind)
		end

		local result = lib.result.apply()
		if args.mode ~= nil then
			result:merge(ctx.host.fs.chmod(args.path, lib.mode_bits(args.mode)))
		end
		local owner = lib.owner(args.owner)
		if owner ~= nil then
			result:merge(ctx.host.fs.chown(args.path, owner))
		end
		return result:build()
	end,
}
