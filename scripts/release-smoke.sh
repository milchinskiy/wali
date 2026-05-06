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
smoke_root_lua="$(lua_quote "$smoke_root")"

cat > "$manifest" <<EOF_MANIFEST
local m = require("manifest")
local root = "$smoke_root_lua"
local function p(path)
    return root .. "/" .. path
end

return {
    name = "release smoke test",
    hosts = {
        m.host.localhost("localhost"),
    },
    tasks = {
        m.task("create workspace")("wali.builtin.dir", {
            path = root,
            state = "present",
            parents = true,
            mode = "0755",
        }),
        m.task("write source file")("wali.builtin.file", {
            path = p("source.txt"),
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
        m.task("copy source file")("wali.builtin.copy_file", {
            src = p("source.txt"),
            dest = p("copy.txt"),
            replace = true,
            preserve_mode = true,
        }, {
            depends_on = { "write source file" },
        }),
        m.task("link source file")("wali.builtin.link", {
            path = p("source.link"),
            target = p("source.txt"),
            replace = true,
        }, {
            depends_on = { "write source file" },
        }),
        m.task("create tree root")("wali.builtin.dir", {
            path = p("tree"),
            state = "present",
        }, {
            depends_on = { "create workspace" },
        }),
        m.task("create tree directory")("wali.builtin.dir", {
            path = p("tree/sub"),
            state = "present",
        }, {
            depends_on = { "create tree root" },
        }),
        m.task("write tree file")("wali.builtin.file", {
            path = p("tree/sub/item.txt"),
            content = "tree item\n",
        }, {
            depends_on = { "create tree directory" },
        }),
        m.task("copy tree")("wali.builtin.copy_tree", {
            src = p("tree"),
            dest = p("tree-copy"),
            replace = true,
            preserve_mode = true,
            symlinks = "preserve",
        }, {
            depends_on = { "write tree file" },
        }),
        m.task("link tree")("wali.builtin.link_tree", {
            src = p("tree"),
            dest = p("tree-link"),
            replace = true,
        }, {
            depends_on = { "write tree file" },
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
run "wali plan" "$wali" plan "$manifest"
run "wali check" "$wali" check "$manifest"
run "wali apply creates resources" "$wali" apply --state-file "$created_state_file" "$manifest"
run "wali apply is idempotent" "$wali" apply --state-file "$idempotent_state_file" "$manifest"

assert_file "$smoke_root/source.txt"
assert_file "$smoke_root/copy.txt"
assert_symlink "$smoke_root/source.link"
assert_file "$smoke_root/tree-copy/sub/item.txt"
assert_symlink "$smoke_root/tree-link/sub/item.txt"
assert_file "$smoke_root/command.txt"
assert_absent "$smoke_root/stale.txt"

run "wali cleanup" "$wali" cleanup --state-file "$created_state_file" "$manifest"
assert_absent "$smoke_root"
