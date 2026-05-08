---@meta
---@module 'manifest'

---@class WaliManifestModule
---@field host WaliManifestHostHelperApi
---@field task fun(id: string): WaliManifestTaskFactory
local manifest = {}

---@class WaliManifestHostHelperApi
manifest.host = {}

---@class WaliManifestDefinition
---@field name? string Optional display name. Defaults to the manifest file path when omitted or empty.
---@field base_path? string Controller-side base directory for controller file APIs and transfer/template sources.
---@field vars? table<string, WaliJsonValue> Manifest variables merged before host and task variables.
---@field hosts? WaliManifestHost[] Target hosts. A useful manifest normally has at least one host.
---@field modules? WaliManifestModuleSource[] Custom module sources.
---@field tasks WaliManifestTask[] Tasks to plan and run.

---@alias WaliManifestHost WaliManifestLocalHost|WaliManifestSshHost

---@class WaliManifestHostBase
---@field id string Unique host id.
---@field tags? string[] Host tags used by task host selectors and CLI selectors.
---@field vars? table<string, WaliJsonValue> Host variables merged after manifest variables.
---@field run_as? WaliManifestRunAs[] Host-local privilege-switching profiles.
---@field command_timeout? string Human duration such as '30s' or '5m'.

---@class WaliManifestLocalHost: WaliManifestHostBase
---@field transport 'local'

---@class WaliManifestSshHost: WaliManifestHostBase
---@field transport WaliManifestSshTransport

---@class WaliManifestLocalHostOpts
---@field tags? string[]
---@field vars? table<string, WaliJsonValue>
---@field run_as? WaliManifestRunAs[]
---@field command_timeout? string Human duration such as '30s' or '5m'.

---@class WaliManifestSshHostOpts: WaliManifestLocalHostOpts
---@field user string SSH user.
---@field host string SSH hostname or address.
---@field port? integer SSH port. Defaults to 22.
---@field host_key_policy? 'ignore'|WaliManifestHostKeyPolicy Defaults to strict checking.
---@field auth? 'agent'|'password'|WaliManifestSshAuth Defaults to agent authentication.
---@field connect_timeout? string Human duration such as '10s'.
---@field keepalive_interval? string Human duration such as '30s'.

---@class WaliManifestSshTransport
---@field ssh WaliManifestSshConnection

---@class WaliManifestSshConnection
---@field user string
---@field host string
---@field port? integer
---@field host_key_policy? 'ignore'|WaliManifestHostKeyPolicy
---@field auth? 'agent'|'password'|WaliManifestSshAuth
---@field connect_timeout? string
---@field keepalive_interval? string

---@class WaliManifestRunAs
---@field id string Profile id used by task `run_as`.
---@field user string Target user name.
---@field via? WaliRunAsVia Defaults to 'sudo'.
---@field env_policy? WaliManifestRunAsEnvPolicy Defaults to 'clear'.
---@field extra_flags? string[] Extra flags passed to the privilege-switching command.
---@field l10n_prompts? string[] Localized password prompt fragments.
---@field pty? WaliPtyMode Defaults to 'auto'.

---@alias WaliManifestRunAsEnvPolicy WaliRunAsEnvPolicy

---@alias WaliManifestHostKeyPolicy WaliManifestHostKeyAllowAdd|WaliManifestHostKeyStrict

---@class WaliManifestHostKeyAllowAdd
---@field allow_add WaliManifestKnownHostsOpts

---@class WaliManifestHostKeyStrict
---@field strict WaliManifestKnownHostsOpts

---@class WaliManifestKnownHostsOpts
---@field path? string Known-hosts file path. Defaults to ~/.ssh/known_hosts.

---@class WaliManifestSshAuth
---@field key_file WaliManifestSshKeyFileAuth

---@class WaliManifestSshKeyFileAuth
---@field private_key string Private key file path.
---@field public_key? string Public key file path.

---@param id string
---@param opts? WaliManifestLocalHostOpts
---@return WaliManifestLocalHost
function manifest.host.localhost(id, opts) end

---@param id string
---@param opts WaliManifestSshHostOpts
---@return WaliManifestSshHost
function manifest.host.ssh(id, opts) end

---@alias WaliManifestModuleSource WaliManifestLocalModuleSource|WaliManifestGitModuleSource

---@class WaliManifestLocalModuleSource
---@field namespace? string Optional namespace mounted in front of exposed task module names.
---@field path string Local module include directory, resolved relative to the manifest file when relative.

---@class WaliManifestGitModuleSource
---@field namespace? string Optional namespace mounted in front of exposed task module names.
---@field git WaliManifestGitSource

