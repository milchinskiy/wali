local lib = require("wali.builtin.lib")

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
			timeout = { type = "number" },
			pty = { type = "enum", values = { "never", "auto", "require" }, default = "auto" },
			creates = { type = "string" },
			removes = { type = "string" },
			changed = { type = "enum", values = { "on_run", "always", "never" }, default = "on_run" },
		},
	},

	validate = function(_, args)
		if args.program == nil and args.script == nil then
			return lib.validation_error("either program or script is required")
		end
		if args.program ~= nil and args.script ~= nil then
			return lib.validation_error("program and script are mutually exclusive")
		end
		return nil
	end,

	apply = function(ctx, args)
		if args.creates ~= nil and ctx.host.fs.exists(args.creates) then
			return lib.result.apply():unchanged(args.creates, "creates guard already exists"):build()
		end
		if args.removes ~= nil and not ctx.host.fs.exists(args.removes) then
			return lib.result.apply():unchanged(args.removes, "removes guard is already absent"):build()
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

		if not output.ok then
			local msg = lib.output_text(output) or lib.status_text(output.status)
			error(msg)
		end

		local result = lib.result.apply()
		if args.changed == "never" then
			result:command("unchanged", detail)
		else
			result:command("updated", detail)
		end
		return result:build()
	end,
}
