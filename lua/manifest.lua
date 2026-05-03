local COMMON_HOST_KEYS = { "tags", "vars", "run_as", "command_timeout" }
local SSH_KEYS = { "user", "host", "port", "host_key_policy", "auth", "connect_timeout", "keepalive_interval" }
local TASK_KEYS = { "tags", "depends_on", "on_change", "when", "host", "run_as", "vars" }

local function join_keys(left, right)
	local out = {}

	for _, key in ipairs(left) do
		out[#out + 1] = key
	end
	for _, key in ipairs(right) do
		out[#out + 1] = key
	end

	return out
end

local LOCAL_HOST_KEYS = COMMON_HOST_KEYS
local SSH_HOST_KEYS = join_keys(COMMON_HOST_KEYS, SSH_KEYS)

local function option_set(keys)
	local out = {}

	for _, key in ipairs(keys) do
		out[key] = true
	end

	return out
end

local LOCAL_HOST_OPTIONS = option_set(LOCAL_HOST_KEYS)
local SSH_HOST_OPTIONS = option_set(SSH_HOST_KEYS)
local TASK_OPTIONS = option_set(TASK_KEYS)

local function options_or_empty(kind, opts)
	if opts == nil then
		return {}
	end
	if type(opts) ~= "table" then
		error(kind .. " options must be a table", 3)
	end

	return opts
end

local function check_options(kind, opts, allowed)
	for key, _ in pairs(opts) do
		if not allowed[key] then
			error(kind .. " option '" .. tostring(key) .. "' is not supported", 3)
		end
	end
end

local function copy_fields(out, opts, keys)
	for _, key in ipairs(keys) do
		local value = opts[key]
		if value ~= nil then
			out[key] = value
		end
	end

	return out
end

local host = {}

function host.localhost(id, opts)
	opts = options_or_empty("host.localhost", opts)
	check_options("host.localhost", opts, LOCAL_HOST_OPTIONS)

	return copy_fields({
		id = id,
		transport = "local",
	}, opts, LOCAL_HOST_KEYS)
end

function host.ssh(id, opts)
	opts = options_or_empty("host.ssh", opts)
	check_options("host.ssh", opts, SSH_HOST_OPTIONS)

	return copy_fields({
		id = id,
		transport = { ssh = copy_fields({}, opts, SSH_KEYS) },
	}, opts, COMMON_HOST_KEYS)
end

local function task(id)
	return function(module, args, opts)
		opts = options_or_empty("task", opts)
		check_options("task", opts, TASK_OPTIONS)

		if args == nil then
			args = {}
		end

		return copy_fields({
			id = id,
			module = module,
			args = args,
		}, opts, TASK_KEYS)
	end
end

return {
	host = host,
	task = task,
}
