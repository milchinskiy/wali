---@meta

-- Wali LuaLS contract.
-- Add this file (or the whole `types/` directory) to LuaLS `workspace.library`.
-- These definitions describe the public Lua surface available to manifests and
-- custom modules. They are editor stubs only; Wali does not load them at runtime.

---@class WaliJsonNull

---@type WaliJsonNull
null = nil

---@alias WaliJsonScalar WaliJsonNull|string|number|boolean
---@alias WaliJsonValue WaliJsonScalar|WaliJsonObject|WaliJsonArray
---@alias WaliJsonObject table<string, WaliJsonValue>
---@alias WaliJsonArray WaliJsonValue[]

---@alias WaliTaskPhase 'validate'|'apply'
---@alias WaliTransportKind 'local'|'ssh'
---@alias WaliPathKind 'file'|'dir'|'symlink'|'other'
---@alias WaliWalkOrder 'native'|'pre'|'post'
---@alias WaliPtyMode 'never'|'auto'|'require'
---@alias WaliChangeKind 'unchanged'|'created'|'updated'|'removed'
---@alias WaliChangeSubject 'fs_entry'|'command'
---@alias WaliCommandChangedPolicy 'on_run'|'always'|'never'
---@alias WaliTreeSymlinkPolicy 'preserve'|'skip'|'error'
---@alias WaliPermissionsExpect 'any'|'file'|'dir'
---@alias WaliRunAsVia 'sudo'|'doas'|'su'

---@alias WaliOwnerPart string|integer

---@class WaliOwner
---@field user? WaliOwnerPart
---@field group? WaliOwnerPart

---@class WaliMetadataOpts
---@field follow? boolean Follow symlinks. Defaults to true.

---@class WaliWalkOpts
---@field include_root? boolean Include the walk root in returned entries.
---@field max_depth? integer Maximum traversal depth, zero or greater.
---@field order? WaliWalkOrder Traversal order. Defaults to 'pre'.

---@class WaliWriteOpts
---@field create_parents? boolean Create missing parent directories.
---@field mode? integer POSIX mode bits as an integer, normally from `lib.mode_bits()`.
---@field owner? WaliOwner
---@field replace? boolean Replace existing regular file content. Defaults to true.

---@class WaliCopyFileOpts
---@field create_parents? boolean Create missing parent directories.
---@field mode? integer POSIX mode bits as an integer, normally from `lib.mode_bits()`.
---@field owner? WaliOwner
---@field replace? boolean Replace existing regular file content. Defaults to true.
---@field preserve_mode? boolean Preserve source mode when explicit mode is omitted. Defaults to true.

---@class WaliDirOpts
---@field recursive? boolean Create parent directories when needed.
---@field mode? integer POSIX mode bits as an integer, normally from `lib.mode_bits()`.
---@field owner? WaliOwner

---@class WaliRemoveDirOpts
---@field recursive? boolean Remove a non-empty directory tree.

---@class WaliRenameOpts
---@field replace? boolean Replace destination when supported. Defaults to true.

---@class WaliMktempOpts
---@field kind? 'file'|'dir' Temporary entry kind. Defaults to 'file'.
---@field parent_dir? string Target-host directory for the temporary entry.
---@field prefix? string Filename prefix.

---@class WaliPullFileOpts
---@field create_parents? boolean Create missing controller-side parent directories.
---@field mode? integer POSIX mode bits for the controller-side destination where supported.
---@field replace? boolean Replace existing destination. Defaults to true.

---@class WaliMetadata
---@field kind WaliPathKind
---@field size integer
---@field link_target? string
---@field created_at? number Unix timestamp in seconds, when available.
---@field modified_at? number Unix timestamp in seconds, when available.
---@field accessed_at? number Unix timestamp in seconds, when available.
---@field changed_at? number Unix timestamp in seconds, when available.
---@field uid integer
---@field gid integer
---@field mode integer POSIX mode bits.

---@class WaliDirEntry
---@field name string
---@field kind WaliPathKind

---@class WaliWalkEntry
---@field path string
---@field relative_path string
---@field depth integer
---@field kind WaliPathKind
---@field metadata WaliMetadata
---@field link_target? string

---@class WaliTaskCtx
---@field id string
---@field module string
---@field tags string[]
---@field depends_on string[]
---@field on_change string[]

