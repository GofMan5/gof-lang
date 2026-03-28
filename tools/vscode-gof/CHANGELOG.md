# Change Log

## 0.2.1

- Added syntax coverage for `run_process(...)`
- Added a process-orchestration snippet for explicit captured-command workflows
- Kept the editor surface aligned with the new bootstrap process stdlib contract

## 0.2.0

- Added compiler-backed VS Code diagnostics via `gof check --json --stdin`
- Added extension commands for diagnostics output and manual refresh
- Added settings for debounce, save-only checks, explicit toolchain path, and cargo fallback
- Extended builtin syntax coverage for `read_lines`, `write_lines`, `toml_parse`, `csv_parse`, and `csv_stringify`
- Added diagnostics helper tests alongside packaging and grammar coverage

## 0.1.0

- Initial VS Code syntax baseline for `gof`
- Comments, strings, numbers, keywords, builtins, type names, enum variants
- Indentation-aware language configuration and packaging flow
- Starter snippets for modules, functions, enums, structs, `select`, and task joins
- Marketplace-ready icon and manifest metadata
