# Language Platform Program

## Goal

Keep `gof` development ordered across language semantics, stdlib, runtime,
tooling, and ecosystem work without collapsing everything into a single
monolithic roadmap file.

## Contract

- `roadmap.md` tracks milestone/checkpoint status only
- major delivery programs live in `plans/roadmap/`
- every meaningful public feature has:
  - status in `roadmap.md`
  - design trace in `rfcs/` or `adrs/` when required
  - rollout detail in one plan file here when the scope is multi-slice

## Non-goals

- replacing RFCs or ADRs
- duplicating the full language spec
- becoming a second source of truth for implementation status

## Diagnostics Impact

- none directly
- plans must call out diagnostics additions when a program changes user-visible
  error behavior

## Tests

- keep tests tied to the real implementation files, not the plan folder
- use this directory to define required test layers before implementation starts

## Exit Criteria

- new multi-slice programs land with a dedicated plan subtree
- `roadmap.md` stays readable as a summary index instead of a dumping ground
