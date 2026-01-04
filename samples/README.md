# Sample Programs

Classic Clojure/ClojureScript programs used to track Suss implementation progress.

These programs represent **target behavior** - idiomatic Clojure code that we're working toward executing. As features are implemented, more samples will run successfully.

## Samples

| File | Description | Status |
|------|-------------|--------|
| `fibonacci.sus` | Fibonacci implementations (naive, tail-recursive, sequence) | Partial |
| `factorial.sus` | Factorial (tail-recursive, reduce-based) | Partial |
| `game_of_life.sus` | Conway's Game of Life (Christophe Grand's elegant version) | Not yet |
| `primes.sus` | Prime number algorithms (trial division, sieve) | Not yet |
| `quicksort.sus` | Functional quicksort | Not yet |
| `tree_traversal.sus` | Binary tree traversals using maps | Not yet |

## Feature Dependencies

| Sample | Required Features |
|--------|-------------------|
| `fibonacci.sus` | loop/recur ✓, closures ✓ |
| `factorial.sus` | loop/recur ✓, reduce, range |
| `game_of_life.sus` | destructuring, for, mapcat, frequencies |
| `primes.sus` | filter, some, range, Math/sqrt |
| `quicksort.sus` | filter, concat |
| `tree_traversal.sus` | keyword lookup, concat |

## Running Samples

```bash
# Run a sample (once features are implemented)
cargo run -p suss-cli -- samples/fibonacci.sus

# Or use the REPL
cargo run -p suss-cli
suss> (load-file "samples/fibonacci.sus")
suss> (fib 10)
```

## Sources

- **Game of Life**: [Christophe Grand's blog](http://clj-me.cgrand.net/2011/08/19/conways-game-of-life/) (2011)
- **Other samples**: Classic functional programming examples adapted for Suss
