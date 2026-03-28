# stdlib

The standard library is intentionally separate from `core`.

Current first-wave shipped module names:

- `bytes`
- `io`
- `time`
- `net`
- `http`

These names are now reserved import targets. `import http` resolves to
`stdlib/http.gof`, not to a same-directory module or local dependency alias.

Planned first-wave areas:

- collections
- io
- net
- time
- json
- test support

No surface in `stdlib/` should be treated as stable until it has spec text and conformance coverage.
