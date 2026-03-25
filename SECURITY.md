# Security Policy

## Supported branch

The only supported branch for security fixes is:

| Branch | Supported |
| --- | --- |
| `main` | Yes |

Preview and experimental work may change quickly and should not be treated as a
stable security target.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability.

Instead:

1. Open a private GitHub security advisory for this repository if available.
2. If private advisory reporting is unavailable, contact the maintainer through
   the repository owner's GitHub contact channel.
3. Include a clear reproduction, impact assessment, affected files or commands,
   and whether the issue is compiler-only, runtime-only, or toolchain-wide.

Useful report contents:

- affected commit, branch, or release
- minimal reproduction
- expected behavior
- actual behavior
- potential impact
- any proposed mitigation

## Response expectations

The project will try to:

- acknowledge the report promptly
- reproduce and assess severity
- prepare a fix with tests
- document the change when a public patch is ready

## Scope notes

The highest-priority security areas in `gof` are:

- compiler correctness leading to unsafe generated behavior
- package or module resolution trust boundaries
- runtime concurrency and memory safety invariants
- unsafe or FFI surfaces once they become available
