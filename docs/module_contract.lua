return {
	---@type string name of the module
	name = "module name (human readable)",
	---@type string description
	description = "module description (human readable)",

	---Host requirements checked before validate() and apply().
	---Supported requirement forms:
	---  { command = "wget" }
	---  { path = "/tmp" }
	---  { env = "HOME" }
	---  { os = "linux" }
	---  { arch = "x86_64" }
	---  { hostname = "web-1" }
	---  { user = "root" }
	---  { group = "root" }
	---  { not = { command = "busybox" } }
	---  { all = { ... } }
	---  { any = { ... } }
	---@type table
	requires = {
		all = {
			{ path = "/tmp" },
			{
				any = {
					{ command = "wget" },
					{ command = "curl" },
				},
			},
		},
	},

	schema = {
		type = "object",
		required = true,
		props = {
			arg1 = { type = "any", required = true, default = 123 },
			arg2 = { type = "null", default = null },
			arg3 = { type = "string", required = true, default = "test string" },
			arg4 = { type = "number", default = 1.23 },
			arg5 = { type = "integer", default = -123 },
			arg6 = { type = "boolean", default = true },
			arg7 = { type = "list", items = { type = "number" }, default = { 1, 2, 3 } },
			arg8 = { type = "tuple", items = { { type = "number" }, { type = "integer" } }, default = { 1, 1.23 } },
			arg9 = { type = "enum", values = { "a", "b", "c", null }, default = null },
			arg10 = { type = "object", props = { a = { type = "string" } }, default = { a = "test" } },
			arg11 = { type = "map", value = { type = "string" }, default = { a = "test" } },
		},
	},

	---Validate input arguments (optional). Return nil/{ ok = true } when valid.
	---@param ctx table
	---@param args any
	---@return nil|{ ok: boolean, message: string? }
	validate = function(ctx, args)
		-- nil means ok
		return nil
		-- or
		-- return { ok = false, message = "error message" }
		-- or
		-- error("unexpected validation error")
	end,

	---Apply desired state. Return an ExecutionResult-compatible table.
	---@param ctx table
	---@param args any
	---@return { changes: table[], message: string? }
	apply = function(ctx, args)
		-- do something
		return {
			changes = {
				{ kind = "created", subject = "fs_entry", path = "/tmp/example" },
				{ kind = "updated", subject = "fs_entry", path = "/tmp/example.conf" },
				{ kind = "removed", subject = "fs_entry", path = "/tmp/old-example" },
				{ kind = "unchanged", subject = "fs_entry", path = "/tmp/already-ok" },
			},
			message = "optional human summary",
		}
		-- or
		-- error("error message")
	end,
}

-- Builtin modules are reserved under the wali.builtin.* namespace:
--   wali.builtin.dir
--   wali.builtin.file
--   wali.builtin.link
--   wali.builtin.remove
--   wali.builtin.command
-- Shared builtin Lua helpers are available as wali.builtin.lib.
