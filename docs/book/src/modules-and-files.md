# Modules and Files

The bootstrap module system is intentionally small and strict.

That is a feature, not a bug. The project is trying to stabilize semantics before
pretending that a full package ecosystem already exists.

## Local imports

`import name` currently resolves `name.gof` next to the importing file.

Example:

```gof
import math

fn main() -> int:
    return square(9)
```

with `math.gof`:

```gof
fn square(x: int) -> int:
    return x * x
```

## How the current bootstrap graph behaves

Today the module graph:

- works for same-directory imports
- loads imported files into one merged bootstrap module graph
- rejects import cycles
- rejects duplicate top-level functions
- rejects duplicate top-level structs
- rejects duplicate top-level enums
- rejects conflicting top-level names across functions, structs, and enums

That strictness exists because constructors, type names, and callable names must
stay unambiguous.

## How to organize small programs today

The current best practice is:

- keep related files in one directory
- split helper logic into clearly named modules
- avoid clever import trees
- keep top-level names distinct and intentional

For example:

- `main.gof` for entrypoint logic
- `math.gof` for math helpers
- `models.gof` for `struct` and `enum` declarations

## What is not there yet

Current non-goals:

- no full package registry
- no published dependency resolution flow
- no arbitrary package install scripts
- no large namespacing system yet

That means you should think of the current module system as:

> a clean local composition model, not a finished ecosystem story.
