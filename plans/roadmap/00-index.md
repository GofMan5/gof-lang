# Roadmap Plans Index

This directory decomposes major `gof` programs into ordered, file-scoped
execution plans.

Rules:

- file names keep numeric prefixes
- one file owns one decision scope
- each file records goal, contract, non-goals, diagnostics impact, tests, and
  exit criteria
- `roadmap.md` remains the status index; these files hold the detailed rollout
  order
- RFCs and ADRs stay the normative design trace for stable public behavior

## Current plan set

- [10-language-platform.md](./10-language-platform.md)
- [20-testing-platform/00-overview.md](./20-testing-platform/00-overview.md)
- [20-testing-platform/01-language-surface.md](./20-testing-platform/01-language-surface.md)
- [20-testing-platform/02-runner-discovery-cli.md](./20-testing-platform/02-runner-discovery-cli.md)
- [20-testing-platform/03-fixtures-snapshots-doctests.md](./20-testing-platform/03-fixtures-snapshots-doctests.md)
- [20-testing-platform/04-property-fuzz-stress-bench.md](./20-testing-platform/04-property-fuzz-stress-bench.md)
- [20-testing-platform/05-ci-quality-gates.md](./20-testing-platform/05-ci-quality-gates.md)
