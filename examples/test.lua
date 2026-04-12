return {
	name = "Test wali manifest file",

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
			vars = { DISPLAY = ":1" },

			run_as = {
				{
					id = "doas-test",
					user = "test",
					via = "doas",
					env_policy = { keep = { "PATH", "HOME" } },
				},
			},
		},
	},

	modules = {
		{ path = "./custom-mods" },
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
            depends_on = { "task #2" },
			module = "test_module",
			args = { path1 = "test", path2 = "../examples" },
		},
        {
            id = "task #2",
            module = "test_module",
            args = {},
        }
	},
}
