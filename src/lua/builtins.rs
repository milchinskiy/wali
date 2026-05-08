pub(crate) struct BuiltinModule {
    pub name: &'static str,
    pub content: &'static str,
    pub task_module: bool,
}

pub(crate) const MODULES: &[BuiltinModule] = &[
    BuiltinModule {
        name: "wali.api",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/api.lua")),
        task_module: false,
    },
    BuiltinModule {
        name: "wali.builtin.lib",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/lib.lua")),
        task_module: false,
    },
    BuiltinModule {
        name: "wali.builtin.dir",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/dir.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.file",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/file.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.copy_file",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/copy_file.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.link",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/link.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.push_file",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/push_file.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.pull_file",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/pull_file.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.remove",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/remove.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.touch",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/touch.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.link_tree",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/link_tree.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.copy_tree",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/copy_tree.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.permissions",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/permissions.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.template",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/template.lua")),
        task_module: true,
    },
    BuiltinModule {
        name: "wali.builtin.command",
        content: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/builtin/command.lua")),
        task_module: true,
    },
];

pub(crate) fn is_task_module(name: &str) -> bool {
    MODULES.iter().any(|module| module.task_module && module.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_module_names_are_unique() {
        let mut names = HashSet::new();
        for module in MODULES {
            assert!(names.insert(module.name), "duplicate builtin module name: {}", module.name);
        }
    }

    #[test]
    fn only_wali_builtin_modules_are_task_modules() {
        for module in MODULES.iter().filter(|module| module.task_module) {
            assert!(
                module.name.starts_with("wali.builtin."),
                "non-task builtin module marked as task module: {}",
                module.name
            );
        }
    }

    #[test]
    fn task_builtin_modules_are_documented() {
        let builtin_docs = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/builtin-modules.md"));
        let module_contract = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/module_contract.lua"));
        let readme = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"));
        let builtin_types = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/types/wali/builtin-modules.d.lua"));

        for module in MODULES.iter().filter(|module| module.task_module) {
            let section = format!("## `{}`", module.name);
            assert!(builtin_docs.contains(&section), "missing builtin docs section for {}", module.name);
            assert!(module_contract.contains(module.name), "missing module_contract entry for {}", module.name);
            assert!(readme.contains(module.name), "missing README entry for {}", module.name);
            assert!(builtin_types.contains(module.name), "missing LuaLS type entry for {}", module.name);
        }
    }

    #[test]
    fn lua_lsp_contract_mentions_core_runtime_surface() {
        let core_types = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/types/wali.d.lua"));
        let manifest_types = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/types/manifest.d.lua"));
        let api_types = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/types/wali/api.d.lua"));
        let lib_types = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/types/wali/builtin/lib.d.lua"));

        for item in [
            "WaliValidateCtx",
            "WaliApplyCtx",
            "WaliHostFsReadApi",
            "WaliHostFsApplyApi",
            "WaliCommandApi",
            "WaliControllerCtx",
            "WaliApplyTransferApi",
            "WaliModule",
            "WaliSchema",
        ] {
            assert!(core_types.contains(item), "missing core LuaLS type: {item}");
        }

        for item in ["host.localhost", "host.ssh", "manifest.task"] {
            assert!(manifest_types.contains(item), "missing manifest LuaLS type: {item}");
        }

        for item in ["api.result.apply", "api.result.validation", "WaliApplyResultBuilder"] {
            assert!(api_types.contains(item), "missing wali.api LuaLS type: {item}");
        }

        for item in [
            "lib.schema.mode",
            "lib.validation_error",
            "lib.mode_bits",
            "lib.validate_absolute_path",
            "lib.apply_mode_owner",
        ] {
            assert!(lib_types.contains(item), "missing wali.builtin.lib LuaLS type: {item}");
        }
    }
}
