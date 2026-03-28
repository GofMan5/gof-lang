# gof Programming Language for VS Code

The `gof` VS Code extension gives the language a real editor surface instead of
fake syntax-only demo tooling.

It ships:

- `.gof` file association
- syntax highlighting for the current language surface, including `test fn` and
  `fixture(scope) fn`
- starter snippets for the shipped language and testing baseline
- compiler-backed diagnostics through `gof check --json --stdin`
- indentation rules, bracket pairs, and comment toggling

It does not pretend to be the final IDE layer yet. LSP, debugger, profiler,
rename, and go-to-definition remain future platform work.

## Install

Install from the VS Code Marketplace:

- [gof Programming Language](https://marketplace.visualstudio.com/items?itemName=gofman5.gof-language)

CLI install:

```bash
code --install-extension gofman5.gof-language
```

## Why this extension is different

The key design choice is simple: diagnostics come from the real compiler.

When a toolchain is available, the extension runs:

```bash
gof check /path/to/file.gof --json --stdin
```

That means:

- syntax and type errors stay aligned with the language contract
- unsaved edits are checked through stdin
- stale editor checks are cancelled instead of piling up
- there is no hidden editor-only validation layer drifting away from `gof`

Toolchain resolution order:

1. `gof.toolchain.path`
2. repo-local cargo fallback inside the `gof` repository
3. `gof` on `PATH`

## Local development

From this directory:

```bash
npm install
npm test
npm run package
```

The packaged extension artifact is left in this folder for manual Marketplace
upload by the maintainer.

## Maintainer note

If you need to repack without touching release assets:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-vscode-extension.ps1
```

That leaves the packaged artifact in `tools/vscode-gof/`.
