return {
	name = "Test wali manifest file",

	hosts = {
		["local"] = {
			transport = "local",
			tags = { "local" },
			vars = { user = "test-user" },
		},
		["ssh-test"] = {
			transport = {
				ssh = {
					host = "1.2.3.4",
					user = "test-user",
				},
			},
			tags = { "remote", "ssh" },
			vars = { user = "remote-user" },

			run_as = {
				["doas-test"] = {
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
		["test task #1"] = {
			tags = { "task-tag-1" },
			when = {
				all = {
					{ hostname = "test-hostname" },
					{ os = "linux" },
					{ arch = "x86_64" },
					{ env_set = "DISPLAY" },
				},
			},
			host = { id = "ssh-test" },
			run_as = "doas-test",
			module = "wali.test.module",
			args = { path1 = "test", path2 = "../examples" },
		},
	},
}
