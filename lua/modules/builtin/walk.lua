local lib = require("wali.builtin.lib")

local function count_entries(entries)
	local counts = {
		file = 0,
		dir = 0,
		symlink = 0,
		other = 0,
	}

	for _, entry in ipairs(entries) do
		counts[entry.kind] = (counts[entry.kind] or 0) + 1
	end

	return counts
end

local function summary(path, entries, counts)
	return string.format(
		"walked %s: %d entries, %d dir(s), %d file(s), %d symlink(s), %d other",
		path,
		#entries,
		counts.dir or 0,
		counts.file or 0,
		counts.symlink or 0,
		counts.other or 0
	)
end

return {
	name = "builtin walk",
	description = "Inspect a filesystem tree and return deterministic walk output.",

	schema = {
		type = "object",
		required = true,
		props = {
			path = { type = "string", required = true },
			include_root = { type = "boolean", default = false },
			max_depth = { type = "number" },
			order = { type = "enum", values = { "pre", "post", "native" }, default = "pre" },
		},
	},

	apply = function(ctx, args)
		local opts = {
			include_root = args.include_root,
			max_depth = args.max_depth,
			order = args.order,
		}
		local entries = ctx.host.fs.walk(args.path, opts)
		local counts = count_entries(entries)
		local message = summary(args.path, entries, counts)

		return lib.result
			.apply()
			:unchanged(args.path, "tree inspected")
			:message(message)
			:data({
				root = args.path,
				include_root = args.include_root,
				max_depth = args.max_depth,
				order = args.order,
				counts = counts,
				entries = entries,
			})
			:build()
	end,
}