---@class WaliRunAs
---@field id string
---@field user string
---@field via WaliRunAsVia
---@field env_policy 'preserve'|'clear'|table<string, boolean>|string[]
---@field extra_flags string[]
---@field l10n_prompts string[]
---@field pty WaliPtyMode

---@class WaliValidateCtx
---@field phase 'validate'
---@field task WaliTaskCtx
---@field vars table<string, WaliJsonValue>
---@field run_as? WaliRunAs
---@field host WaliValidateHostCtx
---@field controller WaliControllerCtx
---@field codec WaliCodecApi
---@field hash WaliHashApi
---@field json WaliJsonApi
---@field template WaliTemplateApi
---@field transfer WaliValidateTransferApi

---@class WaliApplyCtx
---@field phase 'apply'
---@field task WaliTaskCtx
---@field vars table<string, WaliJsonValue>
---@field run_as? WaliRunAs
---@field host WaliApplyHostCtx
---@field controller WaliControllerCtx
---@field codec WaliCodecApi
---@field hash WaliHashApi
---@field json WaliJsonApi
---@field template WaliTemplateApi
---@field transfer WaliApplyTransferApi
---@field rand WaliRandApi
---@field sleep_ms fun(ms: integer)

---@class WaliValidateHostCtx
---@field id string
---@field transport WaliTransportKind
---@field facts WaliFactsApi
---@field fs WaliHostFsReadApi
---@field path WaliPathApi

---@class WaliApplyHostCtx
---@field id string
---@field transport WaliTransportKind
---@field facts WaliFactsApi
---@field fs WaliHostFsApplyApi
---@field path WaliPathApi
---@field cmd WaliCommandApi

---@class WaliFactsApi
---@field os fun(): string
---@field arch fun(): string
---@field hostname fun(): string
---@field env fun(key: string): string?
---@field uid fun(): integer
---@field gid fun(): integer
---@field gids fun(): integer[]
---@field user fun(): string
---@field group fun(): string
---@field groups fun(): string[]
---@field which fun(command: string): string?

---@class WaliPathApi
---@field join fun(base: string, child: string): string
---@field normalize fun(path: string): string
---@field parent fun(path: string): string?
---@field is_absolute fun(path: string): boolean
---@field basename fun(path: string): string?
---@field strip_prefix fun(base: string, path: string): string?

---@class WaliControllerCtx
---@field path WaliControllerPathApi
---@field fs WaliControllerFsApi

---@class WaliControllerPathApi
---@field resolve fun(path: string): string Resolve relative paths against manifest `base_path`.
---@field is_absolute fun(path: string): boolean
---@field join fun(base: string, child: string): string
---@field normalize fun(path: string): string
---@field parent fun(path: string): string?
---@field basename fun(path: string): string?
---@field strip_prefix fun(base: string, path: string): string?

---@class WaliControllerFsApi
---@field metadata fun(path: string, opts?: WaliMetadataOpts): WaliMetadata?
---@field stat fun(path: string): WaliMetadata?
---@field lstat fun(path: string): WaliMetadata?
---@field exists fun(path: string): boolean
---@field read fun(path: string): string Returns file bytes as a Lua string.
---@field read_text fun(path: string): string Returns UTF-8 text.
---@field list_dir fun(path: string): WaliDirEntry[]
---@field walk fun(path: string, opts?: WaliWalkOpts): WaliWalkEntry[]
---@field read_link fun(path: string): string

---@class WaliHostFsReadApi
---@field metadata fun(path: string, opts?: WaliMetadataOpts): WaliMetadata?
---@field stat fun(path: string): WaliMetadata?
---@field lstat fun(path: string): WaliMetadata?
---@field exists fun(path: string): boolean
---@field read fun(path: string): string Returns file bytes as a Lua string.
---@field read_text fun(path: string): string Returns UTF-8 text.
---@field list_dir fun(path: string): WaliDirEntry[]
---@field walk fun(path: string, opts?: WaliWalkOpts): WaliWalkEntry[]
---@field read_link fun(path: string): string

