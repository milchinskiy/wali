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

local function resolved_path(ctx, path)
	local ok, resolved_or_error = pcall(ctx.controller.path.resolve, path)
	if ok then
		return resolved_or_error
	end
	return path
end

local function read_template_source(ctx, src)
	local ok, metadata_or_error = pcall(ctx.controller.fs.metadata, src)
	if not ok then
		return nil, metadata_or_error
	end
	local metadata = metadata_or_error
	if metadata == nil then
		return nil, "template source does not exist: " .. resolved_path(ctx, src)
	end
	if metadata.kind ~= "file" then
		return nil, "template source must be a regular file: " .. resolved_path(ctx, src)
	end

	local content_ok, content_or_error = pcall(ctx.controller.fs.read_text, src)
	if not content_ok then
		return nil, content_or_error
	end
	return content_or_error, nil
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
		local dest_error = lib.validate_absolute_path(ctx, args.dest, "dest")
		if dest_error ~= nil then
			return dest_error
		end

		local metadata_error = lib.validate_mode_owner(args)
		if metadata_error ~= nil then
			return metadata_error
		end

		local source = args.content
		if args.src ~= nil then
			local err
			source, err = read_template_source(ctx, args.src)
			if err ~= nil then
				return lib.validation_error(err)
			end
		end

		local ok, err = pcall(ctx.template.render, source, template_vars(ctx, args.vars))
		if not ok then
			return lib.validation_error(render_validation_error(err))
		end

		return nil
	end,

	apply = function(ctx, args)
		local source = args.content
		if args.src ~= nil then
			local err
			source, err = read_template_source(ctx, args.src)
			if err ~= nil then
				error(err)
			end
		end
		local content = ctx.template.render(source, template_vars(ctx, args.vars))
		return ctx.host.fs.write(args.dest, content, lib.write_file_opts(args))
	end,
}
