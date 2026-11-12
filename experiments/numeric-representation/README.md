# Numeric representation microbenchmark

Evidence for [ADR-0003](../../docs/adr/0003-clojure-numeric-tower.md) (#295):
what candidate number representations cost in WasmGC on the workspace's
wasmtime. It isn't part of the Suss workspace or its tests.

`src/bench.wat` has one kernel per representation. Each loops `n` times doing
`acc = acc + step` through `eqref` values, reading `step` from a mutable global
so it can't be folded. `src/main.rs` runs each kernel `reps` times (default 5), reports the best time
per iteration and peak RSS during that kernel, and checks the result. An
optional live heap of `live` boxed longs, held in an array for the whole run,
gives the copying collector real work to copy at each collection. Last, it
times `a_f64box` in chunks of 10⁵ additions and reports the median, the 99th
percentile and the slowest chunk; a collection shows as a slow chunk.

| Kernel | Representation |
| --- | --- |
| `d_i64_plain` | Unboxed `i64` locals, no overflow check (lower bound) |
| `d_i64_checked` | Unboxed `i64` with Clojure's long-overflow check |
| `a_f64box` | Boxed `f64` struct: unbox, add, rebox (Suss today) |
| `b_i64box` | Boxed `i64` struct with overflow check |
| `c_i31` | i31 when the value fits, boxed `i64` otherwise, inline |
| `c_i31_tagged` | ADR-0003's tagged i31: `(v << 1) \| 1`, even values reserved |
| `c_i31_boxedrange` | The same code on values outside the i31 range |
| `e_call_f64only` | `a_f64box` as an out-of-line call |
| `e_poly_*` | Out-of-line `+` that tests the operand types first |

## Running

```sh
cd experiments/numeric-representation
# Arguments: collector (auto, copying, drc or null; default auto), n, reps, live
cargo run --release -- copying 50000000 5
cargo run --release -- copying 20000000 5 1000000    # with 10^6 live objects
```

It uses the workspace's pinned wasmtime 49.0.1 and the same Wasm features as
`portable_session` (GC, function references, tail calls, exceptions).

## Results

Intel i7-1165G7, Linux 7.2.8, not isolated (load about 2.5). Nanoseconds per
iteration. Each column gives the range across the runs listed; repeat runs vary
by up to 10% under copying and more under DRC and null. Raw output is in
[results](results).

- Copying: `run_copying_rss` (copying, n = 2·10⁷) and `run_auto` (Auto, which
  selects copying, n = 5·10⁷).
- DRC: `run_drc_rss` (n = 2·10⁷) and `run_drc` (n = 5·10⁷).
- Null: `run_null_rss` and `run_null`, n = 2·10⁶, best of three, because it
  never frees.

| Kernel | Copying (default) | DRC | Null |
| --- | --- | --- | --- |
| `d_i64_plain` | 0.27–0.30 | 0.27–0.28 | 0.26–0.30 |
| `d_i64_checked` | 1.24–1.30 | 2.09–2.12 | 1.09–1.14 |
| `a_f64box` | 3.76–4.06 | 113–117 | 3.95–7.12 |
| `b_i64box` | 3.13–3.27 | 112–114 | 6.21–7.41 |
| `c_i31` | 1.84–1.89 | 2.07–2.09 | 1.86–2.28 |
| `c_i31_boxedrange` | 7.84–8.07 | 115–117 | 11.1–12.9 |
| `e_call_f64only` | 4.91–5.09 | 113–152 | 8.78–10.4 |
| `e_poly_i31` | 2.61–2.66 | 2.63–3.23 | 3.07–3.66 |
| `e_poly_i64box` | 11.6–11.7 | 118–142 | 16.1–18.1 |
| `e_poly_f64box` | 10.4–10.5 | 117–153 | 14.6–16.4 |

`c_i31` uses untagged i31 values and treats every i31 as a number.
`c_i31_tagged`, measured only in the second set of runs below, costs
0.3–0.4 ns more per addition under copying, for its parity test and
subtraction. Its slow path traps instead of calling out, so it doesn't measure
the cost of carrying a real fallback; it also only works for small `start`.

- Suss never calls `Config::collector`, so it gets `Collector::Auto`, which in
  wasmtime 49.0.1 selects the copying collector. Copying and null allocate
  inline; DRC calls into the host for every `struct.new`, and with these small
  heaps a box costs about 110–150 ns.
- Peak RSS stayed at 15 MiB under copying and DRC. Under null it grew about 16
  bytes per box.
- These kernels keep almost nothing live, which flatters the copying
  collector; a large live heap makes each collection cost more. BigInt, Ratio and
  BigDecimal weren't measured.

## Live heap

A second set of runs, on 2026-11-06, while `uptime` reported a load of 2.9–4.1,
so absolute figures are noisier than above. Nanoseconds per addition, then the
median, 99th-percentile and slowest of the chunks of 10⁵ `a_f64box` additions.

| Run | `a_f64box` | `b_i64box` | `c_i31` | `c_i31_tagged` | `d_i64_checked` | Median chunk | p99 chunk | Slowest chunk |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Copying, no live heap (`run_copying_live0`) | 4.5 | 3.4 | 2.0 | 2.4 | 1.3 | 0.46 ms | 0.63 ms | 0.67 ms |
| Copying, 10⁶ live (`run_copying_live1000000`) | 22.2 | 21.2 | 1.9 | 2.2 | 1.2 | 0.42 ms | 25 ms | 27 ms |
| Copying, 4·10⁶ live (`run_copying_live4000000`) | 24.5 | 21.7 | 1.9 | 2.2 | 1.3 | 0.41 ms | 67 ms | 67 ms |
| DRC, no live heap (`run_drc_n2m_live0`) | 118 | 121 | 2.5 | 3.2 | 1.9 | 12.1 ms | 12.3 ms | 12.3 ms |
| DRC, 10⁶ live (`run_drc_live1000000`) | 21.6 | 21.4 | 2.3 | 2.4 | 2.2 | 2.3 ms | 312 ms | 312 ms |

- Under copying, a live heap makes every boxed number five to six times as
  expensive, and pauses grow with it: about 25 ms per collection at 10⁶ live
  objects and 67 ms at 4·10⁶. Unboxed and i31 arithmetic don't allocate, so
  they don't change.
- DRC allocates through a host call (`gc_alloc_raw`) for every `struct.new`.
  With a small heap a box costs 110–150 ns; with 10⁶ live objects, about the
  same as copying. Why wasn't investigated. The DRC runs use n = 2·10⁶, so
  there are only 20 chunks and the p99 is the slowest one. Its 312 ms chunk is
  a single sample of unknown cause; without a live heap no chunk stands out.
- Peak RSS at 4·10⁶ live objects was 505 MiB, against 260 MiB during the
  unboxed kernels, which run first. That is consistent with copying keeping
  two halves of its heap.

## Sentinel designs (#335)

#335 measures ADR-0003's tagged encoding against moving the sentinels out of
i31, before ABI v3 fixes the encoding. Three designs:

- **A, tagged:** a long in 30 bits is the i31 `(v << 1) | 1`; nil, false, true
  and undefined are the even i31 values 0, 2, 4 and 6, as today.
- **B, sentinels outside i31:** every i31 is a 31-bit long; nil is `ref.null`,
  and false, true and undefined are singleton structs.
- **C:** B with nil a singleton struct too, so a value is never null.

Each kernel has a real out-of-line fallback. `setup2` builds a 1,024-value
array mixing i31 longs, boxed longs, nil, false and true (5:1:1:1 per 8) in
each design's encoding.

| Kernel | What it does |
| --- | --- |
| `f_addA`, `f_addB` | `acc + 1` with the i31 fast path inline and a boxing fallback |
| `g_truthyA`, `g_truthyB`, `g_truthyC` | `if`'s test (falsy for nil, false, undefined) over the mixed array |
| `h_numberA`, `h_numberB` | `number?` over the mixed array: an odd i31 or a box, against any i31 or a box |
| `k_loopA`, `k_loopB` | `(loop [i 0] (if (< i n) (recur (inc i)) i))` through out-of-line polymorphic `<` and `inc`, where `<` returns a boolean sentinel |

Five runs on 2026-11-07, copying collector, `GC_INITIAL_MIB=64` (see below),
n = 5·10⁷, best of five within each run; nanoseconds per iteration, range and
median across the runs ([run_sentinels_copying](results/run_sentinels_copying.txt)):

| Kernel | A | B | C |
| --- | --- | --- | --- |
| Addition (`f_add`) | 2.36–2.51 (2.38) | 1.84–1.95 (1.87) | as B |
| Addition outside i31 (`f_add*_boxedrange`) | 7.72–8.33 (7.84) | 7.84–8.33 (7.88) | as B |
| Truthiness (`g_truthy`) | 2.27–2.49 (2.30) | 2.18–2.38 (2.25) | 2.27–2.45 (2.34) |
| `number?` (`h_number`) | 1.90–1.93 (1.92) | 2.04–2.09 (2.08) | as B |
| Counting loop (`k_loop`) | 5.12–5.23 (5.17) | 4.50–4.55 (4.53) | as B |

Moving the sentinels out of i31 makes small-long addition 21% faster and the
counting loop 12% faster. Truthiness is the same within noise in all three
designs: comparing against a singleton struct in a global costs no more than
against an i31 constant. `number?` is 0.16 ns slower in B and C, which need
two type tests where A needs a type test and a bit test. ABI v3 uses B: every
i31 is a long from −2³⁰ to 2³⁰ − 1, nil is `ref.null`, and false, true and
undefined are singleton structs. Suss's `Value` type is already a nullable
`eqref`, so B needs no new types, and a null is a constant expression that
default-initialized arrays and structs already hold, which C's nil struct
isn't.

## GC heap growth

`setup2`'s mixed arrays keep about 2,000 objects live, and with them
`a_f64box` costs 23 ns instead of 4. wasmtime 49.0.1 grows the GC heap only
when a collection leaves less free space than the pending allocation needs
(`collect_and_maybe_grow_gc_heap`), so a heap that live data almost fills is
collected every few allocations, each copying all of it. `GC_INITIAL_MIB` sets
`Config::gc_heap_initial_size`. `a_f64box`, ns per addition, n = 2·10⁷, best of
three ([run_gc_initial](results/run_gc_initial.txt),
[run_gc_initial_rss](results/run_gc_initial_rss.txt)):

| Initial heap | 0 live | 2,048 live | 65,536 live | 10⁶ live | 4·10⁶ live | Peak RSS, 0 live |
| --- | --- | --- | --- | --- | --- | --- |
| Default (0) | 4.1–4.7 | 20.4 | 131 | 22.4–23.3 | 22.1 | 16 MiB |
| 16 MiB | 4.2 | 4.5 | 9.0 | 20.1 | | |
| 64 MiB | 4.0 | 4.0 | 4.9 | 20.1–21.3 | 20.6 | 80 MiB |
| 256 MiB | 4.0–4.1 | 3.8 | 4.1 | 9.0–10.9 | 21.7 | 272 MiB |
| 1 GiB | 4.3 | | | 5.0 | 7.6 | 1,040 MiB |

i31 arithmetic doesn't allocate, so it stays at 1.8–2.0 ns throughout. Most
of the live-heap cost measured above comes from this growth policy, not from
the live heap's size alone. A larger initial heap avoids it, but the copying
collector touches all of its heap, so peak RSS grows by the same amount. The
pause for each collection still grows with the live heap: the slowest chunk is
about 17 ms at 10⁶ live objects and 64–72 ms at 4·10⁶, whatever the initial
size.

Suss sets a 64 MiB initial heap (`SUSS_GC_INITIAL_MIB` overrides it), chosen
from REPL measurements recorded in
[ADR-0003](../../docs/adr/0003-clojure-numeric-tower.md#collector).

## Suss against JVM Clojure (#336)

[`suss-jvm/bench.py`](suss-jvm/bench.py) times numeric operations in compiled
Suss against the same forms in JVM Clojure 1.12.6. Each case evaluates one
expression in a loop; its cost is the loop's time per iteration less that of a
control loop that evaluates only the operands. Suss runs a compiled component
with `suss run --invoke`, which meters no fuel, as one process per run; the JVM
runs every case in one process with
`System/nanoTime`, after three warm-up runs of each. JVM forms carry the array
type hints Clojure code would use, and box a long where the shared form would
reflect. The JVM is a local measuring tool, as for the suite oracle.

```sh
cargo build --release -p suss-cli --bin suss --locked
python3 experiments/numeric-representation/suss-jvm/bench.py 5000000 3
```

Since #347 the bench runs a release `suss`, and with `perf` installed also
reports user-space instructions per operation (`perf stat -e instructions:u`,
the count at n less the count at 0), which unlike time doesn't depend on
machine load. The results below this paragraph used a `--profile test` build,
whose debug assertions also apply to wasmtime and add about 106 instructions
to every call; [the M15 baseline](#m15-baseline-347) replaces them.

### Timing without negative results

Runs up to M15-09 (`run_m15_09.txt`) report time per iteration as a run's time
less the best startup of another case at n = 0, and each case less its
control. Both subtractions could go negative, and often did:

- **Startup jitter.** A run's startup, 0.2–0.35 s, varies by tens of
  milliseconds, as much as a fast case's whole loop or the 50 runs of a
  program. `(fib 20)` measured −313.9 µs per run on 06783f4d.
- **JVM boxing.** The JVM control `i` sinks a freshly boxed `Long` each
  iteration, while a case returning a cached value, such as `(f0)` or
  `(= :a :b)`, allocates nothing: 2.85 ns per iteration for the control
  against 2.03 for `(f0)`, so the difference is negative before any noise.
- **A foldable Suss control.** Each Suss iteration tested its result with
  `(identical? r nil)`, which the compiler folds away for a Bool result, so
  `(== i 7)` measured −8 to −14 instructions below its control.

Since then, a Suss case's time and instructions per iteration are the slope
between its best of three runs at a loop count and at twice that count, so
startup cancels; the count grows until the two runs differ by at least 0.5 s
of work. Each Suss iteration compares its result with a var, `sentinel`, which
the compiler can't fold. A time no larger than its run-to-run spread (a
case's, plus its control's), including any negative difference, prints as
≈ 0, below what the bench resolves; `history.py` reads ≈ 0 and earlier
negative times the same way. The JVM's boxing difference remains, so JVM
times for operations cheaper than boxing a long print as ≈ 0. Instruction
counts from before and after compare; net times from before don't.

Besides numeric operations and calls, the bench covers persistent
collections (`conj`, map `get` and `assoc`, keyword lookup, set
`contains?`), `str`, `apply` and a multimethod, and small programs run whole:
recursive `(fib 20)`, word frequencies with `reduce` and `assoc`, a
transducer, a lazy `map`/`filter` pipeline, `into` a vector and a map, and
string building. Since #347's second part, each JVM iteration stores its
result in a sink array, as its control loop does, so the JIT can't drop the
operation; JVM times recorded before that, including the M15 baseline's, are
often below 0.5 ns for that reason.

[`suss-jvm/history.py`](suss-jvm/history.py) builds
[m15_history.md](results/m15_history.md): instructions per operation for every
case, one column per M15 package, from the runs checked in under `results/`.
A package that changes performance adds its run and regenerates the table.

Results on 3950da6a, the same machine at load 1.1–2.6, best of three, in
[run_suss_jvm.txt](results/run_suss_jvm.txt). Nanoseconds per operation, less
the control; a control's row is its whole loop iteration:

| Case | Suss | JVM |
| --- | --- | --- |
| Loop of `<`, `inc` and `identical?` (control) | 87 | 0.29 |
| `(identity i)`, a call to a core function | 66 | 0.09 |
| `(+ i 7)`, `(== i 7)` | 0 | 0.2 |
| `(= i 7)` | 458 | 0.08 |
| `(= i 7.5)` | 466 | 5.6 |
| `(hash i)` | 192 | 4.5 |
| `(+ (double i) 0.5)` | 49 | 0 |
| `(hash (double i))` | 254 | 11 |
| `aget` of a `long-array`, `int-array` or `double-array` | 45–54 | 0.6 |
| `aset` of a `long-array` or `double-array` | 72–98 | 0.8 |
| `aget` of a 1024-element object array (`to-array`) | 46,296 | 3.9 |
| `(nth v i)` of a 1024-element vector | 2,572 | 2.2 |
| `(reduce + a)` of a packed array, per element | 302–318 | 20–21 |
| `(reduce + v)` of a vector, per element | 438 | 5.3 |

- Suss's numeric representation costs little. Inline arithmetic and `==` cost
  nothing measurable beyond the loop, a boxed double about 50 ns, and a packed
  array access about one call. A call to a core function costs about 66 ns.
- Protocol dispatch dominates (#349). In a separate run, `(-equiv i 7)` cost 319 ns,
  `(= i i)` 145 ns and `(= :a :b)` 2,067 ns. `(= i 7)` paid both: core's `=`
  asked `suss.bootstrap/equiv` and then, when two numbers differed, dispatched
  `-equiv`, which Clojure's `Util.equiv` doesn't for a number. Since #336's
  third part `=` answers a number directly, and `(= i 7)` costs 157 ns
  ([run_suss_equality.txt](results/run_suss_equality.txt)).
- An object array built by `.push`, as `to-array` builds one, keeps elements
  past its dense prefix in a chain of entries (`sparse_arrays.rs`), so an
  access is linear in the length and `reduce` over it quadratic (#348).
- Only the Suss side includes process startup noise, so differences under
  about 5 ns are within noise; negative differences are reported as 0.

## M15 baseline (#347)

The baseline for [M15](https://github.com/bobby/suss/milestone/15) (#349):
`bench.py` on 5e5dbc53, a release `suss` on wasmtime 49.0.1 against JVM
Clojure 1.12.6 on OpenJDK 21.0.13, an i7-1165G7 at load 5.5–9.6, best of three,
in [run_m15_baseline.txt](results/run_m15_baseline.txt). Every later M15
package reports its cases against these. Instructions are per operation, less
the control; times are less reliable at this load:

| Case | Suss instructions | Suss ns | JVM ns |
| --- | --- | --- | --- |
| Loop of `<`, `inc` and `identical?` (control) | 1,076 | 104 | 0.63 |
| `(identity i)`, a core function | 806 | 42 | 0.45 |
| `(f0)` to `(f3 i i i)`, user functions | 783–880 | 48–63 | 0–0.9 |
| `(fv i)`, a variadic function | 1,129 | 85 | 0 |
| `(g i)`, a local closure | 617 | 24 | 0 |
| `(identity i)` inside three `binding`s | 1,948 | 165 | 0 |
| `(ident b)`, a protocol method on a `deftype` | 2,552 | 228 | 0.81 |
| `(identical? i 7)`, `(nil? i)` | 834–949 | 50–56 | 0 |
| `(= :a :b)` | 23,202 | 2,755 | 0 |
| `(= i 7)` | 1,914 | 143 | 0.03 |
| `(+ i 7)`, `(== i 7)` | 59–79 | 0–6 | 0.1 |
| `(hash i)` | 2,374 | 232 | 6.0 |
| `aget` of a `long-array`, `int-array` or `double-array` | 700–787 | 65–109 | 0.7–1.0 |
| `(nth v i)` of a 1024-element vector | 34,675 | 3,530 | 3.3 |
| `(reduce + v)` of a vector, per element | 5,631 | 567 | 6.0 |

- A call costs about 800 instructions, whether to core, a user function of any
  fixed arity, or a closure; the closure saves the var read, about 190.
- Each enclosing `binding` adds about 380 instructions to an iteration that
  reads two vars, `identity` and `identical?`, since every var read searches
  the binding frames, about 190 per frame (M15-02, #354).
- The control loop itself costs 1,076 instructions, against 126 for a loop
  without its `identical?`, which is a full call (M15-03, #356).
- Keyword `=`, vector `nth` and protocol dispatch cost several calls each
  (M15-08, #355; M15-09, #361).
- JVM times under about 0.5 ns are within noise, and negative differences are
  reported as 0.

### Profiling

`SUSS_PROFILE=perfmap` makes `suss run` write `/tmp/perf-<pid>.map`, so
`perf record` and `perf report` name compiled functions as
`wasm[N]::function[M]`. Profile a release build: under `--profile test`,
wasmtime's type registry takes a lock on every function reference read from a
GC struct, a third of a call's cycles. Machine code for a component is in the
native component cache (`~/.cache/suss/cache/v1/native-component/`), a 97-byte
header followed by an ELF object that `objdump -d` reads.

## Unboxed loops (#337)

With unboxed numeric locals, a `loop` summing longs compiles to raw `i64`
locals with Clojure's inline overflow check, the same work as the
`d_i64_checked` kernel. Both were timed at n = 10⁹ on 19d03bcc, an Apple M3 Pro
at load 6.5–8, best of three, in
[run_m12_05_loops.txt](results/run_m12_05_loops.txt). The Suss time is the
`suss run` wall-clock time at n = 10⁹ less the time at n = 0:

| Loop | Suss ns per iteration | Kernel | Kernel ns per iteration | Ratio |
| --- | --- | --- | --- | --- |
| `(loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc i)) acc))` | 0.81 | `d_i64_checked` | 0.80 | 1.01× |
| `(loop [i 0 acc 0.0] (if (< i n) (recur (inc i) (+ acc 0.5)) acc))` | 0.83 | | | |

Before `bench.py` timed by slope, its whole-loop rows ran these loops at
n = 5·10⁶, where startup (about 0.2 s) dwarfed the loop (about 9 ms). They now
grow n until the loop's time stands out from startup's jitter, as the cases
do.

## Calling conventions (ADR-0004, #359)

`src/calls.wat` has one kernel per way of reaching a one-argument function
entry, for [ADR-0004](../../docs/adr/0004-calling-convention.md). Each loops
doing `acc = f(acc)` with an i31 increment as `f`, reading the callee from a
mutable global each iteration:

| Kernel | How the call reaches its entry |
| --- | --- |
| `inline` | No call: the increment inline (lower bound) |
| `direct` | A static `call` |
| `indirect`, `indirect_typed` | `call_indirect` by an index the closure holds, through a `funcref` table or one typed as the signature |
| `table_ref` | `table.get` of the typed table, then `call_ref` |
| `field_typed`, `field_untyped` | `call_ref` of a function reference read from the closure struct, typed or cast |
| `index_final` | `br_on_cast_fail` to a final closure type, the entry at table index `base + 1`, `call_indirect`, a capture read from a separate environment |
| `index_subtype` | The same with one closure subtype per function |
| `proposed` | ADR-0004's call site: `br_on_cast_fail` to a closure subtype, its function's code struct, the typed entry, `call_ref` |
| `today` | Suss's ABI 3 path: callable test, argument array, `invoke`, dispatcher, method |

```sh
cd experiments/numeric-representation
sh calls.sh > results/run_calls.txt    # both engines, times and instructions
cargo run --release --bin calls -- 20000000 5 indirect,today
target/release/calls --obj target/calls.cwasm && objdump -d target/calls.cwasm
```

`calls.py` counts user-space instructions per call with `perf stat`, (count at
n − count at 0) / n; with `--node` it counts V8 under `--single-threaded
--no-liftoff`, since a single call otherwise runs in Liftoff, V8's baseline
tier. `calls.mjs` times the same module on V8 with default tiering.

Results on 2026-11-10, i7-1165G7 at load 9–10, in
[run_calls.txt](results/run_calls.txt); instructions per call, less `inline`:

| Kernel | wasmtime 49.0.1 | V8 14 (Node 26.8.2), no feedback |
| --- | --- | --- |
| `direct` | 9 | 0 |
| `indirect` | 53 | 40 |
| `indirect_typed` | 50 | 38 |
| `table_ref` | 50 | 46 |
| `field_typed` | 205 | 32 |
| `field_untyped` | 173 | 37 |
| `index_final` | 82 | 57 |
| `index_subtype` | 172 | 57 |
| `proposed` | 328 | 49 |
| `today` | 402 | 90 |

- Wasmtime stores a function reference in a GC struct as an interned ID, and
  reading it calls `wasmtime_builtin_get_interned_func_ref`; `objdump` shows
  the call in `field_typed` and `field_untyped`.
- A cast to a non-final type whose value is a strict subtype calls
  `wasmtime_builtin_is_subtype`, the 90 instructions between `index_subtype`
  and `index_final`. `proposed` pays both host calls on wasmtime 49; ADR-0004
  keeps the platform's design anyway, and
  [`docs/upstream/`](../../docs/upstream/) has the issues to file.
- On V8 with default tiering, every kernel but `today` usually runs within
  about 1 ns of `inline`, since TurboFan inlines the monomorphic call; `today`
  costs 5–20 ns more. These times vary with load and tier-up timing.