---@class WaliHostFsApplyApi: WaliHostFsReadApi
---@field write fun(path: string, content: string, opts?: WaliWriteOpts): WaliExecutionResult
---@field copy_file fun(from: string, to: string, opts?: WaliCopyFileOpts): WaliExecutionResult
---@field create_dir fun(path: string, opts?: WaliDirOpts): WaliExecutionResult
---@field remove_file fun(path: string): WaliExecutionResult
---@field remove_dir fun(path: string, opts?: WaliRemoveDirOpts): WaliExecutionResult
---@field mktemp fun(opts?: WaliMktempOpts): string
---@field chmod fun(path: string, mode: integer): WaliExecutionResult
---@field chown fun(path: string, owner: WaliOwner): WaliExecutionResult
---@field rename fun(from: string, to: string, opts?: WaliRenameOpts): WaliExecutionResult
---@field symlink fun(target: string, link: string): WaliExecutionResult

---@class WaliExecCommandRequest
---@field program string
---@field args? string[]
---@field cwd? string
---@field env? table<string, string>
---@field stdin? string|integer[]
---@field timeout? string Human duration such as '10s' or '2m'.
---@field pty? WaliPtyMode

---@class WaliShellCommandRequest
---@field script string
---@field cwd? string
---@field env? table<string, string>
---@field stdin? string|integer[]
---@field timeout? string Human duration such as '10s' or '2m'.
---@field pty? WaliPtyMode

---@class WaliCommandApi
---@field exec fun(req: WaliExecCommandRequest): WaliCommandOutput
---@field shell fun(req: string|WaliShellCommandRequest): WaliCommandOutput

---@class WaliCommandOutput
---@field ok boolean
---@field status WaliCommandStatus
---@field stdout? string Split stdout bytes as a Lua string.
---@field stderr? string Split stderr bytes as a Lua string.
---@field output? string Combined PTY output bytes as a Lua string.

---@class WaliCommandStatus
---@field kind 'exited'|'signaled'|'unknown'
---@field code? integer
---@field signal? string

---@class WaliCodecApi
---@field base64_encode fun(bytes: string): string
---@field base64_decode fun(text: string): string

---@class WaliHashApi
---@field sha256 fun(bytes: string): string

---@class WaliJsonApi
---@field decode fun(text: string): WaliJsonValue
---@field encode fun(value: WaliJsonValue): string
---@field encode_pretty fun(value: WaliJsonValue): string

---@class WaliTemplateApi
---@field render fun(source: string, vars?: table<string, WaliJsonValue>): string

---@class WaliValidateTransferApi

---@class WaliApplyTransferApi
---@field push_file fun(src: string, dest: string, opts?: WaliWriteOpts): WaliExecutionResult
---@field pull_file fun(src: string, dest: string, opts?: WaliPullFileOpts): WaliExecutionResult

---@class WaliRandApi
---@field irange fun(min: integer, max: integer): integer
---@field frange fun(min: number, max: number): number
---@field ratio fun(numerator: integer, denominator: integer): boolean

---@class WaliValidationResult
---@field ok boolean
---@field message? string

---@class WaliExecutionResult
---@field changes WaliExecutionChange[]
---@field message? string
---@field data? WaliJsonValue

---@class WaliExecutionChange
---@field kind WaliChangeKind
---@field subject WaliChangeSubject
---@field path? string Required for changed fs_entry records.
---@field detail? string

---@class WaliModule
---@field name string Human-readable module name.
---@field description string Human-readable module description.
---@field requires? WaliRequirement
---@field schema? WaliSchema
---@field validate? fun(ctx: WaliValidateCtx, args: any): WaliValidationResult?
---@field apply fun(ctx: WaliApplyCtx, args: any): WaliExecutionResult?

---@alias WaliRequirement WaliSimpleRequirement|WaliNotRequirement|WaliAllRequirement|WaliAnyRequirement

---@class WaliSimpleRequirement
---@field command? string
---@field path? string
---@field env? string
---@field os? string
---@field arch? string
---@field hostname? string
---@field user? string
---@field group? string

---@class WaliNotRequirement
---@field not WaliRequirement

---@class WaliAllRequirement
---@field all WaliRequirement[]

---@class WaliAnyRequirement
---@field any WaliRequirement[]

---@alias WaliSchemaKind 'any'|'null'|'string'|'number'|'integer'|'boolean'|'list'|'tuple'|'enum'|'object'|'map'

---@class WaliSchema
---@field type WaliSchemaKind
---@field required? boolean
---@field default? any
---@field items? WaliSchema|WaliSchema[]
---@field values? any[]
---@field props? table<string, WaliSchema>
---@field value? WaliSchema
