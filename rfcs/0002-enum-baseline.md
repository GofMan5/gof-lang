# RFC 0002: Enum Baseline

## Problem

`gof` can model aggregate data with `struct`, but it cannot yet model explicit states like `Ready | Busy | Failed` without falling back to ad hoc booleans or strings.

That blocks a clean path to `match`, typed results, and more reliable domain modeling.

## Proposed Change

Add a bootstrap enum baseline with:

- top-level `enum Name:`
- unit variants declared one per line inside the enum block
- type annotations that accept known enum names
- enum variant references through `EnumName.Variant`
- equality and inequality for values of the same enum type
- module-graph loading for imported enums
- diagnostics for duplicate enum variants, unknown variants, duplicate enum names in the module graph, and enum call misuse

This RFC intentionally excludes:

- payload variants
- generic enums
- pattern matching
- exhaustive analysis beyond simple variant lookup

## Alternatives Considered

- Encode states with strings:
  Rejected because it destroys type safety and makes `match` weaker later.
- Delay enums until `match`:
  Rejected because `match` without a real state type becomes surface syntax without semantic value.
- Add payload enums immediately:
  Rejected because it would expand memory layout, type checking, and runtime complexity too early.

## Compatibility Impact

- Existing code remains valid.
- `enum` is now active syntax instead of only being a reserved token.
- Top-level name conflicts are stricter because enums share the public module namespace with structs and functions.

## Diagnostics Impact

New diagnostics:

- `GOF3027`: duplicate enum variant in one declaration
- `GOF3028`: unknown enum variant reference
- `GOF3029`: duplicate top-level enum in the local module graph

Existing diagnostics also become more precise around type annotations and non-callable enum names.

## Test Plan

- parser tests for enum declarations and variant references
- formatter tests for enum layout
- typed HIR tests for enum type annotations, equality, duplicate variants, and unknown variants
- module-graph tests for imported enums and duplicate enum names
- interpreter tests for runtime enum equality and unknown variant failures
- pipeline tests for enum values reaching MIR and SSA
- pass/fail fixture coverage and runnable examples

## Benchmark Impact

No meaningful runtime benchmark change is expected from unit enum values alone.

The main benchmark obligation starts when enums gain payloads or feed into `match` lowering.
