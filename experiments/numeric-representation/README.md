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
with `suss run --invoke`, which meters no fuel, as one process per run, less
the process's startup; the JVM runs every case in one process with
`System/nanoTime`, after three warm-up runs of each. JVM forms carry the array
type hints Clojure code would use, and box a long where the shared form would
reflect. The JVM is a local measuring tool, as for the suite oracle.

```sh
cargo build --profile test -p suss-cli --bin suss --locked
python3 experiments/numeric-representation/suss-jvm/bench.py 5000000 3
```

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
