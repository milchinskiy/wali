local m = require("manifest")

return {
	name = "Test wali manifest file",

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
		m.task("create demo directory")("wali.builtin.dir", {
			path = "/tmp/wali-demo",
			state = "present",
			mode = "0755",
			parents = true,
		}),
		m.task("write demo file")("wali.builtin.file", {
			path = "/tmp/wali-demo/hello.txt",
			content = "hello from wali\n",
			mode = "0644",
		}),
		m.task("copy demo file")("wali.builtin.copy_file", {
			src = "/tmp/wali-demo/hello.txt",
			dest = "/tmp/wali-demo/hello-copy.txt",
			replace = true,
			preserve_mode = true,
		}),
		m.task("create stale file")("wali.builtin.file", {
			path = "/tmp/wali-demo/stale.txt",
			content = "I'll be removed soon by wali\n",
			mode = "0644",
		}),
		m.task("touch marker file")("wali.builtin.touch", {
			path = "/tmp/wali-demo/marker",
			mode = "0644",
		}),
		m.task("enforce demo file permissions")("wali.builtin.permissions", {
			path = "/tmp/wali-demo/hello.txt",
			expect = "file",
			mode = "0644",
		}),
		m.task("link demo file")("wali.builtin.link", {
			path = "/tmp/wali-demo/hello.link",
			target = "/tmp/wali-demo/hello.txt",
			replace = true,
		}),
		m.task("remove stale demo file")("wali.builtin.remove", {
			path = "/tmp/wali-demo/stale.txt",
		}),
		m.task("run guarded command")("wali.builtin.command", {
			program = "sh",
			args = { "-c", "printf command-ran > /tmp/wali-demo/command.txt" },
			creates = "/tmp/wali-demo/command.txt",
		}),
		m.task("link demo tree")("wali.builtin.link_tree", {
			src = "/tmp/wali-demo",
			dest = "/tmp/wali-demo-linked",
			replace = true,
			dir_mode = "0755",
		}),
		m.task("copy demo tree")("wali.builtin.copy_tree", {
			src = "/tmp/wali-demo",
			dest = "/tmp/wali-demo-copied",
			replace = true,
			preserve_mode = true,
			symlinks = "preserve",
		}),
		m.task("run custom module")("custom1.test_module", {
			source = "/tmp/wali-demo",
			target = "/tmp/wali-demo-custom",
		}),
	},
}
