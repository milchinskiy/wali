local m = require("manifest")

local root = "/tmp/wali-all-modules-example"
local controller_out = "/tmp/wali-all-modules-example-controller"

local function p(...)
	local path = root
	for _, part in ipairs({ ... }) do
		path = path .. "/" .. part
	end
	return path
end

local function controller_path(name)
	return controller_out .. "/" .. name
end

local function after(...)
	return { depends_on = { ... } }
end

return {
	name = "All built-in modules example",
	base_path = ".",

	vars = {
		app = "wali-all-modules-example",
		root = root,
	},

	hosts = {
		m.host.localhost("localhost", {
			tags = { "local", "example" },
			vars = { role = "local-controller" },
		}),
	},

	tasks = {
		m.task("create workspace")("wali.builtin.dir", {
			path = root,
			state = "present",
			parents = true,
			mode = "0755",
		}),

		m.task("create source tree")("wali.builtin.dir", {
			path = p("source"),
			state = "present",
			parents = true,
			mode = "0755",
		}, after("create workspace")),

		m.task("create work directory")("wali.builtin.dir", {
			path = p("work"),
			state = "present",
			parents = true,
			mode = "0755",
		}, after("create workspace")),

		m.task("write source file")("wali.builtin.file", {
			path = p("source", "source.txt"),
			state = "present",
			content = "managed by wali\n",
			create_parents = true,
			replace = true,
		}, after("create source tree")),

		m.task("touch marker file")("wali.builtin.touch", {
			path = p("source", "marker.txt"),
			create_parents = true,
			mode = "0644",
		}, after("create source tree")),

		m.task("enforce source file permissions")("wali.builtin.permissions", {
			path = p("source", "source.txt"),
			expect = "file",
			mode = "0644",
		}, after("write source file")),

		m.task("create source symlink")("wali.builtin.link", {
			path = p("source", "source.link"),
			target = p("source", "source.txt"),
			state = "present",
			replace = true,
		}, after("write source file")),

		m.task("copy source file")("wali.builtin.copy_file", {
			src = p("source", "source.txt"),
			dest = p("work", "source-copy.txt"),
			create_parents = true,
			replace = true,
			preserve_mode = true,
		}, after("enforce source file permissions", "create work directory")),

		m.task("render inline template")("wali.builtin.template", {
			content = "app={{ app }}\nrole={{ role }}\nroot={{ root }}\nnote={{ note }}\n",
			dest = p("work", "rendered.conf"),
			vars = { note = "rendered by wali.builtin.template" },
			create_parents = true,
			replace = true,
			mode = "0644",
		}, after("create work directory")),

		m.task("run guarded command")("wali.builtin.command", {
			program = "sh",
			args = { "-c", "printf 'command example\\n' > " .. p("work", "command.txt") },
			creates = p("work", "command.txt"),
		}, after("create work directory")),

		m.task("push controller file")("wali.builtin.push_file", {
			src = "test.lua",
			dest = p("work", "pushed-test.lua"),
			create_parents = true,
			replace = true,
			mode = "0644",
		}, after("create work directory")),

		m.task("pull pushed file")("wali.builtin.pull_file", {
			src = p("work", "pushed-test.lua"),
			dest = controller_path("pulled-test.lua"),
			create_parents = true,
			replace = true,
			mode = "0644",
		}, after("push controller file")),

		m.task("copy tree preserving symlinks")("wali.builtin.copy_tree", {
			src = p("source"),
			dest = p("tree-copy-preserve"),
			replace = true,
			preserve_mode = true,
			symlinks = "preserve",
			dir_mode = "0755",
			file_mode = "0644",
		}, after("touch marker file", "create source symlink", "enforce source file permissions")),

		m.task("copy tree skipping symlinks")("wali.builtin.copy_tree", {
			src = p("source"),
			dest = p("tree-copy-skip"),
			replace = true,
			preserve_mode = true,
			symlinks = "skip",
			dir_mode = "0755",
			file_mode = "0644",
		}, after("touch marker file", "create source symlink", "enforce source file permissions")),

		m.task("link source tree")("wali.builtin.link_tree", {
			src = p("source"),
			dest = p("tree-link"),
			replace = true,
			dir_mode = "0755",
		}, after("touch marker file", "create source symlink", "enforce source file permissions")),

		m.task("remove optional stale file")("wali.builtin.remove", {
			path = p("stale.txt"),
		}, after("create workspace")),
	},
}
