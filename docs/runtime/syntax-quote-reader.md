# Syntax quote reader prerequisite

The working M3 branch now contains a bounded Rust reader transformation adapted
from tools.reader `reader.clj598–753` at the accepted ClojureScript pin
`c4295f303100bbf5afac449242d30bca1126f1a1`. The source file SHA256 is
`614e857a4d92e22edbaf38e4e87c3f499e8703fb7f930c23e1df136535cc4597`.
The adapted file retains its EPL-1.0 copyright and notice; the license text is
retained in `runtime/core-import/epl-v10.html`.

`portable::syntax_quote::ReaderState` stages its counter and commits it only
after successful expansion. Nested templates are processed inside out with
separate gensym maps. Repeated auto-gensyms share identity inside one template;
different templates and subsequent successful inputs receive different IDs.
Numeric IDs are partitioned by phase to avoid collisions between independently
initialized Runtime and Macro reader states. The counter lives in the staged
analysis Environment; failed compilation does not publish its changes.
The pass walks quoted data too, uses the supplied phase namespace catalog,
handles unquote/splicing, emits the pinned collection constructors and retains
metadata before backquote separately from metadata on the template value.
The map constructor threshold is sixteen map entries, not sixteen flattened
elements. Traversal depth, work and text have explicit bounds.

The pinned reader probe records twelve actual analyzer-reader trees, including
the original five macro definitions, scalars, empty list, vector splicing, map,
set, metadata and nested templates. The comparison preserves tree structure,
symbol qualification and generated identity; only generated numeric IDs and
JSON integer versus portable binary64 representation are normalized. Raw primary
observations remain unchanged. Explicit metadata entries use the previously
accepted textual order; the metadata fixture compares that declared variance
against the pinned reader's incidental order. No effectful map/set entry order
is normalized away.

Focused tests cover aliases, ordinary and macro referrals, exclusions, phase
separation, local namespace definitions, transactional failure, counter overflow,
quoted templates, ordinary calls named syntax-quote, metadata before backquote,
sixteen-entry maps, signed-zero bits, lone UTF16 surrogates, textual collection
effect order, and oversized input rejection. The complete compiler library run
passes 65 tests with zero failures or ignores, including ten new reader tests.

The pass is now connected at the top-level analysis boundary. A generated source
catalog includes core declarations, protocol methods and type constructors as
namespace facts; it does not claim these names are executable. Private immutable
correspondence maps lowered code back to actual indexing-reader data for `&form`.
Synthetic helper bodies gain no fabricated original positions. The original
source metadata snapshot remains unchanged and metadata/code children are paired
independently. Counter replay verifies the correspondence uses the same generated
IDs as the executable pass.

Executable reader coercion adapts the exact one-argument `sequence` body at
core.cljs4403–4406 as a private closure in a once-initialized live cell.
That cell shares the canonical identity of a later public source definition,
which promotes the binding. Ordinary source resolution cannot access the
fallback; existing and newly compiled reader calls observe later definitions.
Public `sequence` is not registered:
its full declaration hash is
`748c1e1a78ed4ddfac684f99f973501ceb9366e7fba1cb9074726d27759f8756`,
and its transducer arities remain unfinished. Literal quote payloads retain their
actual reader constructor symbols rather than private helper representations.
The complete retained `seq?`, `vec`, `into-array`, `array-map` and `hash-map`
declarations supply the selected dependencies. Missing-value constructor messages
still depend on unfinished `str_` printing and are not certified.

The three previously failing compiled syntax-quote regressions now pass. A fresh
pinned ClojureScript/Node fixture executes the same six macro definitions and
runtime source as Suss: all eighteen scalar observations match after GC in both
caller phases. These include once-only effects, splicing, nested macro invocation,
quoted data, metadata and the actual constructor observed through `&form`.
Automatic parent positions with Unicode/CRLF and persistent reader state across
successful/failed inputs have separate native regressions. All eleven compiled
syntax-quote/transport tests and the affected suites pass: 37 tests total.
Python validation passes 118 tests. Fifty lazy/constructor observations also
match fresh pinned execution; the original twenty-seven remain unchanged.

The preliminary workspace baseline passed1054/0 with17 existing ignores, before
the live-cell repair. The separate guard then reproduced ignored sequence
redefinition. Coercion selection now occurs in live HIR resolution, and that
guard passes in both phases. The old-closure guard now passes after shared-cell
handling replaced literal fallback closures. Five live-cell regressions pass in both caller phases, including privacy,
no replay, failed compile/initializer recovery and reset. The final workspace
baseline is running.
Current-head rechecks pass65 compiler tests and11 syntaxquote/transport tests.
No new PR, final current-head full
baseline, independent review or final-head CI is claimed.
Bootstrap/cache/evaluator-removal/lifecycle and complete portable schema gates
remain open, along with issues #12–#15.
