local host = {
	localhost = function(id, opts)
		opts = opts or {}

		return {
			id = id,
			transport = "local",
			run_as = opts.run_as or nil,
			tags = opts.tags or nil,
			vars = opts.vars or nil,
		}
	end,

	ssh = function(id, opts)
		opts = opts or {}

		return {
			id = id,
			transport = "ssh",
			host = opts.host,
			user = opts.user,
			port = opts.port or 22,
			run_as = opts.run_as or nil,
			tags = opts.tags or nil,
			vars = opts.vars or nil,
		}
	end,
}

local function task(id)
	return function(module, args, opts)
        opts = opts or {}

		return {
			id = id,
			tags = opts.tags or nil,
			depends_on = opts.depends_on or nil,
			on_change = opts.on_change or nil,
			when = opts.when or nil,
			host = opts.host or nil,
			run_as = opts.run_as or nil,
			vars = opts.vars or nil,
			module = module,
			args = args,
		}
	end
end

return {
	host = host,
	task = task,
}
