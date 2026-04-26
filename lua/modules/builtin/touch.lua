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