---@class WaliManifestGitSource
---@field url string Git URL/path passed to the system git executable.
---@field ref string Git ref to fetch and check out.
---@field path? string Repository-relative module include directory.
---@field depth? integer Positive shallow-fetch depth.
---@field submodules? boolean Whether to initialize/update submodules. Defaults to false.
---@field timeout? string Human duration. Defaults to '5m'.

---@alias WaliManifestHostSelector
---| WaliManifestHostSelectorId
---| WaliManifestHostSelectorTag
---| WaliManifestHostSelectorNot
---| WaliManifestHostSelectorAll
---| WaliManifestHostSelectorAny

---@class WaliManifestHostSelectorId
---@field id string

---@class WaliManifestHostSelectorTag
---@field tag string

---@class WaliManifestHostSelectorNot
---@field not WaliManifestHostSelector

---@class WaliManifestHostSelectorAll
---@field all WaliManifestHostSelector[]

---@class WaliManifestHostSelectorAny
---@field any WaliManifestHostSelector[]

---@alias WaliManifestWhen
---| WaliManifestWhenAll
---| WaliManifestWhenAny
---| WaliManifestWhenNot
---| WaliManifestWhenOs
---| WaliManifestWhenArch
---| WaliManifestWhenHostname
---| WaliManifestWhenUser
---| WaliManifestWhenGroup
---| WaliManifestWhenEnv
---| WaliManifestWhenEnvSet
---| WaliManifestWhenPathExist
---| WaliManifestWhenPathFile
---| WaliManifestWhenPathDir
---| WaliManifestWhenPathSymlink
---| WaliManifestWhenCommandExist

---@class WaliManifestWhenAll
---@field all WaliManifestWhen[]

---@class WaliManifestWhenAny
---@field any WaliManifestWhen[]

---@class WaliManifestWhenNot
---@field not WaliManifestWhen

---@class WaliManifestWhenOs
---@field os string

---@class WaliManifestWhenArch
---@field arch string

---@class WaliManifestWhenHostname
---@field hostname string

---@class WaliManifestWhenUser
---@field user string|integer

---@class WaliManifestWhenGroup
---@field group string|integer

---@alias WaliManifestEnvEquals string[] Environment variable predicate tuple: exactly { name, expected_value }.

---@class WaliManifestWhenEnv
---@field env WaliManifestEnvEquals

---@class WaliManifestWhenEnvSet
---@field env_set string

---@class WaliManifestWhenPathExist
---@field path_exist string

---@class WaliManifestWhenPathFile
---@field path_file string

---@class WaliManifestWhenPathDir
---@field path_dir string

---@class WaliManifestWhenPathSymlink
---@field path_symlink string

---@class WaliManifestWhenCommandExist
---@field command_exist string

---@class WaliManifestTaskOpts
---@field tags? string[]
---@field depends_on? string[]
---@field on_change? string[]
---@field when? WaliManifestWhen
---@field host? WaliManifestHostSelector
---@field run_as? string
---@field vars? table<string, WaliJsonValue>

---@class WaliManifestTask: WaliManifestTaskOpts
---@field id string Unique task id.
---@field module WaliBuiltinModuleName|string Dotted module name, for example 'wali.builtin.file'.
---@field args WaliBuiltinModuleArgs|WaliJsonValue|table Module argument value. The helper defaults this to `{}` when omitted.

---@alias WaliManifestTaskFactory
---| fun(module: 'wali.builtin.dir', args?: WaliBuiltinDirArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.file', args?: WaliBuiltinFileArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.copy_file', args?: WaliBuiltinCopyFileArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.push_file', args?: WaliBuiltinPushFileArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.pull_file', args?: WaliBuiltinPullFileArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.link', args?: WaliBuiltinLinkArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.remove', args?: WaliBuiltinRemoveArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.touch', args?: WaliBuiltinTouchArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.link_tree', args?: WaliBuiltinLinkTreeArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.copy_tree', args?: WaliBuiltinCopyTreeArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.permissions', args?: WaliBuiltinPermissionsArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.command', args?: WaliBuiltinCommandArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: 'wali.builtin.template', args?: WaliBuiltinTemplateArgs, opts?: WaliManifestTaskOpts): WaliManifestTask
---| fun(module: string, args?: WaliJsonValue|table, opts?: WaliManifestTaskOpts): WaliManifestTask

---@param id string
---@return WaliManifestTaskFactory
function manifest.task(id) end

return manifest
