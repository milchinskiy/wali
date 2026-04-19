return {
	name = "Test wali manifest file",

	hosts = {
		{
			id = "localhost",
			transport = "local",
			tags = { "local" },
			vars = { user = "test-user" },
			-- run_as = {
			-- 	{
			-- 		id = "doas-test",
			-- 		user = "test2",
			-- 		via = "doas",
			-- 		env_policy = { keep = { "PATH", "HOME" } },
			-- 	},
			-- },
		},
        {
            id = "another host",
            transport = "local",
        },
        {
            id = "some remote host #3",
            transport = "local",
        }
		-- {
		-- 	id = "ssh-test",
		-- 	transport = {
		-- 		ssh = {
		-- 			host = "127.0.0.2",
		-- 			user = "test-user",
		-- 		},
		-- 	},
		-- 	tags = { "remote", "ssh" },
		-- 	vars = { DISPLAY = ":1" },
		--
		-- 	run_as = {
		-- 		{
		-- 			id = "doas-test",
		-- 			user = "test",
		-- 			via = "doas",
		-- 			env_policy = { keep = { "PATH", "HOME" } },
		-- 		},
		-- 	},
		-- },
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
			args = { source = "test", target = "../examples" },
		},
		{
			id = "task #2",
			module = "test_module",
			args = { target = "some/path" },
		},
        {
			id = "create home dir",
			module = "test_module",
			args = { target = "some/path" },
		},
        {
			id = "link Alacritty config",
			module = "test_module",
			args = { target = "some/path" },
		},
        {
			id = "write git config",
			module = "test_module",
			args = { target = "some/path" },
		},
	},
}
