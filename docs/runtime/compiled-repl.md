# Compiled native REPL frontend

The native `suss repl` command now owns one `portable_session::Session`, including
one long-lived Store/runtime and incremental fragment instances. It evaluates
each complete input once. Definition sources are not accumulated or replayed,
and printing inspects the already-rooted result instead of compiling a `pr-str`
wrapper around the input. The former source-replay frontend modules remain only
as development regression fixtures, not compiled into the shipped native command.

Interactive input uses the namespace prompt; redirected input prints results
without a banner or prompts. The actual portable reader supplies an explicit
continuation hint for unfinished collections, strings and prefixes. Complete
malformed inputs are reported immediately; incomplete EOF never executes a partial
input. Comment/discard-only input produces no result. Ctrl-C at readline discards
an unfinished input. On Unix, Ctrl-C (SIGINT) while an input executes interrupts
it in either phase (Runtime evaluation or macro expansion), prints
`Error: Interrupted` and returns to the prompt; effects before the interrupt are
not rolled back ([`repl_interrupt.rs`](../../crates/suss-cli/tests/repl_interrupt.rs)).
`suss repl --fuel <N>` sets each input's execution budget (default 10,000,000),
so long computations can run until interrupted. Pending I/O cancellation awaits
the design section 9 scheduler.

The native command now uses `Session::new_repl()`, loading the provenance-tracked
core artifact once per Store generation. [Atom storage](atoms.md) persists across
inputs; complete core compatibility remains unfinished.

Namespace commands `:load`, `:reload`, `:reload-all` and `:in-ns` accept one
namespace name; see [loading/reload policy](namespace-session.md).

`:quit` exits. `:reset` replaces the Store/runtime using the existing Session reset
contract. Language exceptions and source failures leave the REPL usable. A failed
initializer preserves its previous binding, while preceding arbitrary effects
remain. Captured function values retain their old behavior after global rebinding.

Display currently supports nil/booleans, binary64 numbers, UTF-16 strings and
descriptive function/type labels. Finite numbers use the shared runtime formatter;
lone surrogates are escaped without replacement. Unsupported objects produce a
display error after successful evaluation; no fabricated collection/string value
is substituted. This is a bounded native frontend, not full portable printing or
an independent compatibility decoder. Tests assert actual process outputs and
use the separate existing native artifact tests for runtime ownership/layout.

Evidence: `cargo test -p suss-cli --locked --test persistent_repl -- --test-threads=2`
passes eight executing command tests, including persistent atom state and old
captured contents. The original persistence regression failed
against the legacy frontend with its explicit unsupported global `set!` diagnostic.
The first multiline probe failed with only `nil/0/0/42` outputs instead of the
ordered complete-input results; reader continuation then fixed that behavior.
An initial defonce output expectation was corrected to nil on skipped initialization,
consistent with the pinned macro; the preserved binding/effect checks were unchanged.
The `(array)` display probe observed the native-object unsupported diagnostic;
it did not establish an actual empty-reference-array rendering failure. The
renderer also checks the actual array storage type before reading UTF-16 units.

Issue #12 stays open: atom persistence remains unimplemented in this frontend.
Namespace `require`/`in-ns` commands, reload/cache policy (#13), isolated compiled
macros/bootstrap (#14), executing-code and I/O cancellation and live GC accounting
(#15), tab completion, full printer/core support, other command/AOT frontend
migration and non-native component delivery remain separate work. No M3 issue
or milestone is complete from these tests. Implementation is original Rust code;
the pinned upstream defonce form informs semantics without being copied here.
