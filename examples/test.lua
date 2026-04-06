return {
	hosts = {
		{
			id = "local",
			transport = "local",
			tags = { "local" },
			vars = { user = "test-user" },
		},
		{
			id = "ssh-test",
			transport = {
				ssh = {
					host = "1.2.3.4",
					user = "test-user",
				},
			},
			tags = { "remote", "ssh" },
			vars = { user = "remote-user" },
		},
	},

	modules = {
		{ path = "../docs" },
	},

	tasks = {
		{
			id = "test task #1",
			tags = { "task-tag-1" },
			when = {
				all = {
					{ hostname = "test-hostname" },
					{ os = "linux" },
					{ arch = "x86_64" },
					{ env_set = "DISPLAY" },
				},
			},
			host = { ["not"] = { tag = "remote" } },
			module = { builtin = "wali.test.module" },
			args = { path1 = "test", path2 = "../examples" },
		},
	},
}
