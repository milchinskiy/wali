local lib = require("wali.builtin.lib")

local function template_vars(ctx, extra)
	return lib.shallow_merge(ctx.vars or {}, extra or {})
end

local function render_validation_error(err)
	local message = tostring(err or "template render failed")
	if message:find("undefined", 1, true) ~= nil and message:find("missing", 1, true) == nil then
		return "missing template variable: " .. message
	end
	return message
end

local function validate_source_args(args)
	local has_src = args.src ~= nil
	local has_content = args.content ~= nil

	if has_src and has_content then
		return lib.validation_error("exactly one of src or content must be set")
	end
	if not has_src and not has_content then
		return lib.validation_error("one of src or content is required")
	end
	if has_src and args.src == "" then
		return lib.validation_error("src must not be empty")
	end
	return nil
end

return {
	name = "builtin template",
	description = "Render a MiniJinja template and write it to the target host.",

	schema = {
		type = "object",
		required = true,
		props = {
			src = { type = "string" },
			content = { type = "string" },
			dest = { type = "string", required = true },
			vars = { type = "map", value = { type = "any" } },
			create_parents = { type = "boolean", default = false },
			replace = { type = "boolean", default = true },
			mode = lib.schema.mode(),
			owner = lib.schema.owner(),
		},
	},

	validate = function(ctx, args)
		local source_error = validate_source_args(args)
		if source_error ~= nil then
			return source_error
		end
		if args.dest == "" then
			return lib.validation_error("dest must not be empty")
		end

		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		local vars = template_vars(ctx, args.vars)
		local ok, err
		if args.src ~= nil then
			local source = ctx.template.check_source(args.src)
			if not source.ok then
				return lib.validation_error(source.message)
			end
			ok, err = pcall(ctx.template.render_file, args.src, vars)
		else
			ok, err = pcall(ctx.template.render, args.content, vars)
		end

		if not ok then
			return lib.validation_error(render_validation_error(err))
		end

		return nil
	end,

	apply = function(ctx, args)
		local vars = template_vars(ctx, args.vars)
		local content
		if args.src ~= nil then
			content = ctx.template.render_file(args.src, vars)
		else
			content = ctx.template.render(args.content, vars)
		end
		return ctx.host.fs.write(args.dest, content, lib.write_file_opts(args))
	end,
}
