# Draft Wasmtime issue: reading a function reference from a GC struct calls the host

A draft to file against [bytecodealliance/wasmtime](https://github.com/bytecodealliance/wasmtime/issues)
when Suss is ready to. Recorded by [ADR-0004](../adr/0004-calling-convention.md),
whose calling convention reads one on every call to an unknown closure.
Before filing, re-measure on the current Wasmtime release, and search for an
existing issue (`get_interned_func_ref`, `intern_func_ref_for_gc_heap`).

---

**Title:** GC: `struct.get` of a function reference is a libcall
(`get_interned_func_ref`) on every read

## Summary

With the GC proposal, a function reference stored in a struct field is kept in
the GC heap as a 32-bit ID into a side table. Reading it back with
`struct.get` loads the ID and calls the `get_interned_func_ref` builtin, so
every read costs a host call: about 120–200 user-space instructions on x86-64,
against a few for a plain load.

This puts the standard way to implement closures and virtual dispatch on WasmGC
behind a libcall: a struct holding typed function references, called with
`call_ref`. Languages that compile closures or vtables this way (Clojure-like
languages, Dart, Kotlin, OCaml, Scheme) pay it on every dynamic call. V8 reads
the same field inline.

## Reproduction

A loop that reads a typed function reference from a struct and calls it:

```wat
(module
  (rec
    (type $f (func (param (ref $code) i32) (result i32)))
    (type $code (struct (field $entry (ref $f)))))
  (elem declare func $inc)
  (func $inc (type $f) (param (ref $code)) (param $x i32) (result i32)
    (i32.add (local.get $x) (i32.const 1)))
  (global $c (mut (ref null $code)) (ref.null $code))
  (func (export "run") (param $n i32) (result i32)
    (local $acc i32) (local $c (ref $code))
    (global.set $c (struct.new $code (ref.func $inc)))
    (local.set $c (ref.as_non_null (global.get $c)))
    (block $done (loop $l
      (br_if $done (i32.eqz (local.get $n)))
      (local.set $n (i32.sub (local.get $n) (i32.const 1)))
      (local.set $acc (call_ref $f (local.get $c) (local.get $acc)
        (struct.get $code $entry (local.get $c))))
      (br $l)))
    (local.get $acc)))
```

`objdump -d` of the precompiled module (`Engine::precompile_module`; in Suss,
`target/release/calls --obj out.cwasm module.wat` in
`experiments/numeric-representation`) shows a call to
`wasmtime_builtin_get_interned_func_ref` in the loop. Untyped
`funcref` fields behave the same.

Measured with Wasmtime 49.0.1 (`wasm_gc`, `wasm_function_references`), default
Cranelift settings, on an i7-1165G7, with `perf stat -e instructions:u` and
(count at n − count at 0) / n, less an inline loop:

| Call path | Instructions per call |
| --- | --- |
| Static `call` | 9 |
| `call_indirect` by an `i32` index read from the struct | 53 |
| `call_ref` of a typed reference read from the struct | 205 |
| `call_ref` of an untyped `funcref` read from the struct, then cast | 173 |

On V8 (Node 26.8.2, `--no-liftoff`), the typed field read and `call_ref` cost
32 instructions.

The full benchmark is Suss's
[`experiments/numeric-representation/src/calls.wat`](../../experiments/numeric-representation/src/calls.wat)
(kernels `field_typed`, `field_untyped`, `proposed`), driven by `calls.sh`.

## Expected

A read of an immutable function-reference field compiles to inline loads, as
reads of other reference fields do, or at worst to an inline lookup of the
side table, without leaving compiled code.

## Notes

- Writes, which call `intern_func_ref_for_gc_heap`, matter less for this use:
  closure and vtable structs are written once and read on every call.
- An inline fast path for the side table lookup (a bounds check and a load from
  the slab) would likely remove most of the cost, if the slab's base can be
  reached from the `vmctx`.
