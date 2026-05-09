---@meta
---@module 'wali.builtin.lib'

---@class WaliBuiltinLib
local lib = {}

---@type WaliApiResult
lib.result = nil

---@class WaliBuiltinLibSchema
lib.schema = {}

---@param default? string
---@return WaliSchema
function lib.schema.mode(default) end

---@param default? WaliOwner
---@return WaliSchema
function lib.schema.owner(default) end

---@return table<string, WaliSchema>
function lib.schema.owner_props() end

---@generic T: table, U: table
---@param base T?
---@param extra U?
---@return table
function lib.shallow_merge(base, extra) end

---@param message? string
---@return WaliValidationResult
function lib.validation_ok(message) end

---@param message any
---@return WaliValidationResult
function lib.validation_error(message) end

---@param ctx WaliApplyCtx
---@param helper? string
function lib.require_apply(ctx, helper) end

---@param value? string
---@return integer?
function lib.mode_bits(value) end

---@param value? string
---@return WaliValidationResult?
function lib.validate_mode(value) end

---@param value? WaliOwner
---@return WaliOwner?
function lib.owner(value) end

---@param value? WaliOwner
---@param field? string
---@return WaliValidationResult?
function lib.validate_owner(value, field) end

---@class WaliModeOwnerSpec
---@field mode? string Argument field name containing an octal mode string. Defaults to 'mode'.
---@field owner? string Argument field name containing a WaliOwner object. Defaults to 'owner'.

---@param args table
---@param spec? WaliModeOwnerSpec
---@return WaliValidationResult?
function lib.validate_mode_owner(args, spec) end

---@param args table
---@param spec? WaliModeOwnerSpec
---@return boolean
function lib.has_mode_owner(args, spec) end

---@param args table
---@param spec? WaliModeOwnerSpec
---@return WaliWriteOpts
function lib.mode_owner_opts(args, spec) end

---@param ctx WaliApplyCtx
---@param result WaliApplyResultBuilder
---@param path string
---@param args table
---@param spec? WaliModeOwnerSpec
---@return WaliApplyResultBuilder
function lib.apply_mode_owner(ctx, result, path, args, spec) end

---@param args table
---@return WaliWriteOpts
function lib.write_file_opts(args) end

---@param args table
---@return WaliDirOpts
function lib.create_dir_opts(args) end

---@param args table
---@return WaliCopyFileOpts
function lib.copy_file_opts(args) end

---@param args table
---@return WaliPullFileOpts
function lib.pull_file_opts(args) end

---@param args table
---@return WaliPushTreeOpts
function lib.push_tree_opts(args) end

---@param args table
---@return WaliPullTreeOpts
function lib.pull_tree_opts(args) end

---@param metadata? WaliMetadata
---@return WaliOwner?
function lib.owner_from_metadata(metadata) end

---@param explicit_owner? WaliOwner
---@param preserve_owner boolean
---@param metadata? WaliMetadata
---@return WaliOwner?
function lib.owner_or_preserved(explicit_owner, preserve_owner, metadata) end

---@param args table
---@param metadata WaliMetadata
---@return WaliDirOpts
function lib.tree_dir_opts(args, metadata) end

---@param args table
---@param metadata WaliMetadata
---@return WaliCopyFileOpts
function lib.tree_copy_file_opts(args, metadata) end

---@param args table
---@return WaliDirOpts
function lib.link_tree_dir_opts(args) end

---@param output? WaliCommandOutput
---@return string?
function lib.output_text(output) end

---@param status? WaliCommandStatus
---@return string
function lib.status_text(status) end

---@param output? WaliCommandOutput
---@param detail? string
---@return string
function lib.command_error(output, detail) end

---@param output? WaliCommandOutput
---@param detail? string
---@return WaliCommandOutput
function lib.assert_command_ok(output, detail) end

---@overload fun(kind: 'shell', value: string): string
---@overload fun(kind: 'exec', value: WaliExecCommandRequest): string
---@param kind 'exec'|'shell'
---@param value string|WaliExecCommandRequest
---@return string
function lib.command_detail(kind, value) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param path string
---@param field? string
---@return WaliValidationResult?
function lib.validate_absolute_path(ctx, path, field) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param args table
---@param fields string[]
---@return WaliValidationResult?
function lib.validate_absolute_paths(ctx, args, fields) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param path? string
---@param field? string
---@return WaliValidationResult?
function lib.validate_optional_absolute_path(ctx, path, field) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param path string
---@return WaliValidationResult?
function lib.validate_safe_remove_path(ctx, path) end

---@param value? integer
---@return WaliValidationResult?
function lib.validate_max_depth(value) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param src string
---@param dest string
---@return WaliValidationResult?
function lib.validate_tree_roots(ctx, src, dest) end

---@param metadata? WaliMetadata
---@return boolean
function lib.is_file(metadata) end

---@param metadata? WaliMetadata
---@return boolean
function lib.is_dir(metadata) end

---@param metadata? WaliMetadata
---@return boolean
function lib.is_symlink(metadata) end

---@class WaliTreeDestinationPolicy
---@field expect 'dir'|'file'|'symlink'
---@field target? string
---@field replace? boolean
---@field label? string

---@param ctx WaliApplyCtx
---@param path string
---@param policy WaliTreeDestinationPolicy
function lib.assert_tree_destination(ctx, path, policy) end

---@param ctx WaliValidateCtx|WaliApplyCtx
---@param dest_root string
---@param relative_path? string
---@return string
function lib.tree_destination(ctx, dest_root, relative_path) end

---@param ctx WaliApplyCtx
---@param result WaliApplyResultBuilder
---@param path string
---@param opts? WaliDirOpts
function lib.ensure_dir(ctx, result, path, opts) end

---@param ctx WaliApplyCtx
---@param result WaliApplyResultBuilder
---@param link_path string
---@param target_path string
---@param replace? boolean
function lib.ensure_symlink(ctx, result, link_path, target_path, replace) end

return lib
