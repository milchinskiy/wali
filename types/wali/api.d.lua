---@meta
---@module 'wali.api'

---@class WaliApiModule
local api = {}

---@class WaliApiResult
api.result = {}

---@return WaliApplyResultBuilder
function api.result.apply() end

---@return WaliValidationResultBuilder
function api.result.validation() end

---@class WaliValidationResultBuilder
local validation_builder = {}

---@param message? string
---@return WaliValidationResultBuilder
function validation_builder:ok(message) end

---@param message? string
---@return WaliValidationResultBuilder
function validation_builder:fail(message) end

---@param message? string
---@return WaliValidationResultBuilder
function validation_builder:message(message) end

---@return WaliValidationResult
function validation_builder:build() end

---@class WaliApplyResultBuilder
local apply_builder = {}

---@param message string
---@return WaliApplyResultBuilder
function apply_builder:message(message) end

---@param value WaliJsonValue
---@return WaliApplyResultBuilder
function apply_builder:data(value) end

---@param kind WaliChangeKind
---@param subject WaliChangeSubject
---@param data? { path?: string, detail?: string }
---@return WaliApplyResultBuilder
function apply_builder:change(kind, subject, data) end

---@param kind WaliChangeKind
---@param path? string
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:fs(kind, path, detail) end

---@param kind WaliChangeKind
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:command(kind, detail) end

---@param path? string
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:created(path, detail) end

---@param path? string
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:updated(path, detail) end

---@param path? string
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:removed(path, detail) end

---@param path? string
---@param detail? string
---@return WaliApplyResultBuilder
function apply_builder:unchanged(path, detail) end

---@param result? WaliExecutionResult
---@return WaliApplyResultBuilder
function apply_builder:merge(result) end

---@return boolean
function apply_builder:empty() end

---@return WaliExecutionResult
function apply_builder:build() end

return api
