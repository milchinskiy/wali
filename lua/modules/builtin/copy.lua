local lib = require("wali.builtin.lib")

local function count_entry(counts, kind)
	if counts[kind] == nil then
		counts[kind] = 0
	end
	counts[kind] = counts[kind] + 1
end

local function preflight_file(ctx, args, path)
	local current = ctx.host.fs.lstat(path)
	if current == nil then
		return nil
	end
	if not args.replace then
		return "destination already exists and replace is false: " .. path
	end
	lib.assert_tree_destination(ctx, path, { expect = "file" })
	return nil
end

local function preflight_dir(ctx, args, path)
	local current = ctx.host.fs.lstat(path)
	if current == nil or current.kind == "dir" then
		return nil
	end
	if not args.replace then
		return "destination already exists and replace is false: " .. path
	end
	error("tree destination path must be a directory: " .. path .. " is " .. current.kind)
end

local function preflight_symlink(ctx, args, path, target)
	local current = ctx.host.fs.lstat(path)
	if current == nil then
		return nil
	end
	if not args.replace then
		return "destination already exists and replace is false: " .. path
	end
	if current.kind == "symlink" and ctx.host.fs.read_link(path) == target then
		return nil
	end
	lib.assert_tree_destination(ctx, path, { expect = "symlink", target = target, replace = true })
	return nil
end

local function preflight_entry(ctx, args, dest, entry)
	local path = lib.tree_destination(ctx, dest, entry.relative_path)
	if entry.kind == "dir" then
		return preflight_dir(ctx, args, path)
	elseif entry.kind == "file" then
		return preflight_file(ctx, args, path)
	elseif entry.kind == "symlink" then
		if args.symlinks == "preserve" then
			if entry.link_target == nil then
				error("source symlink has no target in walk output: " .. entry.path)
			end
			return preflight_symlink(ctx, args, path, entry.link_target)
		elseif args.symlinks == "skip" then
			return nil
		end
		error("refusing to copy source symlink: " .. entry.path)
	elseif args.skip_special then
		return nil
	end
	error("refusing to copy special filesystem entry without skip_special=true: " .. entry.path)
end

local function validate_metadata(args)
	local err = lib.validate_mode_owner(args, { mode = "mode", owner = "owner" })
	if err ~= nil then
		return err
	end
	if not args.recursive then
		return nil
	end
	for _, spec in ipairs({
		{ mode = "dir_mode", owner = "dir_owner" },
		{ mode = "file_mode", owner = "file_owner" },
	}) do
		err = lib.validate_mode_owner(args, spec)
		if err ~= nil then
			return err
		end
	end
	return nil
end

return {
	name = "builtin copy",
	description = "Copy a file or directory tree on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			recursive = { type = "boolean", default = false },
			preserve_mode = { type = "boolean", default = true },
			preserve_owner = { type = "boolean", default = false },
			symlinks = { type = "enum", values = { "preserve", "skip", "error" }, default = "preserve" },
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
		local path_error = lib.validate_absolute_paths(ctx, args, { "src", "dest" })
		if path_error ~= nil then
			return path_error
		end

		local metadata_error = validate_metadata(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		if args.recursive then
			local max_depth_error = lib.validate_max_depth(args.max_depth)
			if max_depth_error ~= nil then
				return max_depth_error
			end
			return lib.validate_tree_roots(ctx, args.src, args.dest)
		end

		return nil
	end,

	apply = function(ctx, args)
		local src = ctx.host.path.normalize(args.src)
		local dest = ctx.host.path.normalize(args.dest)
		local source = ctx.host.fs.lstat(src)
		if source == nil then
			error("copy source does not exist: " .. src)
		end

		if not args.recursive then
			if source.kind ~= "file" then
				error("copy source must be a regular file unless recursive=true: " .. src)
			end
			local skipped = lib.skip_if_replace_false_and_exists(ctx, dest, args.replace, "destination")
			if skipped ~= nil then
				return skipped
			end
			return ctx.host.fs.copy_file(src, dest, lib.copy_file_opts(args))
		end

		if source.kind ~= "dir" then
			error("copy recursive source must be a directory: " .. src)
		end

		local entries = ctx.host.fs.walk(src, {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		})
		local result = lib.result.apply()
		local counts = { dir = 0, file = 0, symlink = 0, other = 0, skipped = 0 }

		local root_reason = preflight_dir(ctx, args, dest)
		if root_reason ~= nil then
			return lib.skip(root_reason)
		end
		for _, entry in ipairs(entries) do
			local reason = preflight_entry(ctx, args, dest, entry)
			if reason ~= nil then
				return lib.skip(reason)
			end
		end

		lib.ensure_dir(ctx, result, dest, lib.tree_dir_opts(args, source))
		for _, entry in ipairs(entries) do
			local path = lib.tree_destination(ctx, dest, entry.relative_path)
			if entry.kind == "dir" then
				count_entry(counts, "dir")
				lib.ensure_dir(ctx, result, path, lib.tree_dir_opts(args, entry.metadata))
			elseif entry.kind == "file" then
				count_entry(counts, "file")
				result:merge(ctx.host.fs.copy_file(entry.path, path, lib.tree_copy_file_opts(args, entry.metadata)))
			elseif entry.kind == "symlink" then
				if args.symlinks == "preserve" then
					if entry.link_target == nil then
						error("source symlink has no target in walk output: " .. entry.path)
					end
					count_entry(counts, "symlink")
					local ok, reason = lib.ensure_symlink(ctx, result, path, entry.link_target, args.replace)
					if not ok then
						return lib.skip(reason)
					end
				elseif args.symlinks == "skip" then
					counts.skipped = counts.skipped + 1
					result:unchanged(path, "skipped source symlink")
				else
					error("refusing to copy source symlink: " .. entry.path)
				end
			else
				count_entry(counts, "other")
				if args.skip_special then
					counts.skipped = counts.skipped + 1
					result:unchanged(path, "skipped special source entry")
				else
					error("refusing to copy special filesystem entry without skip_special=true: " .. entry.path)
				end
			end
		end

		return result
			:message(
				string.format(
					"copied %s -> %s: %d dirs, %d files, %d symlinks",
					src,
					dest,
					counts.dir,
					counts.file,
					counts.symlink
				)
			)
			:data({
				src = src,
				dest = dest,
				recursive = true,
				replace = args.replace,
				preserve_mode = args.preserve_mode,
				preserve_owner = args.preserve_owner,
				symlinks = args.symlinks,
				skip_special = args.skip_special,
				max_depth = args.max_depth,
				counts = counts,
			})
			:build()
	end,
}
