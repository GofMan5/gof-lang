# RFC 0021: Deterministic Lockfile and Local Package Identity

## problem

RFC 0020 established a usable local package workflow, but it deliberately stopped
before reproducibility:

- local package identity was still implicit
- manifest-backed `run` and `build` could execute without a committed graph snapshot
- there was no deterministic artifact that captured the resolved local package graph

That left M6 incomplete. Multi-package development existed, but it was still too
easy for graph drift to stay invisible.

## proposed change

Add a deterministic `gof.lock` for local path packages and make it mandatory for
manifest-backed execution, build, and package-aware test flows.

### CLI contract

- `gof mod resolve [--dir <path>]` is the only command that writes `gof.lock`
- `gof run`, `gof build`, and package-aware `gof test` require a fresh lockfile
- single-file workflows outside any `gof.mod` root stay lockfile-free
- runtime commands never rewrite `gof.lock` implicitly

### identity contract

Each locked package identity is:

- `module`
- plus a SHA-256 digest of the normalized package root path relative to the root package

This keeps identity stable for local graphs without introducing registry or
version-solving semantics too early.

### freshness contract

Lockfile freshness is based on manifest graph metadata:

- module name
- edition
- normalized dependency path map

Normal `.gof` source edits do not stale the lockfile.

### graph rules

The local graph must reject:

- the same `module` resolved from two different package roots
- the same package root observed with conflicting manifest metadata
- dependency cycles through local manifests
- missing or malformed dependency manifests

### lockfile shape

`gof.lock` stores:

- `version = 1`
- root package metadata
- one deterministic entry per resolved package
- package identity
- root-relative source path
- manifest digest
- sorted dependency module references
- entrypoint path (`src/main.gof` or `src/lib.gof`)

## non-goals

- registry resolution
- semantic version solving
- workspace-scale package graphs
- package publishing
- arbitrary install or postinstall scripts

## alternatives considered

- auto-writing `gof.lock` during `run` and `build`
- storing only raw manifest text digests instead of normalized semantic metadata
- deferring identity until registry work exists

These were rejected because they either hide graph changes, make lockfiles noisy,
or postpone a package invariant that Phase 1 already needs.

## compatibility impact

- manifest-backed package commands now fail without a fresh `gof.lock`
- single-file workflows are unchanged
- registry, version ranges, and workspace semantics remain intentionally undefined

## diagnostics impact

- `GOF3090`: missing lockfile for manifest-backed package execution, build, or test
- `GOF3091`: stale or unreadable lockfile for a manifest-backed package
- `GOF3092`: conflicting package identity or metadata in the local dependency graph

## test plan

- unit coverage for lockfile serialization, digest stability, and stale detection
- graph tests for conflicting modules, cycles, and moved dependency paths
- CLI coverage for `gof mod resolve`, missing lockfile, stale lockfile, and fresh execution
- committed example lockfile in `examples/package_app`

## benchmark impact

No new benchmark surface is required for M6 closure. The resolver and lockfile
work must remain deterministic and cheap enough to stay off the critical path of
normal source editing, but performance gates stay part of later tooling/runtime
milestones.

## rationale

This closes the honest M6 gap without pretending that registry, semver, or
workspace-scale tooling are already solved.

The repository now has a deterministic local package graph. The next package
step is ecosystem scale, not local reproducibility.
