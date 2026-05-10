local lib = require("wali.builtin.lib")

local function ensure_parent(ctx, result, path)
	local parent = ctx.host.path.parent(path)
	if parent ~= nil then
		lib.ensure_dir(ctx, result, parent, { recursive = true })
	end
end

local function validate_single(args)
	if args.src == nil or args.src == "" then
		return lib.validation_error("src must not be empty")
	end
	return nil
end

local function preflight_link_path(ctx, path, target, replace)
	local current = ctx.host.fs.lstat(path)
	if current == nil then
		return nil
	end
	if current.kind == "symlink" and ctx.host.fs.read_link(path) == target then
		return nil
	end
	if not replace then
		return "destination already exists and replace is false: " .. path
	end
	if current.kind == "dir" then
		error("refusing to replace directory with symlink: " .. path)
	end
	if current.kind ~= "file" and current.kind ~= "symlink" then
		error("refusing to replace special filesystem entry with symlink: " .. path)
	end
	return nil
end

local function should_skip_relative(relative_path, skipped_dirs)
	for _, prefix in ipairs(skipped_dirs) do
		if relative_path == prefix or relative_path:sub(1, #prefix + 1) == prefix .. "/" then
			return true
		end
	end
	return false
end

local function add_skipped(counts, result, path, reason)
	counts.skipped = counts.skipped + 1
	result:unchanged(path, reason)
end

local function preflight_replace_true_destinations(ctx, args, dest, entries)
	if not args.replace then
		return
	end

	for _, entry in ipairs(entries) do
		local path = lib.tree_destination(ctx, dest, entry.relative_path)
		if entry.kind == "dir" then
			lib.assert_tree_destination(ctx, path, { expect = "dir" })
		elseif entry.kind == "file" or entry.kind == "symlink" then
			lib.assert_tree_destination(ctx, path, {
				expect = "symlink",
				target = entry.path,
				replace = true,
			})
		end
	end
end

local function handle_dir(ctx, args, result, counts, skipped_dirs, path, entry)
	local current = ctx.host.fs.lstat(path)
	if current ~= nil and current.kind ~= "dir" then
		if not args.replace then
			counts.dir = counts.dir + 1
			add_skipped(counts, result, path, "destination already exists and replace is false: " .. path)
			table.insert(skipped_dirs, entry.relative_path)
			return
		end
		error("tree destination path must be a directory: " .. path .. " is " .. current.kind)
	end
	counts.dir = counts.dir + 1
	lib.ensure_dir(ctx, result, path, lib.link_tree_dir_opts({ dir_mode = args.dir_mode, dir_owner = args.dir_owner }))
end

return {
	name = "builtin link",
	description = "Create one symlink or recursively mirror a directory as symlinks.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			recursive = { type = "boolean", default = false },
			skip_special = { type = "boolean", default = false },
			max_depth = { type = "integer" },
			dir_mode = lib.schema.mode(),
			dir_owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local dest_error = lib.validate_absolute_path(ctx, args.dest, "dest")
		if dest_error ~= nil then
			return dest_error
		end

		local max_depth_error = lib.validate_max_depth(args.max_depth)
		if max_depth_error ~= nil then
			return max_depth_error
		end

		if not args.recursive then
			return validate_single(args)
		end

		local metadata_error = lib.validate_mode_owner(args, { mode = "dir_mode", owner = "dir_owner" })
		if metadata_error ~= nil then
			return metadata_error
		end
		local root_error = lib.validate_tree_roots(ctx, args.src, args.dest)
		if root_error ~= nil then
			return root_error
		end
		return nil
	end,

	apply = function(ctx, args)
		local result = lib.result.apply()

		if not args.recursive then
			local skip_reason = preflight_link_path(ctx, args.dest, args.src, args.replace)
			if skip_reason ~= nil then
				return lib.skip(skip_reason)
			end
			if args.parents then
				ensure_parent(ctx, result, args.dest)
			end
			local ok, reason = lib.ensure_symlink(ctx, result, args.dest, args.src, args.replace)
			if not ok then
				return lib.skip(reason)
			end
			return result:build()
		end

		local src = ctx.host.path.normalize(args.src)
		local dest = ctx.host.path.normalize(args.dest)
		local source_root = ctx.host.fs.lstat(src)
		if source_root == nil then
			error("link source does not exist: " .. src)
		end
		if source_root.kind ~= "dir" then
			error("link recursive source must be a directory: " .. src)
		end

		local root_current = ctx.host.fs.lstat(dest)
		if root_current ~= nil and root_current.kind ~= "dir" then
			if not args.replace then
				return lib.skip("destination already exists and replace is false: " .. dest)
			end
			error("tree destination root must be a directory: " .. dest .. " is " .. root_current.kind)
		end

		local entries = ctx.host.fs.walk(src, {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		})
		for _, entry in ipairs(entries) do
			if entry.kind ~= "dir" and entry.kind ~= "file" and entry.kind ~= "symlink" and not args.skip_special then
				error("refusing to link special filesystem entry without skip_special=true: " .. entry.path)
			end
		end
		preflight_replace_true_destinations(ctx, args, dest, entries)

		local counts = { dir = 0, link = 0, skipped = 0, other = 0 }
		local skipped_dirs = {}

		lib.ensure_dir(
			ctx,
			result,
			dest,
			lib.link_tree_dir_opts({ dir_mode = args.dir_mode, dir_owner = args.dir_owner })
		)
		for _, entry in ipairs(entries) do
			local path = lib.tree_destination(ctx, dest, entry.relative_path)
			if should_skip_relative(entry.relative_path, skipped_dirs) then
				add_skipped(counts, result, path, "skipped because parent destination is blocked")
			elseif entry.kind == "dir" then
				handle_dir(ctx, args, result, counts, skipped_dirs, path, entry)
			elseif entry.kind == "file" or entry.kind == "symlink" then
				local ok, reason = lib.ensure_symlink(ctx, result, path, entry.path, args.replace)
				if ok then
					counts.link = counts.link + 1
				else
					add_skipped(counts, result, path, reason)
				end
			elseif args.skip_special then
				counts.other = counts.other + 1
				add_skipped(counts, result, path, "skipped special source entry")
			else
				error("refusing to link special filesystem entry without skip_special=true: " .. entry.path)
			end
		end

		return result
			:message(string.format("linked %s -> %s: %d dirs, %d links", src, dest, counts.dir, counts.link))
			:data({
				src = src,
				dest = dest,
				recursive = true,
				replace = args.replace,
				max_depth = args.max_depth,
				counts = counts,
			})
			:build()
	end,
}
