local lib = require("wali.builtin.lib")

return {
	name = "builtin touch",
	description = "Ensure a regular file exists without replacing existing file content.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			create_parents = { type = "boolean", default = false },
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
			return ctx.host.fs.write(args.path, "", lib.write_file_opts(args))
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
