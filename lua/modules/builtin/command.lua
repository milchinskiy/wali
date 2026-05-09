local lib = require("wali.builtin.lib")

local function path_list(value, field)
	if value == nil then
		return {}
	end
	if type(value) == "string" then
		return { value }
	end
	if type(value) ~= "table" then
		error(field .. " must be a string or list of strings")
	end
	local out = {}
	for idx, item in ipairs(value) do
		if type(item) ~= "string" then
			error(field .. "[" .. tostring(idx) .. "] must be a string")
		end
		table.insert(out, item)
	end
	return out
end

local function validate_path_list(ctx, value, field)
	local ok, paths_or_error = pcall(path_list, value, field)
	if not ok then
		return lib.validation_error(paths_or_error)
	end
	for _, path in ipairs(paths_or_error) do
		local path_error = lib.validate_absolute_path(ctx, path, field)
		if path_error ~= nil then
			return path_error
		end
	end
	return nil
end

local function all_exist(ctx, paths)
	if #paths == 0 then
		return false
	end
	for _, path in ipairs(paths) do
		if not ctx.host.fs.exists(path) then
			return false
		end
	end
	return true
end

local function all_absent(ctx, paths)
	if #paths == 0 then
		return false
	end
	for _, path in ipairs(paths) do
		if ctx.host.fs.exists(path) then
			return false
		end
	end
	return true
end

return {
	name = "builtin command",
	description = "Run a guarded command or shell script on the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			program = { type = "string" },
			args = { type = "list", items = { type = "string" } },
			script = { type = "string" },
			cwd = { type = "string" },
			env = { type = "map", value = { type = "string" } },
			stdin = { type = "string" },
			timeout = { type = "string" },
			pty = { type = "enum", values = { "never", "auto", "require" }, default = "auto" },
			creates = { type = "any" },
			removes = { type = "any" },
			changed = { type = "boolean", default = true },
		},
	},

	validate = function(ctx, args)
		if args.program == nil and args.script == nil then
			return lib.validation_error("either program or script is required")
		end
		if args.program ~= nil and args.script ~= nil then
			return lib.validation_error("program and script are mutually exclusive")
		end
		if args.program ~= nil and args.program:match("%S") == nil then
			return lib.validation_error("program must not be empty")
		end
		if args.script ~= nil and args.script:match("%S") == nil then
			return lib.validation_error("script must not be empty")
		end

		local cwd_error = lib.validate_optional_absolute_path(ctx, args.cwd, "cwd")
		if cwd_error ~= nil then
			return cwd_error
		end
		local creates_error = validate_path_list(ctx, args.creates, "creates")
		if creates_error ~= nil then
			return creates_error
		end
		return validate_path_list(ctx, args.removes, "removes")
	end,

	apply = function(ctx, args)
		local creates = path_list(args.creates, "creates")
		local removes = path_list(args.removes, "removes")
		if all_exist(ctx, creates) then
			return lib.skip("creates guard already exists: " .. table.concat(creates, ", "))
		end
		if all_absent(ctx, removes) then
			return lib.skip("removes guard is already absent: " .. table.concat(removes, ", "))
		end

		local req = {
			cwd = args.cwd,
			env = args.env,
			stdin = args.stdin,
			timeout = args.timeout,
			pty = args.pty,
		}

		local output
		local detail
		if args.program ~= nil then
			req.program = args.program
			req.args = args.args or {}
			output = ctx.host.cmd.exec(req)
			detail = lib.command_detail("exec", req)
		else
			req.script = args.script
			output = ctx.host.cmd.shell(req)
			detail = lib.command_detail("shell", req.script)
		end

		lib.assert_command_ok(output, detail)

		local result = lib.result.apply()
		for _, path in ipairs(creates) do
			if ctx.host.fs.exists(path) then
				result:created(path, "creates guard was created")
			end
		end
		for _, path in ipairs(removes) do
			if not ctx.host.fs.exists(path) then
				result:removed(path, "removes guard was removed")
			end
		end
		if args.changed then
			result:command("updated", detail)
		else
			result:command("unchanged", detail)
		end
		return result:build()
	end,
}
