# HAMT floor

The question: how fast can a Clojure persistent hash map be on WasmGC under the workspace's wasmtime? After M15, Suss's `(get m k)` and `(assoc m k v)` on a 1,024-key map are 90–120× JVM Clojure (#384). This experiment measures how much of that gap the platform imposes and how much is Suss's to remove.

It isn't part of the Suss workspace or its tests.

## What it measures

`src/hamt.wat` is a persistent hash map written directly in WasmGC. Its design is `clojure.lang.PersistentHashMap`'s:
- bitmap-indexed nodes, each a 32-bit bitmap and an array of key/value pairs;
- a null key marking a child node;
- path copying on `assoc`.

Keys and values are i31 refs held as `eqref`, as Suss's small longs are, and the hash is Clojure's `Murmur3.hashLong`.

`src/main.rs` builds the map `(zipmap (range 1024) (range 1024))` and checks every key. It then times these kernels, each called through a WIT-free core export:

| Kernel | Loop body, `n` times |
| --- | --- |
| `control` | build the key `(bit-and i 1023)` and test a result |
| `get_typed` | `(get m (bit-and i 1023))` with direct calls: the floor |
| `get_dispatch` | the same, but the lookup, the hash and each key comparison go through `call_ref`, as a Suss closure or protocol call does |
| `assoc_typed` | `(assoc m (bit-and i 1023) i)` on the same map each time, direct calls |
| `assoc_dispatch` | the same through `call_ref` |

`jvm.clj` runs the same loops in JVM Clojure on a real `PersistentHashMap`.

**Differences from Clojure**, which make this floor slightly pessimistic:
- Clojure turns a node with 16 or more children into an `ArrayNode`, indexed without a popcount, so its root here would be one. This map keeps bitmap nodes throughout.
- Full-hash collisions trap instead of building a collision node; keys 0–1023 have none.

## Running

```sh
cd experiments/hamt-floor
# collector (auto, copying, drc or null), n, reps
cargo run --release -- copying 20000000 5
# one kernel, once, for an external instruction count
cargo run --release -- one get_typed 20000000 copying
# the expanded module, or the precompiled ELF for a disassembler
cargo run --release -- wat
cargo run --release -- cwasm /path/to/hamt.cwasm
# JVM Clojure 1.12
clojure -M jvm.clj 20000000 5
```

The settings:
- **Heap.** `GC_INITIAL_MIB` sets the initial GC heap. It defaults to 64, Suss's `GC_HEAP_INITIAL_SIZE`; 0 gives wasmtime's own default.
- **Profiling.** `HAMT_PERFMAP=1` writes a perf map for `perf` or macOS `sample`.
- **Features.** The engine enables the same Wasm features as `portable_session`: GC, function references, tail calls and exceptions.
- **Lockfile.** Don't commit `Cargo.lock`.

## Results

Apple M3 Pro, wasmtime 49.0.1, JVM Clojure 1.12.6 on Java 21.0.2. The machine had a load of about 10 from other work, so times vary between runs; the times below are the three repeated runs in [results/run_m3pro.txt](results/run_m3pro.txt).

Instructions are macOS "instructions retired" for the whole process: the slope between `n` and `2n` runs, including the loop.

| Operation | JVM Clojure | WasmGC floor, direct | WasmGC floor, `call_ref` | Suss at ebda997f |
| --- | --- | --- | --- | --- |
| `get`, ns | 18.9 | 23.2–23.4 | 22.4–22.6 | 1,745 |
| `get`, instructions | — | 295 | 310 | 31,989 |
| `assoc`, ns | 64.6 | 119–120 | 118–121 | 4,141 |
| `assoc`, instructions | — | 2,221 | 2,236 | 73,316 |

So on wasmtime 49:

- **`get` can run at JVM speed.** The floor is 1.2× JVM Clojure, and Suss is 92× it.
  - Disassembled, the walk loop is about 50 instructions per trie level, including bounds and overflow checks. The casts to the node type, which is final, are an inline type-id compare, with no `is_subtype` call.
  - About 31,700 of Suss's 32,000 instructions are Suss's own.
- **`assoc` can run within about 2× the JVM.** The floor is 1.9× JVM Clojure, and Suss is 35× it. The floor's extra cost over the JVM is allocation and collection.
- **Dynamic dispatch alone costs almost nothing.** Moving the operation, the hash and the equality behind `call_ref` adds 15 instructions to `get` and doesn't change either time. The costs that M15's profiles show elsewhere don't appear in this floor:
  - casts to a non-final `$closure` type, with wasmtime's `is_subtype` host call;
  - `protocol-marker-satisfies`;
  - boxed hashes;
  - core's call chains.

  Those are Suss's choices, not the platform's (#378, #387).
- **The collector's heap size matters enormously, and Suss already sets it.**

  | Collector and heap | `assoc` |
  | --- | --- |
  | copying, 64 MiB initial heap (Suss's default) | 120 ns |
  | null, which never frees | 152 ns |
  | DRC | 1,826 ns |
  | copying, wasmtime's default initial heap | 4,167 ns |

  With wasmtime's default heap, the copying collector collects on almost every `assoc`. Suss's own `assoc` costs the same 73k instructions at 64 and 256 MiB, so its cost isn't the collector.

## Conclusion

The platform isn't what limits Suss's collections. On wasmtime 49, a straightforward WasmGC HAMT is within 1.2× of JVM Clojure for `get` and 2× for `assoc`, even with every call made indirect. The 90–120× gap that remains in Suss is about 100× more executed instructions than the floor, and it belongs to Suss's runtime and codegen.

The open issues for that work:
- #378, core calls and type-directed specialization;
- #387, `hash` and `=`;
- #385, boxed longs;
- #389, arrays.

## Further variants worth adding

- An `ArrayNode` root, to match Clojure exactly.
- Keys typed as a non-final struct, with `ref.cast` to a non-final type, to price wasmtime's `is_subtype` path that Suss's `$closure` casts take.
- Boxed long keys beyond i31 (#385), and string keys.
- The same module on V8 (Node), to compare its compiler and collector with wasmtime's.
