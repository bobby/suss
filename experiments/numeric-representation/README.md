# Numeric representation microbenchmark

Evidence for [ADR-0003](../../docs/adr/0003-clojure-numeric-tower.md) (#295):
what candidate number representations cost in WasmGC on the workspace's
wasmtime. It isn't part of the Suss workspace or its tests.

`src/bench.wat` has one kernel per representation. Each loops `n` times doing
`acc = acc + step` through `eqref` values, reading `step` from a mutable global
so it can't be folded. `src/main.rs` runs each kernel five times, reports the
best time per iteration and peak RSS, and checks the result.

| Kernel | Representation |
| --- | --- |
| `d_i64_plain` | Unboxed `i64` locals, no overflow check (lower bound) |
| `d_i64_checked` | Unboxed `i64` with Clojure's long-overflow check |
| `a_f64box` | Boxed `f64` struct: unbox, add, rebox (Suss today) |
| `b_i64box` | Boxed `i64` struct with overflow check |
| `c_i31` | i31 when the value fits, boxed `i64` otherwise, inline |
| `c_i31_boxedrange` | The same code on values outside the i31 range |
| `e_call_f64only` | `a_f64box` as an out-of-line call |
| `e_poly_*` | Out-of-line `+` that tests the operand types first |

## Running

```sh
cd experiments/numeric-representation
cargo run --release -- copying 50000000 5   # collector: auto, copying, drc or null
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

`c_i31` uses untagged i31 values and treats every i31 as a number. ADR-0003's
proposed encoding tags small longs with the low bit, which adds a parity test
and a shift that this kernel doesn't measure.

- Suss never calls `Config::collector`, so it gets `Collector::Auto`, which in
  wasmtime 49.0.1 selects the copying collector. Copying and null allocate
  inline; DRC calls into the host for every `struct.new`, about 110–150 ns.
- Peak RSS stayed at 15 MiB under copying and DRC. Under null it grew about 16
  bytes per box.
- These kernels keep almost nothing live, which flatters the copying
  collector; a large live heap makes each collection cost more. BigInt, Ratio and
  BigDecimal weren't measured.
