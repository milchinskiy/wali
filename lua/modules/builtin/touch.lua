local lib = require("wali.builtin.lib")

return {
	name = "builtin touch",
	description = "Create a regular file if absent without replacing existing content.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			parents = { type = "boolean", default = false },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_path(ctx, args.path, "path")
		if path_error ~= nil then
			return path_error
		end

		return lib.validate_mode_owner(args)
	end,

	apply = function(ctx, args)
		local current = ctx.host.fs.lstat(args.path)
		if current == nil then
			local opts =
				lib.write_file_opts({ parents = args.parents, replace = false, mode = args.mode, owner = args.owner })
			return ctx.host.fs.write(args.path, "", opts)
		end

		if current.kind ~= "file" then
			error("touch target already exists and is not a regular file: " .. args.path)
		end

		local result = lib.result.apply()
		lib.apply_mode_owner(ctx, result, args.path, args)
		if result:empty() then
			result:unchanged(args.path, "file already exists")
		end
		return result:build()
	end,
}
