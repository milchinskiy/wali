-- Lua execution phases:
--
-- requires  -> checked by Rust against the effective backend before module Lua
--              validation/apply. It is for host capability checks only.
-- validate  -> receives a read/probe-only ctx. It may inspect facts, paths,
--              target-host metadata, directory listings, file contents,
--              symlink targets, tree walk output, and controller-side read-only
--              files through ctx.controller.
--              It must not mutate host state and does not expose ctx.host.cmd,
--              ctx.rand, ctx.sleep_ms, or transfer mutation helpers.
-- apply     -> receives the full ctx, including mutating target filesystem
--              functions, command execution, transfer mutation helpers,
--              random helpers, and sleep_ms.
--
-- wali check runs requires + validate only. It never calls apply().
--
-- Common ctx fields:
--   ctx.phase                     "validate" or "apply"
--   ctx.task.id                   task id
--   ctx.task.module               module name
--   ctx.task.tags                 task tags
--   ctx.task.depends_on           task dependency ids; dependencies must succeed before this task runs
--   ctx.task.on_change            change-gated dependency ids; apply runs this task only if any changed
--   ctx.vars                      effective manifest/host/task variables; shallow merge, task wins
--   ctx.run_as                    optional effective run_as spec
--   ctx.host.id                   host id
--   ctx.host.transport            "local" or "ssh"
--   ctx.host.facts.*              os/arch/hostname/env/user/group/which/etc
--   ctx.host.path.*               join/normalize/parent/is_absolute/basename/strip_prefix
--   ctx.controller.path.*         resolve/is_absolute/join/normalize/parent/basename/strip_prefix
--   ctx.controller.fs.*           read-only controller filesystem helpers
--   ctx.codec.*                   byte/string codec helpers
--   ctx.hash.*                    one-way byte digest helpers
--   ctx.json.*                    JSON decode/encode helpers
--   ctx.template.*                pure MiniJinja rendering helpers
--   ctx.transfer.*                controller/host file transfer helpers
--
-- validate ctx.controller.fs exposes read-only controller helpers:
--   metadata, stat, lstat, exists, read, read_text, list_dir, walk, read_link
--   list_dir output is sorted deterministically by entry name.
--   walk output uses lstat-style metadata, does not follow symlinks, and
--   defaults to deterministic pre-order.
--
-- validate ctx.codec exposes pure byte/string codec helpers:
--   base64_encode(bytes), base64_decode(text)
--
-- validate ctx.hash exposes pure one-way byte digest helpers:
--   sha256(bytes)
--
-- validate ctx.json exposes pure JSON helpers:
--   decode(text), encode(value), encode_pretty(value)
--   JSON null maps to the global null sentinel.
--
-- validate ctx.template exposes pure rendering helpers:
--   render(source, vars)
--
-- validate ctx.transfer is present, but controller-side inspection belongs in
-- ctx.controller.fs. Transfer mutation helpers are apply-only.
--
-- validate ctx.host.fs exposes only read/probe helpers:
--   metadata, stat, lstat, exists, read, read_text, list_dir, walk, read_link
--   list_dir output is sorted deterministically by entry name.
--
-- apply ctx.transfer additionally exposes mutation helpers:
--   push_file(src, dest, opts), pull_file(src, dest, opts)
--   relative controller paths are resolved against manifest base_path.
--
-- apply ctx.host.fs additionally exposes mutation helpers:
--   write, copy_file, create_dir, remove_file, remove_dir, mktemp, chmod,
--   chown, rename, symlink
--   remove_dir refuses unsafe lexical targets such as "/", ".", "..", and "../x".
--   rename uses exact-path semantics and refuses existing directory destinations.
--
-- apply ctx.host.cmd exposes command execution helpers:
--   exec, shell
--   request env values are maps: { FOO = "bar" }
--   request stdin values are strings passed as child-process input bytes
--   timeout values are human strings such as "10s" or "2m"
--   when omitted, host.command_timeout is used if configured
--   split commands return stdout/stderr; PTY commands return combined output
--   option/request tables and module result tables reject unknown fields.

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
	---  { ["not"] = { command = "busybox" } }
	---  { all = { ... } }
	---  { any = { ... } }
	--- Empty all/any lists and empty or whitespace-only string arguments are invalid.
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

	---Apply the task. Return an ExecutionResult-compatible table.
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

	-- Apply-result rules:
	--   changed fs_entry records (created/updated/removed) require a non-empty
	--   absolute target-host path;
	--   unchanged fs_entry records may omit path when no concrete resource changed;
	--   command records use detail; path is ignored for command changes;
	--   whitespace-only message/detail fields are treated as absent.
}

-- Builtin modules are reserved under the wali.builtin.* namespace:
--   wali.builtin.dir
--   wali.builtin.file
--   wali.builtin.copy_file
--   wali.builtin.push_file
--   wali.builtin.pull_file
--   wali.builtin.link
--   wali.builtin.remove
--   wali.builtin.touch
--   wali.builtin.link_tree
--   wali.builtin.copy_tree
--   wali.builtin.permissions
--   wali.builtin.command
--   wali.builtin.template
-- Shared builtin Lua helpers are available as wali.builtin.lib.

-- Shared helper library for custom modules:
--   local lib = require("wali.builtin.lib")
--
-- Stable helper groups:
--   lib.result.apply(), lib.result.validation()
--   lib.validation_ok(message?), lib.validation_error(message)
--   lib.schema.mode(), lib.schema.owner()
--   lib.mode_bits("0644"), lib.owner({ user = "root", group = 0 })
--   lib.validate_mode_owner(args, spec?)
--   lib.mode_owner_opts(args, spec?)
--   lib.apply_mode_owner(ctx, result, path, args, spec?) -- apply phase only
--   lib.validate_absolute_path(ctx, path, field?)
--   lib.validate_safe_remove_path(ctx, path)
--   lib.validate_tree_roots(ctx, src, dest)
--   lib.output_text(output), lib.status_text(status)
--   lib.command_error(output, detail?), lib.assert_command_ok(output, detail?)
--   lib.is_file(metadata), lib.is_dir(metadata), lib.is_symlink(metadata)
--
-- Helpers only compose public ctx.host.* primitives.
-- Use them when you want manifest mode strings and validation/apply errors to
-- match wali's builtins.
