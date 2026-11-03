# Atoms, volatiles and delays

Suss's atoms, volatiles and delays follow Clojure's `ARef`, `Atom`, `Volatile` and
`Delay` (#30). They are imported from the pinned ClojureScript source with patches
where Clojure behaves differently; each patch is listed in
`docs/compatibility/reviews.edn` with its rationale.

## Atoms

`atom`, `deref`/`@`, `reset!`, `reset-vals!`, `swap!` (every arity, including
variadic), `swap-vals!`, `compare-and-set!`, `set-validator!`, `get-validator`,
`add-watch` and `remove-watch`.

| | Clojure (followed) | Pinned ClojureScript |
| --- | --- | --- |
| Falsy validator result | throws "Invalid reference state" | "Validator rejected reference state" |
| Exception from a validator | propagates unchanged | same |
| `(atom x :validator f)` | validates `x` | stores `f` unchecked (CLJS-3447) |
| `(atom x :meta m)` | `m` must be a map | any value |
| Options | `(apply hash-map options)`: an odd count throws "No value supplied for key" | destructured |
| `compare-and-set!` | compares the current value by identity | compares with `=` |
| `set-validator!` | checks the current state first; returns nil | returns the validator |
| Watch order | the watch map's seq order (a hash map) | insertion order (an array map) |
| `remove-watch` | returns the reference | returns the reference |

Each change validates the new value, then sets it, then calls each watch as
`(f key ref old new)`. A rejected value changes nothing and notifies no one. A watch
whose function is nil is skipped, as in `ARef.notifyWatches`. Suss is
single-threaded, so `swap!` needs no retry loop: the function runs once.

`compare-and-set!` compares by identity, like Clojure's `AtomicReference`, and
treats equal keywords as identical because Clojure interns keywords. It validates
the new value before comparing, and notifies watches only when it sets. Clojure on
the JVM boxes numbers, so its result for two equal large numbers can depend on
boxing; Suss compares numbers by value.

Watches live in a `PersistentHashMap` from the first `add-watch`, as Clojure's
`ARef` starts from `PersistentHashMap.EMPTY`. Suss hashes keywords and lays out
the hash map as Clojure does, so the watch order should be Clojure's. That is by
construction: no JVM was run to compare it.

`reset!`, `reset-vals!`, `swap!`, `swap-vals!` and `compare-and-set!` use the
`IReset`, `ISwap` and `IDeref` protocols for references that are not atoms, as
ClojureScript does. Clojure throws a `ClassCastException` for those instead.

## Volatiles

`volatile!`, `vswap!`, `vreset!` and `volatile?`. A volatile has no validator or
watches. Like Clojure's macro, `vswap!` evaluates its volatile expression twice (to
deref and to reset) and each argument once.

## Delays

`delay`, `force`, `deref`, `realized?` and `delay?`. The body runs at most once.
If it throws, the delay keeps the exception, `realized?` becomes true, and every
later deref throws the same exception, as Clojure's `Delay` does. ClojureScript
reruns the body instead.

## Printing

Atoms, volatiles and delays print as Clojure prints a reference:
`#object[suss.core.Atom 0x… {:status :ready, :val …}]`, with `:pending` for an
unforced delay and `:failed` (and the exception) for a delay whose body threw. The
REPL can't yet display these objects directly as a result (#288); `pr-str` prints
them.

## Evidence

- `cargo test -p suss-compile --test core_state` covers:
  - watch order and arguments;
  - validators (message, rollback, propagation, `set-validator!`);
  - atom options;
  - identity `compare-and-set!`;
  - the `-vals!` variants and variadic `swap!`;
  - the protocol fallback;
  - volatiles;
  - delays (one run, the cached exception, printing).
- jank's clojure-test-suite: the `atom`, `add-watch` and `remove-watch` namespaces
  run and match the reviewed observations. Patch 0004 selects the suite's Clojure
  assertions for atom options, with corrections recorded in
  `tests/oracle/clojure-test-suite-corrections.json`.
- `cargo test -p suss-cli --test portable_atoms --test persistent_repl`: the
  original GC, alias and operand-order cases and the 28-case ClojureScript corpus.
