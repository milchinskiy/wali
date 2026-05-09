local lib = require("wali.builtin.lib")

local function validate_metadata(args)
	for _, spec in ipairs({
		{ mode = "mode", owner = "owner" },
		{ mode = "dir_mode", owner = "dir_owner" },
		{ mode = "file_mode", owner = "file_owner" },
	}) do
		local err = lib.validate_mode_owner(args, spec)
		if err ~= nil then
			return err
		end
	end
	return nil
end

local function has_metadata(args)
	return lib.has_mode_owner(args)
		or lib.has_mode_owner(args, { mode = "dir_mode", owner = "dir_owner" })
		or lib.has_mode_owner(args, { mode = "file_mode", owner = "file_owner" })
end

local function metadata_args(args, kind)
	local out = {}
	if kind == "dir" then
		out.mode = args.dir_mode or args.mode
		out.owner = args.dir_owner or args.owner
	else
		out.mode = args.file_mode or args.mode
		out.owner = args.file_owner or args.owner
	end
	return out
end

local function apply_one(ctx, result, path, args, kind)
	local opts = metadata_args(args, kind)
	if opts.mode == nil and lib.owner(opts.owner) == nil then
		result:unchanged(path, "no metadata requested for " .. kind)
		return
	end
	lib.apply_mode_owner(ctx, result, path, opts)
end

return {
	name = "builtin permissions",
	description = "Reconcile mode and/or owner metadata on an existing path or tree.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			follow = { type = "boolean", default = true },
			expect = { type = "enum", values = { "any", "file", "dir" }, default = "any" },
			recursive = { type = "boolean", default = false },
			symlinks = { type = "enum", values = { "skip", "error" }, default = "skip" },
			skip_special = { type = "boolean", default = false },
			max_depth = { type = "integer" },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
			dir_mode = lib.schema.mode(),
			file_mode = lib.schema.mode(),
			dir_owner = lib.schema.owner(),
			file_owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local path_error = lib.validate_absolute_path(ctx, args.path, "path")
		if path_error ~= nil then
			return path_error
		end

		local metadata_error = validate_metadata(args)
		if metadata_error ~= nil then
			return metadata_error
		end
		if not has_metadata(args) then
			return lib.validation_error("mode or owner is required")
		end
		if args.recursive then
			return lib.validate_max_depth(args.max_depth)
		end
		return nil
	end,

	apply = function(ctx, args)
		local current = args.follow and ctx.host.fs.stat(args.path) or ctx.host.fs.lstat(args.path)
		if current == nil then
			error("permissions target does not exist: " .. args.path)
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
		if not args.recursive then
			if current.kind == "symlink" then
				error("refusing to manage symlink permissions with follow=false: " .. args.path)
			end
			if current.kind == "other" then
				error("refusing to manage special filesystem entry permissions: " .. args.path)
			end
			apply_one(ctx, result, args.path, args, current.kind)
			return result:build()
		end

		if current.kind ~= "dir" then
			error("permissions recursive target must be a directory: " .. args.path)
		end
		local entries = ctx.host.fs.walk(args.path, {
			include_root = true,
			order = "pre",
			max_depth = args.max_depth,
		})
		for _, entry in ipairs(entries) do
			if entry.kind == "dir" or entry.kind == "file" then
				apply_one(ctx, result, entry.path, args, entry.kind)
			elseif entry.kind == "symlink" then
				if args.symlinks == "skip" then
					result:unchanged(entry.path, "skipped symlink")
				else
					error("refusing to manage symlink permissions: " .. entry.path)
				end
			elseif args.skip_special then
				result:unchanged(entry.path, "skipped special filesystem entry")
			else
				error("refusing to manage special filesystem entry permissions: " .. entry.path)
			end
		end
		return result:build()
	end,
}
