return {
	base_path = ".",
	target_path = "/tmp",

	hosts = {
		"local",
		{
			ssh = {
				id = "host-1",
				host = "10.77.0.15",
				connect_timeout = "10s",
				command_timeout = "2m",
				keepalive = "1m",
			},
		},
	},

	modules = {
		"test-modules",
		{
			url = "https://github.com/test-modules/test-modules.git",
			ref = "main",
			name = "test-modules-2",
		},
	},

	tasks = {
		{
			id = "test task #1",
			host = "local",
			module = "wali.builtins.link",
			argv = {
				src = "test.file",
				dst = "test.link",
			},
		},
	},
}
