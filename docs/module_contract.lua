return {
	---@type string name of the module
	name = "module name (human readable)",
	---@type string description
	description = "module description (human readable)",

	---Nested table with declared requirements on host
	---@type table
	requires = {
		all = {
			{ command = "wget" },
			{ path = "/tmp" },
		},
		-- or
		any = {
			{ command = "wget" },
			{ command = "curl" },
		},
	},

	schema = {
		type = "object",
		required = true,
		props = {
			arg1 = { type = "string", required = true },
			arg2 = { type = "number", default = 123 },
			arg3 = { type = "list", default = { "a", "b", "c" } },
			arg4 = { type = "union", default = "a", values = { "a", "b", "c" } },
			arg5 = { type = "boolean", default = true },
			arg6 = {
				type = "object",
				default = { key1 = "val1", key2 = "val2" },
				props = { key1 = { type = "string" }, key2 = { type = "number" } },
			},
		},
	},

	---Validate input arguments (optional)
    ---business logic can be added here to validate input arguments
    ---in context to actual host state
    ---@param ctx table
	---@param normalized_args table
	---@return boolean result true if input arguments are valid
	---@error string error message
	validate = function(ctx, normalized_args)
		return true
		-- or
		-- error("error message")
	end,

	---Probe input arguments with actual host state (optional)
    ---@param ctx table
	---@param normalized_args table
	---@return boolean result true if input arguments and host state are ready to apply
	probe = function(ctx, normalized_args)
		return true
		-- or
		-- error("error message")
	end,

	---Generate list of changes that will be applied (optional)
	---if not provided or returns nil or empty table, assumed changes will be "unknown"
    ---@param ctx table
	---@param normalized_args table
	---@return table|nil changes
	assumed_changes = function(ctx, normalized_args)
		return {
			{ path = "path/to/file-1", action = "created" },
			{ path = "path/to/file-2", action = "deleted" },
			{ path = "path/to/file-3", action = "updated" },
			-- other kinds of actions might be added later
		}
	end,

	---Apply changes
    ---@param ctx table
	---@param normalized_args table
	---@return table|nil changes
	apply = function(ctx, normalized_args)
		-- do something
		-- and return the actual changes state (if applicable)
		return {
			{ path = "path/to/file-1", action = "created" },
			{ path = "path/to/file-2", action = "deleted" },
			{ path = "path/to/file-3", action = "updated" },
		}
	end,

	---Revert changes (optional)
    ---@param ctx table
	---@param normalized_args table
	---@param apply_state table state from `apply` stage if applicable
	---@return table|nil changes
	revert = function(ctx, normalized_args, apply_state)
		-- do something
		-- and return the actual changes state (if applicable)
		return {
			{ path = "path/to/file-1", action = "deleted" },
			{ path = "path/to/file-2", action = "created" },
			{ path = "path/to/file-3", action = "updated" },
		}
	end,

	---Cleanup changes (optional)
    ---@param ctx table
	---@param normalized_args table
	---@param apply_state table state from `apply` stage if applicable
	---@return table|nil changes
	cleanup = function(ctx, normalized_args, apply_state)
		-- do something
		-- and return the actual changes state (if applicable)
		return {
			{ path = "path/to/file-1", action = "deleted" },
			{ path = "path/to/file-2", action = "created" },
			{ path = "path/to/file-3", action = "updated" },
		}
	end,
}
