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
}
