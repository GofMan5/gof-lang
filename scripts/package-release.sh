#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-dev}"
DIST_DIR="${DIST_DIR:-dist}"
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

detect_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    echo "cargo"
    return
  fi

  if command -v cargo.exe >/dev/null 2>&1; then
    echo "cargo.exe"
    return
  fi

  echo "cargo is required to build a gof release package." >&2
  exit 1
}

detect_binary() {
  if [[ -f target/release/gof ]]; then
    echo "target/release/gof"
    return
  fi

  if [[ -f target/release/gof.exe ]]; then
    echo "target/release/gof.exe"
    return
  fi

  echo "Unable to find the built gof binary in target/release." >&2
  exit 1
}

platform="$(detect_platform)"
arch="$(detect_arch)"
asset="gof-${platform}-${arch}.tar.gz"
cargo_cmd="$(detect_cargo)"

mkdir -p "$DIST_DIR"
"$cargo_cmd" build --release -p gof-cli --bin gof

cp "$(detect_binary)" "$TMP_DIR/gof"
cp README.md "$TMP_DIR/README.md"
cp LICENSE "$TMP_DIR/LICENSE"

tar -C "$TMP_DIR" -czf "$DIST_DIR/$asset" gof README.md LICENSE
echo "Created $DIST_DIR/$asset for $VERSION"
