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

	---Validate input arguments (optional)
	---business logic can be added here to validate input arguments
	---in context to actual host state
	---@param ctx table
	---@param args any
	---@return boolean result true if input arguments are valid
	---@error string error message
	validate = function(ctx, args)
		return true
		-- or
		-- error("error message")
	end,

	---Apply changes
	---@param ctx table
	---@param args any
	---@return boolean result true if changes were applied
	apply = function(ctx, args)
		-- do something
		return true
		-- or
		-- error("error message")
	end,
}
