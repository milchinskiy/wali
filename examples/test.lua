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
		{ namespace = "custom1", path = "./custom-mods" },
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
			id = "copy demo file",
			module = "wali.builtin.copy_file",
			args = {
				src = "/tmp/wali-demo/hello.txt",
				dest = "/tmp/wali-demo/hello-copy.txt",
				replace = true,
				preserve_mode = true,
			},
		},
		{
			id = "write stale file",
			module = "wali.builtin.file",
			args = {
				path = "/tmp/wali-demo/stale.txt",
				content = "I'll be removed soon by wali\n",
				mode = "0644",
			},
		},
		{
			id = "touch marker file",
			module = "wali.builtin.touch",
			args = {
				path = "/tmp/wali-demo/marker",
				mode = "0644",
			},
		},
		{
			id = "enforce demo file permissions",
			module = "wali.builtin.permissions",
			args = {
				path = "/tmp/wali-demo/hello.txt",
				expect = "file",
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
			id = "remove stale demo file",
			module = "wali.builtin.remove",
			args = {
				path = "/tmp/wali-demo/stale.txt",
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
		{
			id = "link demo tree",
			module = "wali.builtin.link_tree",
			args = {
				src = "/tmp/wali-demo",
				dest = "/tmp/wali-demo-linked",
				replace = true,
				dir_mode = "0755",
			},
		},
		{
			id = "copy demo tree",
			module = "wali.builtin.copy_tree",
			args = {
				src = "/tmp/wali-demo",
				dest = "/tmp/wali-demo-copied",
				replace = true,
				preserve_mode = true,
				symlinks = "preserve",
			},
		},
		{
			id = "inspect demo tree",
			module = "wali.builtin.walk",
			args = {
				path = "/tmp/wali-demo",
				include_root = true,
				order = "pre",
			},
		},

        {
            id = "test custom module",
            module = "custom1.test_module",
            args = {
                source = "/tmp/wali-demo",
                target = "/tmp/wali-demo-custom",
            },
        },
	},
}
