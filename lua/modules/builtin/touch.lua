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
		return lib.validate_mode(args.mode)
	end,

	apply = function(ctx, args)
		local current = ctx.host.fs.stat(args.path)
		if current == nil then
			local opts = lib.fs_opts(args)
			opts.create_parents = args.create_parents
			return ctx.host.fs.write(args.path, "", opts)
		end

		if current.kind ~= "file" then
			error("touch target already exists and is not a regular file: " .. args.path)
		end

		local result = lib.result.apply()
		if args.mode ~= nil then
			result:merge(ctx.host.fs.chmod(args.path, lib.mode_bits(args.mode)))
		end
		local owner = lib.owner(args.owner)
		if owner ~= nil then
			result:merge(ctx.host.fs.chown(args.path, owner))
		end
		if result:empty() then
			result:unchanged(args.path, "file already exists")
		end
		return result:build()
	end,
}
