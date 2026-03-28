#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="${DIST_DIR:-tools/vscode-gof}"
SKIP_INSTALL="${SKIP_INSTALL:-0}"
SKIP_TESTS="${SKIP_TESTS:-0}"
SKIP_MARKETPLACE="${SKIP_MARKETPLACE:-0}"
SKIP_OPEN_VSX="${SKIP_OPEN_VSX:-0}"
PACKAGE_ONLY="${PACKAGE_ONLY:-0}"
DRY_RUN="${DRY_RUN:-0}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTENSION_DIR="$ROOT_DIR/tools/vscode-gof"

if [[ "$DIST_DIR" = /* ]]; then
  TARGET_DIR="$DIST_DIR"
else
  TARGET_DIR="$ROOT_DIR/$DIST_DIR"
fi

DIST_DIR="$TARGET_DIR" \
SKIP_INSTALL="$SKIP_INSTALL" \
SKIP_TESTS="$SKIP_TESTS" \
"$ROOT_DIR/scripts/package-vscode-extension.sh"

asset_name="$(node -p "const pkg=require('$EXTENSION_DIR/package.json'); `${pkg.name}-${pkg.version}.vsix`")"
vsix_path="$TARGET_DIR/$asset_name"

if [[ ! -f "$vsix_path" ]]; then
  echo "Expected packaged VSIX not found: $vsix_path" >&2
  exit 1
fi

if [[ "$PACKAGE_ONLY" = "1" ]]; then
  echo "Packaged VS Code extension only: $vsix_path"
  exit 0
fi

pushd "$EXTENSION_DIR" >/dev/null

if [[ "$SKIP_MARKETPLACE" != "1" ]]; then
  : "${VSCE_PAT:?VSCE_PAT is required to publish to the VS Code Marketplace.}"
  if [[ "$DRY_RUN" = "1" ]]; then
    echo "Dry run: npx @vscode/vsce publish --packagePath $vsix_path --skip-duplicate --pat [redacted]"
  else
    npx @vscode/vsce publish --packagePath "$vsix_path" --skip-duplicate --pat "$VSCE_PAT"
  fi
fi

if [[ "$SKIP_OPEN_VSX" != "1" ]]; then
  : "${OVSX_PAT:?OVSX_PAT is required to publish to Open VSX.}"
  if [[ "$DRY_RUN" = "1" ]]; then
    echo "Dry run: npx ovsx publish $vsix_path --pat [redacted]"
  else
    npx ovsx publish "$vsix_path" --pat "$OVSX_PAT"
  fi
fi

popd >/dev/null

if [[ "$DRY_RUN" = "1" ]]; then
  echo "Prepared VS Code extension package $vsix_path (dry run)"
else
  echo "Published VS Code extension package $vsix_path"
fi
