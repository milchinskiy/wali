local lib = require("wali.builtin.lib")

local function count_plan(entries)
	local counts = {
		dir = 0,
		link = 0,
		other = 0,
	}

	for _, entry in ipairs(entries) do
		if entry.kind == "dir" then
			counts.dir = counts.dir + 1
		elseif entry.kind == "file" or entry.kind == "symlink" then
			counts.link = counts.link + 1
		else
			counts.other = counts.other + 1
		end
	end

	return counts
end

local function dir_opts(args)
	local opts = lib.dir_opts(args)
	opts.recursive = true
	return opts
end

return {
	name = "builtin link tree",
	description = "Mirror a source tree as directories and symlinks at a destination path.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string", required = true },
			dest = { type = "string", required = true },
			replace = { type = "boolean", default = false },
			allow_special = { type = "boolean", default = false },
			max_depth = { type = "number" },
			dir_mode = { type = "string" },
			dir_owner = {
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
		local mode_error = lib.validate_mode(args.dir_mode)
		if mode_error ~= nil then
			return mode_error
		end
		return lib.validate_owner(args.dir_owner, "dir_owner")
	end,

	apply = function(ctx, args)
		local src = ctx.host.path.normalize(args.src)
		local dest = ctx.host.path.normalize(args.dest)
		local source_root = ctx.host.fs.lstat(src)
		if source_root == nil then
			error("link_tree source does not exist: " .. src)
		end
		if source_root.kind ~= "dir" then
			error("link_tree source must be a directory: " .. src)
		end

		local walk_opts = {
			include_root = false,
			order = "pre",
			max_depth = args.max_depth,
		}
		local entries = ctx.host.fs.walk(src, walk_opts)
		local result = lib.result.apply()
		local options = dir_opts(args)

		lib.ensure_dir(ctx, result, dest, options)

		for _, entry in ipairs(entries) do
			local path = lib.tree_destination(ctx, dest, entry.relative_path)
			if entry.kind == "dir" then
				lib.ensure_dir(ctx, result, path, options)
			elseif entry.kind == "file" or entry.kind == "symlink" then
				lib.ensure_symlink(ctx, result, path, entry.path, args.replace)
			elseif args.allow_special then
				lib.ensure_symlink(ctx, result, path, entry.path, args.replace)
			else
				error("refusing to link special filesystem entry without allow_special=true: " .. entry.path)
			end
		end

		local counts = count_plan(entries)
		local message = string.format(
			"linked tree %s -> %s: %d dirs, %d links",
			src,
			dest,
			counts.dir,
			counts.link
		)
		return result:message(message):data({
			src = src,
			dest = dest,
			replace = args.replace,
			allow_special = args.allow_special,
			max_depth = args.max_depth,
			counts = counts,
		}):build()
	end,
}
