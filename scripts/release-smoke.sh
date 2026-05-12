#!/bin/sh

set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 PATH_TO_WALI_BINARY" >&2
    exit 2
fi

wali=$1
if [ ! -x "$wali" ]; then
    echo "wali binary is not executable: $wali" >&2
    exit 1
fi

root="$(mktemp -d "${TMPDIR:-/tmp}/wali-smoke.XXXXXX")"
trap 'rm -rf "$root"' EXIT HUP INT TERM

fail() {
    echo "release smoke test failed: $*" >&2
    echo "smoke root: $root" >&2

    if [ -d "$root" ]; then
        echo "smoke root contents:" >&2
        find "$root" -maxdepth 5 -print >&2 || true
    fi

    exit 1
}

run() {
    label=$1
    shift
    echo "smoke: $label" >&2
    "$@" || fail "$label"
}

assert_file() {
    [ -f "$1" ] || fail "expected regular file: $1"
}

assert_symlink() {
    [ -L "$1" ] || fail "expected symlink: $1"
}

assert_absent() {
    if [ -e "$1" ] || [ -L "$1" ]; then
        fail "expected path to be absent: $1"
    fi
}

lua_quote() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

manifest="$root/manifest.lua"
created_state_file="$root/apply-created-state.json"
idempotent_state_file="$root/apply-idempotent-state.json"
smoke_root="$root/host"
controller_tree="$root/controller-tree"
set_root="root=$smoke_root"

mkdir -p "$controller_tree/sub"
printf '%s\n' 'controller tree item' >"$controller_tree/sub/item.txt"

cat >"$manifest" <<EOF_MANIFEST
local m = require("manifest")
local function p(path)
    return "{{ root }}/" .. path
end

return {
    name = "release smoke test",
    vars = {
        root = "/wali-smoke-root-not-overridden",
    },
    hosts = {
        m.host.localhost("localhost"),
    },
    tasks = {
        m.task("create workspace")("wali.builtin.mkdir", {
            path = "{{ root }}",
            parents = true,
            mode = "0755",
        }),
        m.task("write source file")("wali.builtin.write", {
            dest = p("source.txt"),
            content = "hello from wali\\n",
            mode = "0644",
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("touch marker")("wali.builtin.touch", {
            path = p("marker"),
            mode = "0644",
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("check permissions")("wali.builtin.permissions", {
            path = p("source.txt"),
            expect = "file",
            mode = "0644",
        }, {
            depends_on = { "write source file" },
        }),
        m.task("copy source file")("wali.builtin.copy", {
            src = p("source.txt"),
            dest = p("copy.txt"),
            replace = true,
            preserve_mode = true,
        }, {
            depends_on = { "write source file" },
        }),
        m.task("link source file")("wali.builtin.link", {
            dest = p("source.link"),
            src = p("source.txt"),
            replace = true,
        }, {
            depends_on = { "write source file" },
        }),
        m.task("create tree root")("wali.builtin.mkdir", {
            path = p("tree"),
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("create tree directory")("wali.builtin.mkdir", {
            path = p("tree/sub"),
        }, {
            depends_on = { "create tree root" },
        }),
        m.task("write tree file")("wali.builtin.write", {
            dest = p("tree/sub/item.txt"),
            content = "tree item\n",
        }, {
            depends_on = { "create tree directory" },
        }),
        m.task("copy tree")("wali.builtin.copy", {
            src = p("tree"),
            dest = p("tree-copy"),
            recursive = true,
            replace = true,
            preserve_mode = true,
            symlinks = "preserve",
        }, {
            depends_on = { "write tree file" },
        }),
        m.task("link tree")("wali.builtin.link", {
            src = p("tree"),
            dest = p("tree-link"),
            recursive = true,
            replace = true,
        }, {
            depends_on = { "write tree file" },
        }),
        m.task("push controller tree")("wali.builtin.push", {
            src = "controller-tree",
            dest = p("pushed-tree"),
            recursive = true,
            replace = true,
            preserve_mode = true,
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("pull pushed tree")("wali.builtin.pull", {
            src = p("pushed-tree"),
            dest = "pulled-tree",
            recursive = true,
            replace = true,
            preserve_mode = true,
        }, {
            depends_on = { "push controller tree" },
        }),
        m.task("run command")("wali.builtin.command", {
            program = "tee",
            args = { p("command.txt") },
            stdin = "command\\n",
            creates = p("command.txt"),
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("remove stale file")("wali.builtin.remove", {
            path = p("stale.txt"),
        }, {
            depends_on = { "create workspace" },
        }),
    },
}
EOF_MANIFEST

run "wali --version" "$wali" --version
run "wali plan" "$wali" plan --set "$set_root" "$manifest"
run "wali check" "$wali" check --set "$set_root" "$manifest"
run "wali apply creates resources" "$wali" apply --set "$set_root" --state-file "$created_state_file" "$manifest"
run "wali apply is idempotent" "$wali" apply --set "$set_root" --state-file "$idempotent_state_file" "$manifest"

assert_file "$smoke_root/source.txt"
assert_file "$smoke_root/copy.txt"
assert_symlink "$smoke_root/source.link"
assert_file "$smoke_root/tree-copy/sub/item.txt"
assert_symlink "$smoke_root/tree-link/sub/item.txt"
assert_file "$smoke_root/pushed-tree/sub/item.txt"
assert_file "$root/pulled-tree/sub/item.txt"
assert_file "$smoke_root/command.txt"
assert_absent "$smoke_root/stale.txt"

run "wali cleanup" "$wali" cleanup --set "$set_root" --state-file "$created_state_file" "$manifest"
assert_absent "$smoke_root"
