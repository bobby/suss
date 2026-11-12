# Call floor

This experiment measures what the WasmGC operations behind a Suss closure call cost in wasmtime. The question is whether the out-of-line host calls that dominate Suss's protocol calls come from the platform or from the calling convention.

It isn't part of the Suss workspace or its tests.

## Why

After the M15 call work (#392–#394), profiles of a protocol call, `(ident b)` at about 1,100 instructions, attribute about 60% of its time to two wasmtime host calls:

- **`get_interned_func_ref`.** It runs when a function reference is read from a GC struct field. ADR-0004 keeps each closure's per-arity entries as function references in its code struct, so every closure call does this.
- **`is_subtype`.** It runs when a value is tested or cast against a non-final type it doesn't match exactly. Every compiled closure is a subtype of the non-final `$closure`, so calls, `closure-environment` and the method paths pay this on each cast.

## What it measures

`src/k.wat` has one loop per operation. Each kernel returns its count, so nothing is dropped. `src/main.rs` reports the best of 5 runs and subtracts the control loop.

| Kernel | Operation per iteration |
| --- | --- |
| `control` | A global read and a null test. |
| `cast_final` | `ref.test` of a final struct type against a value of that type. |
| `cast_super` | `ref.test` of a non-final supertype against a value of a subtype: Suss's `$closure` casts. |
| `callref_global` | `call_ref` through a function reference in an immutable global. |
| `callref_struct` | `call_ref` through a function reference read from a GC struct field: Suss's code struct entries. |
| `call_indirect` | `call_indirect` through a funcref table, with its signature check. |

## Running

```sh
cd experiments/call-floor
cargo run --release -- 50000000
```

To measure another wasmtime, change the `wasmtime` pin in `Cargo.toml`. Don't commit `Cargo.lock`.

## Results

Apple M3 Pro, nanoseconds per operation less the control. Raw output is in [results/run_m3pro.txt](results/run_m3pro.txt).

| Kernel | wasmtime 49.0.1 | 50.0.1 | 51.0.0-rc.1 |
| --- | --- | --- | --- |
| `cast_final` | 0.00 | 0.00 | 0.00 |
| `cast_super` | 3.27 | 3.59 | 3.50 |
| `callref_global` | 0.79 | 0.82 | 0.80 |
| `callref_struct` | 8.41 | 8.45 | 11.22 |
| `call_indirect` | 0.82 | 0.91 | 0.95 |

## Conclusions

- **Upgrading wasmtime doesn't help.** Neither 50 nor 51 inlines either check, and 51 makes the struct-held function reference slower.
- **Function references read from GC objects cost about 10× a table call.** `call_indirect` costs the same as `call_ref` through a global. A code struct that held table indices instead of function references would remove every closure call's `get_interned_func_ref`.
- **Casts to a non-final supertype cost about 3.3 ns each. Casts to a final type are free.** A closure representation whose calls cast to a final type, such as one final closure struct with its captures in a per-body environment, would remove the `is_subtype` calls.

Both points are changes to ADR-0004's calling convention, which #378 (and an ADR revising ADR-0004) would take up. The HAMT floor ([../hamt-floor](../hamt-floor)) agrees: its `call_ref` variant reads its function references from immutable globals, and costs no more than direct calls.
