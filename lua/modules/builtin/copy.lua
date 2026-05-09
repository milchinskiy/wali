local lib = require("wali.builtin.lib")

local function count_entry(counts, kind)
	if counts[kind] == nil then
		counts[kind] = 0
	end
	counts[kind] = counts[kind] + 1
end

local function add_skipped(counts, result, path, reason)
	counts.skipped = counts.skipped + 1
	result:unchanged(path, reason)
end

local function should_skip_relative(relative_path, skipped_dirs)
	for _, prefix in ipairs(skipped_dirs) do
		if relative_path == prefix or relative_path:sub(1, #prefix + 1) == prefix .. "/" then
			return true
		end
	end
	return false
end

local function source_and_dest_match(ctx, src, dest)
	local ok_source, source = pcall(ctx.host.fs.read, src)
	if not ok_source then
		error(source)
	end
	return lib.host_file_content_matches(ctx, dest, source)
end

local function replace_false_file_skip(ctx, src, path, skip_structural_conflicts)
	local current = ctx.host.fs.lstat(path)
	if current == nil then
		return nil
	end
	if current.kind == "file" then
		if source_and_dest_match(ctx, src, path) then
			return nil
		end
		return "destination already exists and replace is false: " .. path
	end
	if current.kind == "symlink" then
		return "destination already exists and replace is false: " .. path
	end
	if current.kind == "dir" then
		if skip_structural_conflicts then
			return "destination is a directory where a file is expected and replace is false: " .. path
		end
		error("copy destination is a directory: " .. path)
	end
	if skip_structural_conflicts then
		return "destination is a special filesystem entry where a file is expected and replace is false: " .. path
	end
	error("copy destination is a special filesystem entry: " .. path)
end

local function handle_dir(ctx, args, result, counts, skipped_dirs, path, entry)
	local current = ctx.host.fs.lstat(path)
	if current ~= nil and current.kind ~= "dir" then
		if not args.replace then
			count_entry(counts, "dir")
			add_skipped(counts, result, path, "destination already exists and replace is false: " .. path)
			table.insert(skipped_dirs, entry.relative_path)
			return
		end
		error("tree destination path must be a directory: " .. path .. " is " .. current.kind)
	end
	count_entry(counts, "dir")
	lib.ensure_dir(ctx, result, path, lib.tree_dir_opts(args, entry.metadata))
end

local function handle_file(ctx, args, result, counts, path, entry)
	if not args.replace then
		local reason = replace_false_file_skip(ctx, entry.path, path, true)
		if reason ~= nil then
			add_skipped(counts, result, path, reason)
			return
		end
	end
	count_entry(counts, "file")
	result:merge(ctx.host.fs.copy_file(entry.path, path, lib.tree_copy_file_opts(args, entry.metadata)))
end

local function handle_symlink(ctx, args, result, counts, path, entry)
	if args.symlinks == "preserve" then
		if entry.link_target == nil then
			error("source symlink has no target in walk output: " .. entry.path)
		end
		local ok, reason = lib.ensure_symlink(ctx, result, path, entry.link_target, args.replace)
		if ok then
			count_entry(counts, "symlink")
		else
			add_skipped(counts, result, path, reason)
		end
	elseif args.symlinks == "skip" then
		add_skipped(counts, result, path, "skipped source symlink")
	else
		error("refusing to copy source symlink: " .. entry.path)
	end
end

local function preflight_replace_true_destinations(ctx, args, dest, entries)
	if not args.replace then
		return
	end

	for _, entry in ipairs(entries) do
		local path = lib.tree_destination(ctx, dest, entry.relative_path)
		if entry.kind == "dir" then
			lib.assert_tree_destination(ctx, path, { expect = "dir" })
		elseif entry.kind == "file" then
			lib.assert_tree_destination(ctx, path, { expect = "file" })
		elseif entry.kind == "symlink" and args.symlinks == "preserve" then
			lib.assert_tree_destination(ctx, path, {
				expect = "symlink",
				target = entry.link_target,
				replace = true,
			})
		end
	end
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
			if not args.replace then
				local skip_reason = replace_false_file_skip(ctx, src, dest, false)
				if skip_reason ~= nil then
					return lib.skip(skip_reason)
				end
			end
			return ctx.host.fs.copy_file(src, dest, lib.copy_file_opts(args))
		end

		if source.kind ~= "dir" then
			error("copy recursive source must be a directory: " .. src)
		end

		local root_current = ctx.host.fs.lstat(dest)
		if root_current ~= nil and root_current.kind ~= "dir" then
			if not args.replace then
				return lib.skip("destination already exists and replace is false: " .. dest)
			end
			error("copy destination root must be a directory: " .. dest .. " is " .. root_current.kind)
		end

		local entries = ctx.host.fs.walk(src, {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		})
		for _, entry in ipairs(entries) do
			if entry.kind == "symlink" and args.symlinks == "error" then
				error("refusing to copy source symlink: " .. entry.path)
			end
			if entry.kind ~= "dir" and entry.kind ~= "file" and entry.kind ~= "symlink" and not args.skip_special then
				error("refusing to copy special filesystem entry without skip_special=true: " .. entry.path)
			end
		end
		preflight_replace_true_destinations(ctx, args, dest, entries)

		local result = lib.result.apply()
		local counts = { dir = 0, file = 0, symlink = 0, other = 0, skipped = 0 }
		local skipped_dirs = {}

		lib.ensure_dir(ctx, result, dest, lib.tree_dir_opts(args, source))
		for _, entry in ipairs(entries) do
			local path = lib.tree_destination(ctx, dest, entry.relative_path)
			if should_skip_relative(entry.relative_path, skipped_dirs) then
				add_skipped(counts, result, path, "skipped because parent destination is blocked")
			elseif entry.kind == "dir" then
				handle_dir(ctx, args, result, counts, skipped_dirs, path, entry)
			elseif entry.kind == "file" then
				handle_file(ctx, args, result, counts, path, entry)
			elseif entry.kind == "symlink" then
				handle_symlink(ctx, args, result, counts, path, entry)
			else
				count_entry(counts, "other")
				if args.skip_special then
					add_skipped(counts, result, path, "skipped special source entry")
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
