# gof Language Support for VS Code

This extension provides an installable editor baseline for `gof`:

- `.gof` file association
- syntax highlighting
- starter snippets for the core language surface
- comment toggling with `#`
- bracket pairing
- indentation rules for colon-ended blocks
- marketplace-ready packaging metadata and icon

This is intentionally an editor baseline, not a full IDE layer yet. It does not
ship LSP, debugger, profiler, rename support, or semantic analysis.

## Marketplace readiness

The extension is already shaped so other users can install it without manual
repo surgery:

- a dedicated extension icon
- gallery banner metadata for Marketplace/Open VSX listings
- bundled snippets for common `gof` constructs
- packaging tests that validate manifest metadata, snippets, and icon shape

It still does not claim full language tooling. The package is ready to publish,
but the language-platform layer is still future M12 work.

## Local development

From this directory:

```bash
npm install
npm test
npm run package
```

That produces a `.vsix` package in this directory.

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
