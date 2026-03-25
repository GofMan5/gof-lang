# Modules and Files

The bootstrap module system is intentionally small.

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

## Current limits

Today the module graph:

- works for same-directory imports
- rejects import cycles
- rejects duplicate top-level names across the merged bootstrap graph

Full package management is future work. For now, keep examples and small programs local and explicit.
