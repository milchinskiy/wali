---@meta
---@module 'wali'

---@class WaliVersionInfo
---@field major integer
---@field minor integer
---@field patch integer
---@field prerelease? string
---@field build? string
---@field text string

---@class WaliRuntimeModule
local wali = {}

---@type string Current wali runtime version, for example "0.2.0".
wali.version = nil

---@type WaliVersionInfo Parsed current wali runtime version.
wali.version_info = nil

---@param value string
---@return WaliVersionInfo
function wali.parse_version(value) end

---@param left string|WaliVersionInfo
---@param right string|WaliVersionInfo
---@return -1|0|1
function wali.compare_versions(left, right) end

---@param requirement string Whitespace-separated comparators such as ">=0.2.0 <0.3.0".
---@param version? string Version to check. Defaults to the current wali runtime version.
---@return boolean
function wali.compatible(requirement, version) end

---@param requirement string Whitespace-separated comparators such as ">=0.2.0 <0.3.0".
---@param label? string Human label used in the error message.
---@return true
function wali.require_version(requirement, label) end

return wali
