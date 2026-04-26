return {
	name = "Test wali manifest file",

	hosts = {
		{
			id = "localhost",
			transport = "local",
			tags = { "local" },
			vars = { user = "test-user" },
		},
	},

	modules = {
		{ path = "./custom-mods" },
	},

	tasks = {
		{
			id = "create demo directory",
			module = "wali.builtin.dir",
			args = {
				path = "/tmp/wali-demo",
				state = "present",
				mode = "0755",
				parents = true,
			},
		},
		{
			id = "write demo file",
			module = "wali.builtin.file",
			args = {
				path = "/tmp/wali-demo/hello.txt",
				content = "hello from wali\n",
				mode = "0644",
			},
		},
		{
			id = "link demo file",
			module = "wali.builtin.link",
			args = {
				path = "/tmp/wali-demo/hello.link",
				target = "/tmp/wali-demo/hello.txt",
				replace = true,
			},
		},
		{
			id = "run guarded command",
			module = "wali.builtin.command",
			args = {
				program = "sh",
				args = { "-c", "printf command-ran > /tmp/wali-demo/command.txt" },
				creates = "/tmp/wali-demo/command.txt",
			},
		},
	},
}
