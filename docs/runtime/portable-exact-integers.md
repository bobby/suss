# Exact WIT integer adapter prerequisite

The accepted design requires separate signed and unsigned wrapper values for
WIT `s64`/`u64`. Ordinary Suss numbers retain binary64 semantics. This preparation
adds freestanding exported scalar `s64`/`u64` boundaries and resolved aliases,
including the existing asynchronous export bridge. Nested exact integers and the
full generic WIT graph remain explicitly unsupported development features.
This does not complete M3, M5 or a release gate.

## Source API and storage

Canonical `suss.core/wit-s64` and `suss.core/wit-u64` accept decimal strings:

```clojure
(suss.core/wit-s64 "-9223372036854775808")
(suss.core/wit-u64 "18446744073709551615")
(suss.core/wit-to-number (suss.core/wit-s64 "-9007199254740991"))
```

`wit-s64?` and `wit-u64?` distinguish the nominal `WitSigned64` and
`WitUnsigned64` types. They are separate from `number?`. Each instance stores
high and low unsigned 32-bit words through the existing generic nominal object
layout. Neither construction nor WIT transfer converts the complete integer to
binary64. The existing ten-type shared GC group and ABI2 remain unchanged.

Decimal parsing accepts ASCII digits, leading zeros and a leading minus for
signed input. Empty strings, plus signs, unsigned negative values, whitespace,
fractional/scientific notation, non-ASCII digits and range overflow raise language
errors. Signed negative zero constructs zero. Arithmetic updates bounded words
and carries exactly. `wit-to-number` validates nominal identity and both words,
and accepts only the inclusive range `[-9007199254740991,9007199254740991]`.

The private generated adapter captures the first core's nominal descriptors and
word validator before user initializers. WIT input creates that nominal wrapper;
output checks the selected signedness and finite integral words in
`[0,4294967295]` before reconstructing the `i64` bits. Public class or validator
redefinition cannot bypass this boundary validation. Generic constructor misuse
can create invalid objects; returning them produces a language boundary error.

The exact schema has its own rooted closure alongside option/list schemas. Bare
exact integers use direct `i64` canonical signatures without transfer memory.
Mixed string/list arguments retain their existing allocation/cleanup behavior.
The helpers are original Suss code in a separately hashed `loader.original` foundation source, with
recorded loader hash and strict generated core import. No upstream declaration
or license is reclassified by these additions.

## Executing evidence and remaining validation

The nine `portable_aot_exact_integers` tests instantiate actual components with
Wasmtime, decode typed Rust values, bound fuel/memory, require zero hidden imports,
assert required memory layouts and explicitly decode language failures. They
exercise both endpoints, 128 varied bit patterns, values beyond the safe-number
range, atom retention after GC, decimal constructors, nominal predicates, checked
conversion, malformed words, aliases, public redefinition, async results and
mixed exact/option/list roots and flattened parameters.

The final byte-identical parent fixture at reviewed330e7507 failed all9 tests
(86760 terminal101,3.20s). Both Runtime/Macro image pairs regenerated successfully
(46559 terminal0). The final focused run50296 passed all74 tests across7groups:
exact9 plus existing scalar13/async3/options16/strings11/list10/publicpipeline12,
with zero failures, ignores or filtered tests. Expanded malformed cases include
NaN/infinity/boolean word storage, non-string constructors and trailing whitespace.

Java/Node-free verifier82713 terminated0: both phase Wasm/JSON pairs reproduced
byte-exact between two fresh processes and tracked assets, compiler identity checks
passed and all4 bootstrap regressions passed. Full178 Python checks,21 licensed
sequence setup forms and strict277-file generated core reproduction also pass.
The licensed sequence initializer remains byte-identical to the parent. Original
helpers have their own hashed `loader.original` stage after before/forms/after;
a regression rejects unknown stages and stale hashes and checks fixed ordering
independent of JSON key order.

Recorded earlier failures remain real: generator88357 rejected a mid-fragment
namespace directive; Python177 initially reported1failure/1error when original
helpers were appended to the licensed sequence initializer. Both were repaired
without weakening assertions. Logs are retained at `/private/tmp/suss-exact-final-
{parent,bootstrap,focus,verify}.log` and `/private/tmp/suss-exact-python-
{checkpoint,packaging}.log`. These focused results establish the described bounded
export prerequisite. Independent review and significant fixes, the unfiltered
workspace baseline and exact final-head CI remain required before merge readiness.

Use `CARGO_TARGET_DIR=/private/tmp/suss-m3-pr143-target CARGO_BUILD_JOBS=2` and
coordinate the sole heavy process slot. Focus with `cargo test -p suss-compile
--locked --test portable_aot_exact_integers -- --test-threads=2`; regenerate with
`cargo run --profile test -p suss-cli --bin suss-bootstrap --locked --
runtime/bootstrap`; verify with `sh scripts/verify-bootstrap.sh`. Full validation
uses `cargo test --workspace --locked -- --test-threads=2`.

Next integrate validated prerequisites into the held public file/project
compiler migration and execute all original typed component fixtures. Public
expression/component production evaluator retirement, complete portable source
schemas/inference and rooted pending-I/O/live-GC lifecycle acceptance remain M3
requirements. Refs #12, #13, #14, #15; no issue closing claim.
