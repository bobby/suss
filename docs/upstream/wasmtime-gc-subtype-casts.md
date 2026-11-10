# Draft Wasmtime issue: a cast to a non-final type calls the host for subtypes

A draft to file against [bytecodealliance/wasmtime](https://github.com/bytecodealliance/wasmtime/issues)
when Suss is ready to. Recorded by [ADR-0004](../adr/0004-calling-convention.md),
whose calling convention casts every callee to a non-final closure type.
Before filing, re-measure on the current Wasmtime release, and search for an
existing issue (`is_subtype`, `ref.cast`, `br_on_cast`).

---

**Title:** GC: `ref.cast`/`br_on_cast` to a non-final type is a libcall
(`is_subtype`) when the value is a strict subtype

## Summary

A cast to a struct type succeeds inline when the value's type is exactly the
target type, or when the target is final. When the target is non-final and the
value's type is a strict subtype, Cranelift's code calls the `is_subtype`
builtin: about 90 user-space instructions on x86-64.

This is the common case for any object model on WasmGC: a base type for
closures, objects or variants, and one subtype per function, class or
constructor. A call site or method dispatch that casts to the base type then
pays a host call on every execution. On V8 (Node 26.8.2, `--no-liftoff`), the
same call site costs the same whether it casts to a final type or to a
non-final supertype (57 instructions per call in both cases).

## Reproduction

A loop that casts a value of a final subtype to its non-final supertype:

```wat
(module
  (type $base (sub (struct (field $x i32))))
  (type $leaf (sub final $base (struct (field $x i32) (field $y i32))))
  (global $g (mut eqref) (ref.null eq))
  (func (export "run") (param $n i32) (result i32)
    (local $acc i32)
    (global.set $g (struct.new $leaf (i32.const 1) (i32.const 2)))
    (block $done (loop $l
      (br_if $done (i32.eqz (local.get $n)))
      (local.set $n (i32.sub (local.get $n) (i32.const 1)))
      (local.set $acc (i32.add (local.get $acc)
        (struct.get $base $x (ref.cast (ref $base) (global.get $g)))))
      (br $l)))
    (local.get $acc)))
```

`objdump -d` of the precompiled module (`Engine::precompile_module`; in Suss,
`target/release/calls --obj out.cwasm module.wat` in
`experiments/numeric-representation`) shows the cast comparing type IDs
inline, then calling `wasmtime_builtin_is_subtype` when they differ. So the
loop pays the call for a `$leaf`, and not when it allocates a `$base` instead.

Measured with Wasmtime 49.0.1 (`wasm_gc`), default Cranelift settings, on an
i7-1165G7, with `perf stat -e instructions:u` and (count at n − count at 0) / n:
in Suss's calling-convention benchmark, a call site that casts to a final
closure type costs 82 instructions per call, and the same site casting to a
non-final closure type whose value is a subtype costs 172.

The full benchmark is Suss's
[`experiments/numeric-representation/src/calls.wat`](../../experiments/numeric-representation/src/calls.wat)
(kernels `index_final` and `index_subtype`), driven by `calls.sh`.

## Expected

The subtype check compiles inline. Types' supertype chains are known when the
module is compiled, so a fixed-depth supertype display per type (as in V8,
and Cohen's display technique) makes the check a few loads and a compare,
without a host call.
