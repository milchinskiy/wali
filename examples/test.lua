local m = require("manifest")

return {
	name = "Test wali manifest file",

	vars = {
		demo_root = "/tmp/wali-demo",
	},

	hosts = {
		m.host.localhost("localhost", {
			tags = { "local" },
			vars = { user = "test-user" },
		}),
	},

	modules = {
		{ namespace = "custom1", path = "./custom-mods" },
	},

	tasks = {
		m.task("create demo directory")("wali.builtin.mkdir", {
			path = "{{ demo_root }}",
			mode = "0755",
			parents = true,
		}),
		m.task("write demo file")("wali.builtin.write", {
			dest = "{{ demo_root }}/hello.txt",
			content = "hello from wali for {{ user }}\n",
			mode = "0644",
		}),
		m.task("copy demo file")("wali.builtin.copy", {
			src = "{{ demo_root }}/hello.txt",
			dest = "{{ demo_root }}/hello-copy.txt",
			replace = true,
			preserve_mode = true,
		}),
		m.task("create stale file")("wali.builtin.write", {
			dest = "{{ demo_root }}/stale.txt",
			content = "I'll be removed soon by wali\n",
			mode = "0644",
		}),
		m.task("touch marker file")("wali.builtin.touch", {
			path = "{{ demo_root }}/marker",
			mode = "0644",
		}),
		m.task("enforce demo file permissions")("wali.builtin.permissions", {
			path = "{{ demo_root }}/hello.txt",
			expect = "file",
			mode = "0644",
		}),
		m.task("link demo file")("wali.builtin.link", {
			dest = "{{ demo_root }}/hello.link",
			src = "{{ demo_root }}/hello.txt",
			replace = true,
		}),
		m.task("remove stale demo file")("wali.builtin.remove", {
			path = "{{ demo_root }}/stale.txt",
		}),
		m.task("run guarded command")("wali.builtin.command", {
			program = "sh",
			args = { "-c", "printf command-ran > {{ demo_root }}/command.txt" },
			creates = "{{ demo_root }}/command.txt",
		}),
		m.task("link demo tree")("wali.builtin.link", {
			src = "{{ demo_root }}",
			dest = "{{ demo_root }}-linked",
			recursive = true,
			replace = true,
			dir_mode = "0755",
		}),
		m.task("copy demo tree")("wali.builtin.copy", {
			src = "{{ demo_root }}",
			dest = "{{ demo_root }}-copied",
			recursive = true,
			replace = true,
			preserve_mode = true,
			symlinks = "preserve",
		}),
		m.task("run custom module")("custom1.test_module", {
			source = "{{ demo_root }}",
			target = "{{ demo_root }}-custom",
		}),
	},
}
