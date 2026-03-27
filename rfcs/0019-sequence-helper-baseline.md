# RFC 0019: Sequence Helper Baseline

## problem

The bootstrap language already has list literals, indexing, `append`, and `range`,
but the operational surface for common sequence work is still too raw:

- callers must spell `values[0]` or `values[len(values) - 1]` directly even when the
  empty-list failure path should be explicit
- extracting a middle window still requires manual indexing patterns with no shared
  error contract
- reversing, sorting, or selecting extrema has no standard helper surface yet

That leaves a gap in M4 and pushes users toward ad hoc helpers instead of one
predictable allocation-visible stdlib contract.

## proposed change

Add a minimal explicit sequence-helper slice:

- `first(list)` -> `Result[element, RuntimeError]`
- `last(list)` -> `Result[element, RuntimeError]`
- `slice(list, start, end)` -> `Result[list[element], RuntimeError]`
- `reverse(list)` -> `list[element]`
- `sort(list)` -> `list[element]`
- `min(list)` -> `Result[element, RuntimeError]`
- `max(list)` -> `Result[element, RuntimeError]`

Detailed rules:

- `first` and `last` return `RuntimeError.EmptySequence(message)` on an empty list
- `slice` returns `RuntimeError.Slice(message)` when indices are negative, when
  `start > end`, or when `end` exceeds the list length
- `reverse` returns a new list and does not mutate its input
- `sort` returns a new list and is currently restricted to `list[int]` and
  `list[string]` so the ordering contract stays deterministic
- `min` and `max` return `RuntimeError.EmptySequence(message)` on an empty list
- `min` and `max` are currently restricted to `list[int]` and `list[string]`
- builtin helper names remain fallback-only; a top-level user function with the
  same name shadows the builtin call

The type layer rejects invalid helper operands with `GOF3088`.

## alternatives considered

- Make `first` and `last` panic or emit runtime diagnostics on empty lists.
  Rejected because recoverable sequence-shape failures should use the same
  `Result[..., RuntimeError]` contract as other operational helpers.
- Add mutating helpers such as in-place `sort`.
  Rejected because the current language direction prefers explicit value
  construction over hidden mutation.
- Allow `sort` for arbitrary comparable values.
  Rejected because ordering is intentionally narrow today and widening it would
  create more surface area than the type system can defend cleanly.

## compatibility impact

This is additive for well-typed programs.

The only behavior-sensitive compatibility rule is name resolution:
top-level user functions now shadow builtin helper names instead of builtins
acting like reserved global call targets.

## diagnostics impact

Add `GOF3088` for invalid sequence-helper operands.

Expected behavior:

- `first(1)` is rejected in the type layer
- `slice([1, 2], "0", 1)` is rejected in the type layer
- `sort([true, false])` is rejected in the type layer
- `min([true, false])` is rejected in the type layer
- `max(1)` is rejected in the type layer
- empty-list and invalid-slice runtime states become `RuntimeError` values rather
  than generic evaluator diagnostics

## test plan

- typed HIR unit tests for valid and invalid sequence-helper contracts
- evaluator tests for `first`, `last`, `slice`, `reverse`, `sort`, `min`, and `max`
- regression coverage proving user-defined functions shadow builtin names
- pipeline coverage for sequence-helper lowering into SSA
- pass/fail fixtures and CLI example coverage
- spec/book/README/roadmap synchronization verified through repo checks

## benchmark impact

No dedicated benchmark gate is required for this bootstrap slice.

The helpers keep the cost model explicit:

- `reverse` and `sort` allocate new lists
- `sort` uses deterministic scalar ordering only
- no hidden mutation or dynamic comparator dispatch is introduced
