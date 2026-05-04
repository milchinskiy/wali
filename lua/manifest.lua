local COMMON_HOST_KEYS = { "tags", "vars", "run_as", "command_timeout" }
local SSH_KEYS = {
	"user",
	"host",
	"port",
	"host_key_policy",
	"auth",
	"connect_timeout",
	"keepalive_interval",
}
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

local function checked_string(kind, field, value)
	if type(value) ~= "string" then
		error(kind .. " " .. field .. " must be a string", 3)
	end
	if value == "" then
		error(kind .. " " .. field .. " must not be empty", 3)
	end
	if value:match("^%s") ~= nil or value:match("%s$") ~= nil then
		error(kind .. " " .. field .. " must not contain leading or trailing whitespace", 3)
	end
	if value:find("%c") ~= nil then
		error(kind .. " " .. field .. " must not contain control characters", 3)
	end

	return value
end

local function options_table(kind, opts)
	if opts == nil then
		return {}
	end
	if type(opts) ~= "table" then
		error(kind .. " options must be a table", 3)
	end

	return opts
end

local function required_option(kind, opts, key)
	if opts[key] == nil then
		error(kind .. " option '" .. key .. "' is required", 3)
	end

	return opts[key]
end

local function reject_unknown_options(kind, opts, allowed)
	for key, _ in pairs(opts) do
		if not allowed[key] then
			error(kind .. " option '" .. tostring(key) .. "' is not supported", 3)
		end
	end
end

local function copy_keys(out, opts, keys)
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
	opts = options_table("host.localhost", opts)
	reject_unknown_options("host.localhost", opts, LOCAL_HOST_OPTIONS)

	return copy_keys({
		id = checked_string("host.localhost", "id", id),
		transport = "local",
	}, opts, LOCAL_HOST_KEYS)
end

function host.ssh(id, opts)
	opts = options_table("host.ssh", opts)
	reject_unknown_options("host.ssh", opts, SSH_HOST_OPTIONS)
	checked_string("host.ssh", "option 'user'", required_option("host.ssh", opts, "user"))
	checked_string("host.ssh", "option 'host'", required_option("host.ssh", opts, "host"))

	return copy_keys({
		id = checked_string("host.ssh", "id", id),
		transport = { ssh = copy_keys({}, opts, SSH_KEYS) },
	}, opts, COMMON_HOST_KEYS)
end

local function task(id)
	checked_string("task", "id", id)

	return function(module, args, opts)
		opts = options_table("task", opts)
		reject_unknown_options("task", opts, TASK_OPTIONS)

		if args == nil then
			args = {}
		end

		return copy_keys({
			id = id,
			module = checked_string("task", "module", module),
			args = args,
		}, opts, TASK_KEYS)
	end
end

return {
	host = host,
	task = task,
}
