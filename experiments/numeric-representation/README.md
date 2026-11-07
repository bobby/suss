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
