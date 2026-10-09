# Full str / StringBuffer source prerequisite

This author checkpoint imports the whole pinned public `str` declaration:
its documentation, zero/one/variadic arities, live `str`/`first`/`next` dispatch,
loop and ordered once-only conversions remain. Macro `nil?` expands through the
immutable bootstrap predicate. Boxed Number/String/Boolean `.toString` uses
checked scalar conversion; other objects retain actual method lookup and its
raw result. The Closure constructor spelling targets an owned portable adapter.

`stringbuffer.js` is the complete source from the exact transitive Closure jar
identified in `source.json`; its Apache license is retained. `adapter.sus`
retains every constructor branch and all five methods. First append conversion
uses the string hint, later append and set use default-hint addition; a nil or
undefined second append argument suppresses all remaining arguments. The old
left buffer value is captured before conversion, preserving conversion mutation
and throw ordering. Constructor nil-first suppression, zero append's undefined
conversion, self return, clear/set undefined returns and UTF16 length remain.
The internal `to-array` callable is captured before public rebinding.

Native owned storage replaces Closure's inherited initial buffer slot. General
Closure/JavaScript prototype interoperability is not claimed. Full arbitrary
Unicode field/schema support, exotic conversion and exact TypeError/prototype
coverage remain broader dependencies; the existing foreign-schema guards remain.
This checkpoint does not implement reify/nil-iter or close original #18/#19.

Compiler changes preserve original declarations and source field facts, while
schema strings and Object lexical reads use the same canonical munged spelling.
Names colliding after munging alias the last slot for protocol reads/writes and
named reads/writes. Constructor arguments still evaluate once in source order;
private payload normalization gives duplicate slots the last supplied value.
The schema lookup selects the last match only for stride-one field tables;
stride-two method-table lookup retains its existing first-match policy.
Full schema validation remains; constructor normalization adds quadratic pair
comparison work in the number of fields and needs actual native cost validation.

Object callbacks retain genuine fixed/rest signatures, resolve the actual
IndexedSeq rest class, and select general dispatch for a single variadic method.
The receiver remains physical `this`, outside method-head recurrence. Fresh pin
probes include fixed/rest overlap as well as standalone variadic callbacks.

The closed 29-case corpus independently compares tagged raw strings, binary64
bits, nil/Boolean results and nested ordered effect vectors. Fresh pinned
compile/Node/compare passed; three fail-closed harness tests and all 278 Python
tests passed. The two direct macro cases also have fresh pinned raw matches, with native macro
implementation still pending. The independent constructor/named-lookup ABI test
is authored unexecuted. Native tests (both phases, post-GC raw corpus, sole-host callable
retaining buffer and captured vector) are authored **uncompiled/unexecuted**.
The older named-property rejection test now executes the supported reads/writes,
with raw undefined decoding and protocol recovery; unsupported extra-property
writes and all separate prototype-boundary assertions remain.

The initial two direct `(str object)` expectations mismatched. Both inputs and
observations are retained. This is pinned macro optimization, not transport
normalization: the complete `str` macro emits a string-context addition around
its one-arity call. Calling the public function through a lexical alias returns
raw Number23/nil as its complete runtime source specifies. The revised cases
retain those original raw expectations and exercise that alias. `str-macros.cljc`
retains both complete macro witnesses and their three immediate source helpers, and compiled cases are retained. A live
single-arity replacement also caused a pinned arity-property TypeError; its log
is retained, and the live test now saves the genuine variadic delegate and uses
a replacement with the matching fixed/variadic shape. No failed run is a pass.

Macro inventory rows `macro:str:877` and `macro:str_:852` are explicitly
unimplemented, with source witnesses and analyzer/js* dependencies. Their full
compiled graph and direct-call/alias semantic correspondence remain required.

No Cargo, bootstrap generation or native execution ran in this author checkout.
All four bootstrap artifacts remain unchanged and stale for this source identity.
After independent source review and coordination, regenerate through the normal
Java/Node-free path, execute portable_string_fields, portable_named_properties,
portable_record_iteration and the entire 84-row portable_record_graph_runtime,
then relevant compiler/ABI/lifecycle gates, reproducibility and the unchanged
locked workspace baseline. Final-head CI remains required.

`tests/oracle/string-direct-macro-cases.json` separately preserves the two direct
syntax obligations with their actual pinned string expectations ("23"/"null").
They are pending genuine macro implementation and are not folded into the
29-case runtime-function acceptance claim. The initial corpus remains unchanged;
its incorrect expectations are diagnostics, never passes.

Compiled JavaScript witnesses are stored as deterministic gzip files, preserving
every original byte (including compiler trailing whitespace). Uncompressed
hashes and lengths are recorded in evidence/files.json; no normalization occurred.
