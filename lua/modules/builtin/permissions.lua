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
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(_, args)
		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end
		if not lib.has_mode_owner(args) then
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
			error(
				"permissions target kind mismatch for "
					.. args.path
					.. ": expected "
					.. args.expect
					.. ", got "
					.. current.kind
			)
		end

		local result = lib.result.apply()
		lib.apply_mode_owner(ctx, result, args.path, args)
		return result:build()
	end,
}
