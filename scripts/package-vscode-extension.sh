#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="${DIST_DIR:-tools/vscode-gof}"
SKIP_INSTALL="${SKIP_INSTALL:-0}"
SKIP_TESTS="${SKIP_TESTS:-0}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTENSION_DIR="$ROOT_DIR/tools/vscode-gof"

if [[ "$DIST_DIR" = /* ]]; then
  TARGET_DIR="$DIST_DIR"
else
  TARGET_DIR="$ROOT_DIR/$DIST_DIR"
fi

mkdir -p "$TARGET_DIR"

pushd "$EXTENSION_DIR" >/dev/null

if [[ "$SKIP_INSTALL" != "1" ]]; then
  npm ci --no-audit --no-fund
fi

if [[ "$SKIP_TESTS" != "1" ]]; then
  npm test
fi

npm run package

asset_name="$(node -p "const pkg=require('./package.json'); `${pkg.name}-${pkg.version}.vsix`")"
if [[ "$(realpath "$asset_name")" != "$(realpath -m "$TARGET_DIR/$asset_name")" ]]; then
  cp "$asset_name" "$TARGET_DIR/$asset_name"
fi
echo "Created $TARGET_DIR/$asset_name"

popd >/dev/null
