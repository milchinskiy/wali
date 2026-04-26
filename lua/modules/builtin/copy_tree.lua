local lib = require("wali.builtin.lib")

local function count_entry(counts, kind)
	if counts[kind] == nil then
		counts[kind] = 0
	end
	counts[kind] = counts[kind] + 1
end

local function preflight_entry(ctx, args, dest, entry)
	local path = lib.tree_destination(ctx, dest, entry.relative_path)
	if entry.kind == "dir" then
		lib.assert_tree_destination(ctx, path, { expect = "dir" })
	elseif entry.kind == "file" then
		lib.assert_tree_destination(ctx, path, { expect = "file" })
	elseif entry.kind == "symlink" then
		if args.symlinks == "preserve" then
			if entry.link_target == nil then
				error("source symlink has no target in walk output: " .. entry.path)
			end
			lib.assert_tree_destination(ctx, path, {
				expect = "symlink",
				target = entry.link_target,
				replace = args.replace,
			})
		elseif args.symlinks ~= "skip" then
			error("refusing to copy source symlink: " .. entry.path)
		end
	elseif not args.skip_special then
		error("refusing to copy special filesystem entry without skip_special=true: " .. entry.path)
	end
end

return {
	name = "builtin copy tree",
	description = "Copy a source directory tree into a destination directory on the same target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			replace = { type = "boolean", default = true },
			preserve_mode = { type = "boolean", default = true },
			preserve_owner = { type = "boolean", default = false },
			symlinks = { type = "enum", values = { "preserve", "skip", "error" }, default = "preserve" },
			skip_special = { type = "boolean", default = false },
			max_depth = { type = "number" },
			dir_mode = { type = "string" },
			file_mode = { type = "string" },
			dir_owner = {
				type = "object",
				props = {
					user = { type = "any" },
					group = { type = "any" },
				},
			},
			file_owner = {
				type = "object",
				props = {
					user = { type = "any" },
					group = { type = "any" },
				},
			},
		},
	},

	validate = function(ctx, args)
		local root_error = lib.validate_tree_roots(ctx, args.src, args.dest)
		if root_error ~= nil then
			return root_error
		end

		local dir_metadata_error = lib.validate_mode_owner(args, { mode = "dir_mode", owner = "dir_owner" })
		if dir_metadata_error ~= nil then
			return dir_metadata_error
		end

		return lib.validate_mode_owner(args, { mode = "file_mode", owner = "file_owner" })
	end,

	apply = function(ctx, args)
		local src = ctx.host.path.normalize(args.src)
		local dest = ctx.host.path.normalize(args.dest)
		local source_root = ctx.host.fs.lstat(src)
		if source_root == nil then
			error("copy_tree source does not exist: " .. src)
		end
		if source_root.kind ~= "dir" then
			error("copy_tree source must be a directory: " .. src)
		end

		local entries = ctx.host.fs.walk(src, {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		})
		local result = lib.result.apply()
		local counts = {
			dir = 0,
			file = 0,
			symlink = 0,
			other = 0,
			skipped = 0,
		}

		lib.assert_tree_destination(ctx, dest, { expect = "dir", label = "tree destination root" })
		for _, entry in ipairs(entries) do
			preflight_entry(ctx, args, dest, entry)
		end

		lib.ensure_dir(ctx, result, dest, lib.tree_dir_opts(args, source_root))

		for _, entry in ipairs(entries) do
			local path = lib.tree_destination(ctx, dest, entry.relative_path)
			if entry.kind == "dir" then
				count_entry(counts, "dir")
				lib.ensure_dir(ctx, result, path, lib.tree_dir_opts(args, entry.metadata))
			elseif entry.kind == "file" then
				count_entry(counts, "file")
				result:merge(ctx.host.fs.copy_file(entry.path, path, lib.tree_copy_file_opts(args, entry.metadata)))
			elseif entry.kind == "symlink" then
				count_entry(counts, "symlink")
				if args.symlinks == "preserve" then
					if entry.link_target == nil then
						error("source symlink has no target in walk output: " .. entry.path)
					end
					lib.ensure_symlink(ctx, result, path, entry.link_target, args.replace)
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

		local message = string.format(
			"copied tree %s -> %s: %d dirs, %d files, %d symlinks",
			src,
			dest,
			counts.dir,
			counts.file,
			counts.symlink
		)
		return result:message(message):data({
			src = src,
			dest = dest,
			replace = args.replace,
			preserve_mode = args.preserve_mode,
			preserve_owner = args.preserve_owner,
			symlinks = args.symlinks,
			skip_special = args.skip_special,
			max_depth = args.max_depth,
			counts = counts,
		}):build()
	end,
}
