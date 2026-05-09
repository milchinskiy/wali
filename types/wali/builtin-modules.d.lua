---@meta

-- Typed argument tables for Wali's builtin task modules.
-- Use these with manual `---@type` annotations when composing reusable task args
-- or when documenting wrappers around builtin modules.

---@class WaliBuiltinDirArgs
---@field path string
---@field state? 'present'|'absent'
---@field parents? boolean
---@field recursive? boolean
---@field mode? string Octal mode string, for example '0755'.
---@field owner? WaliOwner

---@class WaliBuiltinFileArgs
---@field path string
---@field state? 'present'|'absent'
---@field content? string Required when state is 'present'.
---@field create_parents? boolean
---@field replace? boolean
---@field mode? string Octal mode string, for example '0644'.
---@field owner? WaliOwner

---@class WaliBuiltinCopyFileArgs
---@field src string
---@field dest string
---@field create_parents? boolean
---@field replace? boolean
---@field preserve_mode? boolean
---@field mode? string Octal mode string.
---@field owner? WaliOwner

---@class WaliBuiltinPushFileArgs
---@field src string Controller-side source path.
---@field dest string Target-host destination path.
---@field create_parents? boolean
---@field replace? boolean
---@field mode? string Octal mode string.
---@field owner? WaliOwner

---@class WaliBuiltinPushTreeArgs
---@field src string Controller-side source directory.
---@field dest string Target-host destination directory.
---@field replace? boolean
---@field preserve_mode? boolean
---@field symlinks? WaliTreeSymlinkPolicy
---@field skip_special? boolean
---@field max_depth? integer
---@field dir_mode? string Octal mode string for directories.
---@field file_mode? string Octal mode string for files.
---@field dir_owner? WaliOwner
---@field file_owner? WaliOwner

---@class WaliBuiltinPullFileArgs
---@field src string Target-host source path.
---@field dest string Controller-side destination path.
---@field create_parents? boolean
---@field replace? boolean
---@field mode? string Octal mode string for the controller-side destination where supported.

---@class WaliBuiltinPullTreeArgs
---@field src string Target-host source directory.
---@field dest string Controller-side destination directory.
---@field replace? boolean
---@field preserve_mode? boolean
---@field symlinks? WaliTreeSymlinkPolicy
---@field skip_special? boolean
---@field max_depth? integer
---@field dir_mode? string Octal mode string for directories.
---@field file_mode? string Octal mode string for files.

---@class WaliBuiltinLinkArgs
---@field path string Symlink path on the target host.
---@field target? string Symlink target. Required when state is 'present'.
---@field state? 'present'|'absent'
---@field replace? boolean

---@class WaliBuiltinRemoveArgs
---@field path string Target-host path to remove.
---@field recursive? boolean Required for non-empty directories.

---@class WaliBuiltinTouchArgs
---@field path string
---@field create_parents? boolean
---@field mode? string Octal mode string.
---@field owner? WaliOwner

---@class WaliBuiltinLinkTreeArgs
---@field src string Source directory on the target host.
---@field dest string Destination directory on the target host.
---@field replace? boolean Replace existing file/symlink destinations.
---@field allow_special? boolean Allow special entries by skipping them.
---@field max_depth? integer
---@field dir_mode? string Octal mode string for created directories.
---@field dir_owner? WaliOwner

---@class WaliBuiltinCopyTreeArgs
---@field src string Source directory on the target host.
---@field dest string Destination directory on the target host.
---@field replace? boolean
---@field preserve_mode? boolean
---@field preserve_owner? boolean
---@field symlinks? WaliTreeSymlinkPolicy
---@field skip_special? boolean
---@field max_depth? integer
---@field dir_mode? string Octal mode string for directories.
---@field file_mode? string Octal mode string for files.
---@field dir_owner? WaliOwner
---@field file_owner? WaliOwner

---@class WaliBuiltinPermissionsArgs
---@field path string
---@field follow? boolean
---@field expect? WaliPermissionsExpect
---@field mode? string Octal mode string.
---@field owner? WaliOwner

---@class WaliBuiltinCommandArgs
---@field program? string Mutually exclusive with script.
---@field args? string[]
---@field script? string Mutually exclusive with program.
---@field cwd? string
---@field env? table<string, string>
---@field stdin? string
---@field timeout? string Human duration such as '10s' or '2m'.
---@field pty? WaliPtyMode
---@field creates? string Skip command when this absolute path already exists.
---@field removes? string Skip command when this absolute path is already absent.
---@field changed? WaliCommandChangedPolicy

---@class WaliBuiltinTemplateArgs
---@field src? string Controller-side template source. Mutually exclusive with content.
---@field content? string Inline template source. Mutually exclusive with src.
---@field dest string Target-host destination path.
---@field vars? table<string, WaliJsonValue>
---@field create_parents? boolean
---@field replace? boolean
---@field mode? string Octal mode string.
---@field owner? WaliOwner


---@alias WaliBuiltinModuleArgs
---| WaliBuiltinDirArgs
---| WaliBuiltinFileArgs
---| WaliBuiltinCopyFileArgs
---| WaliBuiltinPushFileArgs
---| WaliBuiltinPushTreeArgs
---| WaliBuiltinPullFileArgs
---| WaliBuiltinPullTreeArgs
---| WaliBuiltinLinkArgs
---| WaliBuiltinRemoveArgs
---| WaliBuiltinTouchArgs
---| WaliBuiltinLinkTreeArgs
---| WaliBuiltinCopyTreeArgs
---| WaliBuiltinPermissionsArgs
---| WaliBuiltinCommandArgs
---| WaliBuiltinTemplateArgs

---@alias WaliBuiltinModuleName
---| 'wali.builtin.dir'
---| 'wali.builtin.file'
---| 'wali.builtin.copy_file'
---| 'wali.builtin.push_file'
---| 'wali.builtin.push_tree'
---| 'wali.builtin.pull_file'
---| 'wali.builtin.pull_tree'
---| 'wali.builtin.link'
---| 'wali.builtin.remove'
---| 'wali.builtin.touch'
---| 'wali.builtin.link_tree'
---| 'wali.builtin.copy_tree'
---| 'wali.builtin.permissions'
---| 'wali.builtin.command'
---| 'wali.builtin.template'
