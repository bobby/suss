# Upstream issue drafts

Engine costs that Suss's design relies on the engine to fix, rather than
routing around ([ADR-0004](../adr/0004-calling-convention.md#principle-use-the-platform-as-designed)).
Each file is a draft issue, with a reproduction and measurements, to file
upstream after re-measuring on the engine's current release.

| Engine | Draft | Status |
| --- | --- | --- |
| Wasmtime | [Reading a function reference from a GC struct calls the host](wasmtime-gc-funcref-reads.md) | Not filed |
| Wasmtime | [A cast to a non-final type calls the host for subtypes](wasmtime-gc-subtype-casts.md) | Not filed |
