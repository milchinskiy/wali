---@meta
---@module 'manifest'

---@class WaliManifestModule
local manifest = {}

---@class WaliManifestHostApi
manifest.host = {}

---@class WaliManifestLocalHostOpts
---@field tags? string[]
---@field vars? table<string, WaliJsonValue>
---@field run_as? WaliManifestRunAs[]
---@field command_timeout? string Human duration such as '30s' or '5m'.

---@class WaliManifestSshHostOpts: WaliManifestLocalHostOpts
---@field user string
---@field host string
---@field port? integer
---@field host_key_policy? 'ignore'|WaliManifestHostKeyPolicy
---@field auth? 'agent'|'password'|WaliManifestSshAuth
---@field connect_timeout? string
---@field keepalive_interval? string

---@class WaliManifestRunAs
---@field id string
---@field user string
---@field via? WaliRunAsVia
---@field env_policy? 'preserve'|'clear'|string[]
---@field extra_flags? string[]
---@field l10n_prompts? string[]
---@field pty? WaliPtyMode

---@class WaliManifestHostKeyPolicy
---@field allow_add? WaliManifestKnownHostsOpts
---@field strict? WaliManifestKnownHostsOpts

---@class WaliManifestKnownHostsOpts
---@field path? string

---@class WaliManifestSshAuth
---@field key_file? WaliManifestSshKeyFileAuth

---@class WaliManifestSshKeyFileAuth
---@field private_key string
---@field public_key? string

---@class WaliManifestSshTransport
---@field ssh table

---@class WaliManifestHost
---@field id string
---@field transport 'local'|WaliManifestSshTransport
---@field tags? string[]
---@field vars? table<string, WaliJsonValue>
---@field run_as? WaliManifestRunAs[]
---@field command_timeout? string

---@param id string
---@param opts? WaliManifestLocalHostOpts
---@return WaliManifestHost
function manifest.host.localhost(id, opts) end

---@param id string
---@param opts WaliManifestSshHostOpts
---@return WaliManifestHost
function manifest.host.ssh(id, opts) end

---@class WaliManifestTaskOpts
---@field tags? string[]
---@field depends_on? string[]
---@field on_change? string[]
---@field when? WaliJsonValue
---@field host? string|string[]
---@field run_as? string
---@field vars? table<string, WaliJsonValue>

---@class WaliManifestTask
---@field id string
---@field module string
---@field args any
---@field tags? string[]
---@field depends_on? string[]
---@field on_change? string[]
---@field when? WaliJsonValue
---@field host? string|string[]
---@field run_as? string
---@field vars? table<string, WaliJsonValue>

---@alias WaliManifestTaskFactory fun(module: string, args?: any, opts?: WaliManifestTaskOpts): WaliManifestTask

---@param id string
---@return WaliManifestTaskFactory
function manifest.task(id) end

return manifest
