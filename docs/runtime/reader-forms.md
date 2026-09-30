# Portable reader-form boundary

`suss_reader::forms` introduces source syntax for the replacement pipeline.
It is separate from the prototype `Edn` runtime/evaluator representation.
`read_forms` returns `Form { span, metadata, kind }`:

- Spans are byte ranges in the original UTF-8 source, excluding padding. Nested
  forms and metadata retain individual locations. A metadata-bearing target's
  span includes its prefixes; generated quote/deref symbols locate their prefix.
- Every ordinary numeric value is binary64, including rounded integer literals,
  exponent syntax, infinities, NaN and signed floating zero. Decimal, hexadecimal,
  leading-zero octal and radix syntax are supported. The pinned reader spells
  integer `-0` as positive zero; `-0.0` and `-0e0` retain negative zero.
- Strings contain UTF-16 units. Unicode escapes retain lone surrogates; a raw
  astral scalar becomes two units. Character literals become one-unit strings;
  surrogate character escapes and multi-unit character tokens fail explicitly.
- Lists, vectors, sets and maps retain entry order. Maps retain a flat syntax
  sequence until conditional selection checks pairing; they are not runtime maps.
- Metadata prefixes remain separate syntax, in source order. They are not runtime
  metadata and do not add a symbol to a function's argument/declaration sequence.
- Conditional feature/body forms remain a flat ordered syntax sequence; pairing
  waits for prefixes/discards to find their retained targets within that sequence.
  `resolve_conditionals` selects the
  first `:suss`, `:cljs` or `:default` clause where encountered, recursively, and
  skips unselected bodies as syntax without selecting their nested conditionals.
  It removes unmatched forms. Selection happens before map entry pairing and before
  reader prefixes find their retained targets. Metadata applies to the selected
  target, or the next retained target after an unmatched conditional; scalar
  targets and missing targets fail with located diagnostics.

Comments, commas, quote/syntax-quote/unquote/deref/var-quote, sets and discard are
recognized. Quote/deref/var-quote and related syntax prefixes remain `Kind::Prefix`
with operator/target spans until selection lowers them to lists. A discard whose
target depends on conditional selection remains `Kind::Discard` until selection;
ordinary discards are removed while reading. Prefix target lookup stays within
its containing sequence; it never crosses a collection or enclosing conditional
boundary.
Both raw syntax and synthesized prefix nesting are bounded to 64 forms for
ordinary thread-stack safety.
Unsupported dispatch, splicing conditionals, auto-resolved keywords and numeric
precision/ratio forms return located errors. This is a bounded syntax foundation;
it does not promise all upstream reader syntax or numeric spellings.

The implementation is original Rust code. Observable scalar behavior was checked
against the pinned ClojureScript compiler source at
`c4295f303100bbf5afac449242d30bca1126f1a1`, using its tools.reader 1.3.6 dependency.
The development runner executes that upstream dependency under its EPL-1.0
license; no upstream reader/core implementation is copied into the shipped code.
The shared, original 14-case scalar corpus is `tests/oracle/reader-cases.json`.
`scripts/test-reader-oracle.sh` compiles and executes fresh Node observations,
compares float bits and UTF-16 units exactly, then runs the Rust boundary checks.
The JVM and Node remain development-only.

Eleven reader tests check syntax, byte spans, metadata, ordering, rejection and
stack bounds. Two `reader_runtime` tests check the same scalar corpus and transfer
parsed values into the generated ABI v1 runtime, inspect actual GC fields/units
and force collection. The initial surrogate regression failed against the old
EDN path, and the first excessive-nesting test exposed an actual stack overflow;
both now pass with explicit results/diagnostics.

The dispatched PR review reproduced a token-termination regression against the
pinned tools.reader: apostrophes continue symbols and keywords, while an
apostrophe at the start of a form is quote syntax. The reader now distinguishes
these cases and preserves apostrophe as an octal-escape macro boundary. The
regression covers qualified/trailing-prime tokens, quote prefixes and character
rejection, without expanding the currently supported reader syntax.

## Integration still required

The legacy compiler, macro evaluator and component reader still use the old
`parse`/`parse_all` EDN API. No lossless form is converted through that API here:
such a conversion would lose surrogates, source spans or portable numeric meaning.
Next, adapt HIR and the explicit evaluation-order IR to consume these forms and
emit the shared runtime ABI, then switch AOT/REPL/macros through that one pipeline.
Retire the old parser/runtime representation after the replacement meets its
acceptance tests. Namespace file resolution/ambiguity, aliases/refers/phases,
`cljs.core` binding aliasing, full syntax-quote expansion and splicing conditionals
remain M2-01 work. Full source compatibility remains unchanged: 9 differential
passing, 7 exact known failures, 0 skipped. The 14 passing reader observations
must not be counted as passing compiler observations or completed M2 acceptance.
