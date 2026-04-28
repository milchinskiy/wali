-- Lua execution phases:
--
-- requires  -> checked by Rust against the effective backend before module Lua
--              validation/apply. It is for host capability checks only.
-- validate  -> receives a read/probe-only ctx. It may inspect facts, paths,
--              metadata, directory listings, file contents, symlink targets,
--              and tree walk output. It must not mutate host state and does
--              not expose ctx.host.cmd, ctx.rand, or ctx.sleep_ms.
-- apply     -> receives the full ctx, including mutating filesystem functions,
--              command execution, random helpers, and sleep_ms.
--
-- wali check runs requires + validate only. It never calls apply().
--
-- Common ctx fields:
--   ctx.phase                     "validate" or "apply"
--   ctx.task.id                   task id
--   ctx.task.module               module name
--   ctx.task.tags                 task tags
--   ctx.task.depends_on           task dependency ids
--   ctx.vars                      task variables
--   ctx.run_as                    optional effective run_as spec
--   ctx.host.id                   host id
--   ctx.host.transport            "local" or "ssh"
--   ctx.host.facts.*              os/arch/hostname/env/user/group/which/etc
--   ctx.host.path.*               join/normalize/parent
--
-- validate ctx.host.fs exposes only read/probe helpers:
--   metadata, stat, lstat, exists, read, list_dir, walk, read_link
--
-- apply ctx.host.fs additionally exposes mutation helpers:
--   write, copy_file, create_dir, remove_file, remove_dir, mktemp, chmod,
--   chown, rename, symlink
--
-- apply ctx.host.cmd exposes command execution helpers:
--   exec, shell

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

    ---Input arguments (required).
    ---Supported argument types:
    ---  any
    ---  null
    ---  string
    ---  number
    ---  integer
    ---  boolean
    ---  list
    ---  tuple
    ---  enum
    ---  object
    ---  map
    ---@type table
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
	---@return { changes: table[], message: string?, data: any? }
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
			data = { optional = "structured machine-readable payload" },
		}
		-- or
		-- error("error message")
	end,
}

-- Builtin modules are reserved under the wali.builtin.* namespace:
--   wali.builtin.dir
--   wali.builtin.file
--   wali.builtin.copy_file
--   wali.builtin.link
--   wali.builtin.remove
--   wali.builtin.touch
--   wali.builtin.link_tree
--   wali.builtin.copy_tree
--   wali.builtin.permissions
--   wali.builtin.command
-- Shared builtin Lua helpers are available as wali.builtin.lib.
