return {
	hosts = {
		{
			id = "local",
			transport = "local",
			tags = { "local" },
			vars = { user = "test-user" },
		},
		-- { id = "local", transport = "local" },
	},

    modules = {
        { path = "../docs" },
        { path = "../docs" },
    },

	tasks = {},
}
