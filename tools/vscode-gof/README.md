# gof Language Support for VS Code

This extension provides an installable editor baseline for `gof`:

- `.gof` file association
- syntax highlighting
- starter snippets for the core language surface
- compiler-backed diagnostics powered by `gof check --json`
- comment toggling with `#`
- bracket pairing
- indentation rules for colon-ended blocks
- marketplace-ready packaging metadata and icon

This is intentionally an editor-first baseline, not the full IDE/platform layer
yet. It now does real compiler diagnostics, but it still does not ship LSP,
debugger, profiler, rename support, or go-to-definition.

## Compiler-backed diagnostics

When a `gof` toolchain is available, the extension runs:

```bash
gof check /path/to/file.gof --json --stdin
```

and maps the reported diagnostics back into VS Code. That means:

- syntax and type errors come from the real compiler, not hand-written editor heuristics
- the active buffer is checked through stdin, so diagnostics track unsaved edits
- diagnostics stay aligned with the language's stable CLI contract instead of hidden extension-only logic

The extension only publishes diagnostics for the active file it checked. If the
compiler reports an imported module error, open that `.gof` file directly to see
its inline diagnostics.

### Toolchain resolution order

The extension resolves the diagnostics toolchain in this order:

1. `gof.toolchain.path`
2. repo-local cargo fallback inside the `gof` repository
3. `gof` on `PATH`

The cargo fallback runs:

```bash
cargo run -q -p gof-cli --bin gof -- check /path/to/file.gof --json --stdin
```

That keeps the extension useful while working directly inside the `gof` repo,
even before a standalone CLI is installed globally.

### Settings

- `gof.diagnostics.enabled`
- `gof.diagnostics.debounceMs`
- `gof.diagnostics.runOnSaveOnly`
- `gof.toolchain.path`
- `gof.toolchain.useCargoFallback`

Use `gof: Recheck Active Document` to force an immediate refresh and
`gof: Show Diagnostics Output` to inspect toolchain errors or invalid JSON
payloads.

## Marketplace readiness

The extension is already shaped so other users can install it without manual
repo surgery:

- a dedicated extension icon
- gallery banner metadata for Marketplace/Open VSX listings
- bundled snippets for common `gof` constructs
- compiler-backed diagnostics commands and settings
- packaging tests that validate manifest metadata, snippets, icon shape, and diagnostics plumbing

It still does not claim full language tooling. The package is ready to publish
and useful for real editing, but the deeper M12 platform layer is still future
work.

## Local development

From this directory:

```bash
npm install
npm test
npm run package
```

That produces a `.vsix` package in this directory.

The test suite covers:

- TextMate grammar scopes
- manifest/configuration packaging contract
- diagnostics helper logic and toolchain resolution

## Local installation in VS Code

1. Open VS Code.
2. Open the Extensions view.
3. Run `Extensions: Install from VSIX...`.
4. Choose the generated `.vsix` file.

## How other users can install it

There are two sane distribution paths:

1. Ship the `.vsix` file as a GitHub release asset.
2. Publish the extension to the VS Code Marketplace.

The repository snapshot release path now also attaches the generated `.vsix`
asset, so users who do not want Marketplace publishing can still install the
same package directly from the rolling prerelease.

If you publish a `.vsix` only, users install it through `Install from VSIX...`.
If you publish to the Marketplace, users can install it directly from the
Extensions UI or with:

```bash
code --install-extension gofman5.gof-language
```

## Publishing

The extension already has a valid `publisher` field. To publish it:

```bash
npx @vscode/vsce login gofman5
npx @vscode/vsce publish
```

For VSCodium and Open VSX users, publish the same package to Open VSX as a
separate step if you want one-click installation there too.

At the repository level there is also a repeatable helper that packages the
same VSIX and publishes it from one command when the required tokens are set:

```powershell
$env:VSCE_PAT="..."
$env:OVSX_PAT="..."
powershell -ExecutionPolicy Bypass -File .\scripts\publish-vscode-extension.ps1
```

Use `-PackageOnly` if you only want the packaged `.vsix`, and `-DryRun` if you
want to verify the publish commands without sending anything.
