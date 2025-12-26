# Sample Programs

Classic Clojure/ClojureScript programs used to track Suss implementation progress.

These programs represent **target behavior** - idiomatic Clojure code that we're working toward executing. As features are implemented, more samples will run successfully.

## Samples

| File | Description | Status |
|------|-------------|--------|
| `fibonacci.suss` | Fibonacci implementations (naive, tail-recursive, sequence) | Partial |
| `factorial.suss` | Factorial (tail-recursive, reduce-based) | Partial |
| `game_of_life.suss` | Conway's Game of Life (Christophe Grand's elegant version) | Not yet |
| `primes.suss` | Prime number algorithms (trial division, sieve) | Not yet |
| `quicksort.suss` | Functional quicksort | Not yet |
| `tree_traversal.suss` | Binary tree traversals using maps | Not yet |

## Feature Dependencies

| Sample | Required Features |
|--------|-------------------|
| `fibonacci.suss` | loop/recur ✓, closures ✓ |
| `factorial.suss` | loop/recur ✓, reduce, range |
| `game_of_life.suss` | destructuring, for, mapcat, frequencies |
| `primes.suss` | filter, some, range, Math/sqrt |
| `quicksort.suss` | filter, concat |
| `tree_traversal.suss` | keyword lookup, concat |

## Running Samples

```bash
# Run a sample (once features are implemented)
cargo run -p suss-cli -- samples/fibonacci.suss

# Or use the REPL
cargo run -p suss-cli
suss> (load-file "samples/fibonacci.suss")
suss> (fib 10)
```

## Sources

- **Game of Life**: [Christophe Grand's blog](http://clj-me.cgrand.net/2011/08/19/conways-game-of-life/) (2011)
- **Other samples**: Classic functional programming examples adapted for Suss
