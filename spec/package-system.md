# Package System

## Files

- source: `*.gof`
- module manifest: `gof.mod`
- lockfile: `gof.lock`

## Manifest shape

```toml
module = "example/hello"
edition = "2026"

[dependencies]
```

## Resolver rules

- semver-compatible resolution
- immutable registry artifacts
- content-addressed local cache
- deterministic lockfile updates
- no arbitrary postinstall scripts
