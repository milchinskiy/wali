#!/bin/sh
set -eu

repo="${WALI_REPO:-milchinskiy/wali}"
version="${WALI_VERSION:-latest}"
install_dir="${WALI_INSTALL_DIR:-/usr/local/bin}"
package="${WALI_PACKAGE:-}"
checksum_file="${WALI_CHECKSUMS:-}"

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
  x86_64|amd64)
    arch="x86_64"
    ;;
  arm64|aarch64)
    arch="aarch64"
    ;;
  *)
    echo "unsupported CPU architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ -n "$package" ]; then
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

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

archive="$tmp_dir/$asset"
checksums="$tmp_dir/sha256sums.txt"
extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"

if [ -n "$package" ]; then
  cp "$package" "$archive"

  if [ -n "$checksum_file" ]; then
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
  if [ -z "$expected" ]; then
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

use_sudo=0
if [ -d "$install_dir" ]; then
  if [ ! -w "$install_dir" ]; then
    use_sudo=1
  fi
else
  parent="$(dirname "$install_dir")"
  if [ ! -w "$parent" ]; then
    use_sudo=1
  fi
fi

if [ "$use_sudo" -eq 1 ]; then
  if ! command -v sudo >/dev/null 2>&1; then
    echo "$install_dir is not writable and sudo is not available" >&2
    exit 1
  fi
  sudo mkdir -p "$install_dir"
  sudo install -m 0755 "$extract_dir/wali" "$install_dir/wali"
else
  mkdir -p "$install_dir"
  install -m 0755 "$extract_dir/wali" "$install_dir/wali"
fi

echo "installed wali to $install_dir/wali"
