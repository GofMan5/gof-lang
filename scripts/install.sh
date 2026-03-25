#!/usr/bin/env bash
set -euo pipefail

REPO="${GOF_REPO:-GofMan5/gof-lang}"
VERSION="${1:-latest}"
INSTALL_ROOT="${GOF_INSTALL_ROOT:-$HOME/.gof}"
BIN_DIR="$INSTALL_ROOT/bin"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

detect_platform() {
  case "$(uname -s)" in
    Linux) echo "linux" ;;
    Darwin) echo "macos" ;;
    *)
      echo "Unsupported operating system: $(uname -s)" >&2
      exit 1
      ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    arm64|aarch64) echo "aarch64" ;;
    *)
      echo "Unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}

download() {
  local url="$1"
  local output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -qO "$output" "$url"
    return
  fi

  echo "Missing downloader: install curl or wget first." >&2
  exit 1
}

ensure_path() {
  local path_line="export PATH=\"$BIN_DIR:\$PATH\""
  local target_rc=""

  for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    if [[ -f "$rc" ]]; then
      target_rc="$rc"
      break
    fi
  done

  if [[ -z "$target_rc" ]]; then
    target_rc="$HOME/.profile"
    touch "$target_rc"
  fi

  if ! grep -Fq "$BIN_DIR" "$target_rc"; then
    printf "\n# gof\n%s\n" "$path_line" >> "$target_rc"
    echo "Updated $target_rc to include $BIN_DIR"
  else
    echo "PATH entry already present in $target_rc"
  fi

  export PATH="$BIN_DIR:$PATH"
}

platform="$(detect_platform)"
arch="$(detect_arch)"
asset="gof-${platform}-${arch}.tar.gz"

if [[ "$VERSION" == "latest" ]]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
fi

mkdir -p "$BIN_DIR"
archive_path="$TMP_DIR/$asset"

echo "Downloading $url"
download "$url" "$archive_path"
tar -xzf "$archive_path" -C "$TMP_DIR"

install -m 0755 "$TMP_DIR/gof" "$BIN_DIR/gof"
ensure_path

echo "Installed gof to $BIN_DIR/gof"
echo "Run the installer again later to update."
