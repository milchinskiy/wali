#!/bin/sh

set -eu

repo="${WALI_REPO:-milchinskiy/wali}"
version="${WALI_VERSION:-latest}"
install_dir="${WALI_INSTALL_DIR:-/usr/local/bin}"
package="${WALI_PACKAGE:-}"
checksum_file="${WALI_CHECKSUMS:-}"
install_types="${WALI_INSTALL_TYPES:-1}"
data_dir=""
types_dir=""

case "$(uname -s)" in
Linux)
    os="linux"
    ;;
Darwin)
    os="macos"
    ;;
*)
    echo "unsupported operating system: $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
x86_64 | amd64)
    arch="x86_64"
    ;;
arm64 | aarch64)
    arch="aarch64"
    ;;
*)
    echo "unsupported CPU architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$package" != "" ]; then
    if [ ! -f "$package" ]; then
        echo "local package does not exist: $package" >&2
        exit 1
    fi
    asset="$(basename "$package")"
else
    if [ "$os" = "macos" ]; then
        asset="wali-macos-universal.tar.gz"
    else
        asset="wali-linux-$arch.tar.gz"
    fi

    if [ "$version" = "latest" ]; then
        base_url="https://github.com/$repo/releases/latest/download"
    else
        case "$version" in
        v*) tag="$version" ;;
        *) tag="v$version" ;;
        esac
        base_url="https://github.com/$repo/releases/download/$tag"
    fi
fi

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

download() {
    url="$1"
    dst="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dst"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dst" "$url"
    else
        echo "missing required command: curl or wget" >&2
        exit 1
    fi
}

sha256_of() {
    path="$1"

    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$path" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$path" | awk '{print $1}'
    else
        echo "missing required command: sha256sum or shasum" >&2
        exit 1
    fi
}

need awk
need tar
need install

case "$install_types" in
1 | yes | true)
    install_types=1
    ;;
0 | no | false)
    install_types=0
    ;;
*)
    echo "WALI_INSTALL_TYPES must be 1 or 0" >&2
    exit 1
    ;;
esac

if [ "$install_types" -eq 1 ]; then
    if [ "${WALI_DATA_DIR:-}" != "" ]; then
        data_dir="$WALI_DATA_DIR"
    elif [ "${XDG_DATA_HOME:-}" != "" ]; then
        data_dir="$XDG_DATA_HOME/wali"
    elif [ "${HOME:-}" != "" ]; then
        data_dir="$HOME/.local/share/wali"
    elif [ "${WALI_TYPES_DIR:-}" != "" ]; then
        data_dir="$(dirname "$WALI_TYPES_DIR")"
    else
        echo "HOME is not set; set WALI_DATA_DIR or WALI_TYPES_DIR" >&2
        exit 1
    fi

    types_dir="${WALI_TYPES_DIR:-$data_dir/types}"
fi

dir_needs_sudo() {
    dir="$1"

    if [ -d "$dir" ]; then
        if [ -w "$dir" ]; then
            return 1
        fi
        return 0
    fi

    parent="$dir"
    while [ ! -d "$parent" ]; do
        next="$(dirname "$parent")"
        if [ "$next" = "$parent" ]; then
            return 0
        fi
        parent="$next"
    done

    if [ -w "$parent" ]; then
        return 1
    fi
    return 0
}

require_sudo_for_dir() {
    dir="$1"

    if dir_needs_sudo "$dir"; then
        if ! command -v sudo >/dev/null 2>&1; then
            echo "$dir is not writable and sudo is not available" >&2
            exit 1
        fi
        return 0
    fi

    return 1
}

install_tree() {
    src="$1"
    dst="$2"

    if require_sudo_for_dir "$dst"; then
        sudo mkdir -p "$dst"
        (cd "$src" && tar -cf - .) | sudo tar -xf - -C "$dst"
    else
        mkdir -p "$dst"
        (cd "$src" && tar -cf - .) | (cd "$dst" && tar -xf -)
    fi
}

install_data_file() {
    src="$1"
    dst="$2"
    dst_dir="$(dirname "$dst")"

    if require_sudo_for_dir "$dst_dir"; then
        sudo mkdir -p "$dst_dir"
        sudo install -m 0644 "$src" "$dst"
    else
        mkdir -p "$dst_dir"
        install -m 0644 "$src" "$dst"
    fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

archive="$tmp_dir/$asset"
checksums="$tmp_dir/sha256sums.txt"
extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"

if [ "$package" != "" ]; then
    cp "$package" "$archive"

    if [ "$checksum_file" != "" ]; then
        if [ ! -f "$checksum_file" ]; then
            echo "checksum file does not exist: $checksum_file" >&2
            exit 1
        fi
        cp "$checksum_file" "$checksums"
    elif [ -f "$(dirname "$package")/sha256sums.txt" ]; then
        cp "$(dirname "$package")/sha256sums.txt" "$checksums"
    fi
else
    download "$base_url/$asset" "$archive"
    download "$base_url/sha256sums.txt" "$checksums"
fi

if [ -f "$checksums" ]; then
    expected="$(awk -v file="$asset" '$2 == file { print $1 }' "$checksums")"
    if [ "$expected" = "" ]; then
        echo "checksum file does not contain $asset" >&2
        exit 1
    fi

    actual="$(sha256_of "$archive")"
    if [ "$actual" != "$expected" ]; then
        echo "checksum mismatch for $asset" >&2
        echo "expected: $expected" >&2
        echo "actual:   $actual" >&2
        exit 1
    fi
else
    echo "no checksum file found for local package; skipping checksum verification" >&2
fi

tar -xzf "$archive" -C "$extract_dir"
if [ ! -f "$extract_dir/wali" ]; then
    echo "archive does not contain wali binary" >&2
    exit 1
fi

if require_sudo_for_dir "$install_dir"; then
    sudo mkdir -p "$install_dir"
    sudo install -m 0755 "$extract_dir/wali" "$install_dir/wali"
else
    mkdir -p "$install_dir"
    install -m 0755 "$extract_dir/wali" "$install_dir/wali"
fi

echo "installed wali to $install_dir/wali"

if [ "$install_types" -eq 1 ]; then
    if [ -d "$extract_dir/types" ]; then
        install_tree "$extract_dir/types" "$types_dir"
        echo "installed LuaLS stubs to $types_dir"

        if [ -f "$extract_dir/.luarc.example.json" ]; then
            install_data_file "$extract_dir/.luarc.example.json" "$data_dir/.luarc.example.json"
            echo "installed LuaLS example config to $data_dir/.luarc.example.json"
        fi

        cat <<EOF

LuaLS setup hint:
  add this directory to workspace.library:
    $types_dir
EOF
    else
        echo "archive does not contain LuaLS stubs; skipping type installation" >&2
    fi
else
    echo "skipped LuaLS stub installation"
fi
