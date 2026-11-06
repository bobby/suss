# Compiled native REPL frontend

The native `suss repl` command now owns one `portable_session::Session`, including
one long-lived Store/runtime and incremental fragment instances. It evaluates
each complete input once. Definition sources are not accumulated or replayed,
and display calls core's `pr-str` on the already-rooted result instead of
compiling a `pr-str` wrapper around the input (see [Display](#display)). The former source-replay frontend modules remain only
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
`suss repl --fuel <N>` sets both the program-evaluation and compilation operation
allowances. Both defaults remain 10,000,000. `--compile-fuel <N>` overrides the
compilation allowance independently of argument order:

```sh
suss repl --compile-fuel 10000000000
```

This keeps evaluation at its default while permitting more accounted compilation
and macro work. Input preparation consumes compilation fuel even when no macro
is invoked. These cumulative operation budgets use the existing host/guest
charges, including allocation work. Engine JIT time is outside that accounting;
fuel does not bound retained process memory. Increasing an allowance permits more
work and does not accelerate compilation. A successful
`:reset` preserves each phase's selected allowance; reset initialization must
also fit those allowances.

Pending I/O cancellation awaits the design section 9 scheduler.

The native command now uses `Session::new_repl()`, loading the provenance-tracked
core artifact once per Store generation. [Atom storage](atoms.md) persists across
inputs; complete core compatibility remains unfinished.

Namespace commands `:load`, `:reload`, `:reload-all` and `:in-ns` accept one
namespace name; see [loading/reload policy](namespace-session.md).

`:quit` exits. `:reset` replaces the Store/runtime using the existing Session reset
contract. Language exceptions and source failures leave the REPL usable. A failed
initializer preserves its previous binding, while preceding arbitrary effects
remain. Captured function values retain their old behavior after global rebinding.

## Display

The REPL displays a result as Clojure's REPL prints one (#288):

- A value prints as core's `pr-str` prints it, so collections, seqs, records,
  types, arrays and references print as in [printing](printing.md), and
  `print-method` methods apply. Display calls the session's compiled `pr-str`
  once per result; it doesn't compile anything. nil, booleans, numbers and
  strings are formatted by the host, with the same text, so a session without
  core can still display them.
- A future or stream prints its state, such as `#<future pending>`, alone or
  inside a collection. Printing never awaits it.
- An input whose value is a `def`'s, such as `(def x 1)`, `defn`, `declare`,
  `defmacro` or `(let [] (def z 1))`, displays the var, `#'user/x`, without
  printing or realizing the value. `defmulti` displays its var when it defines
  the multimethod and nil when the multimethod already exists, as in Clojure.
  `defprotocol` displays the protocol's name, and `deftype` and `defrecord`
  the type.
- A thrown error with a message reports the message, such as `Error: Uncaught
  language exception: boom` for `(throw (ex-info "boom" {}))`. Any other thrown
  value is printed, such as `"s"` for `(throw "s")`.

Differences from Clojure remain:

- Suss has no var objects. After `(declare d)` or a failed `def`, `d`
  displays `#<undefined>` and prints as nil inside a collection, where Clojure
  prints an `Unbound` object. Calling it throws "Not callable", where Clojure
  throws "Attempting to call unbound fn: #'user/d".
- Only a `def` reached through `do`, `let` and `binding` bodies, or one branch
  of an `if` whose other branch is nil, displays as a var. `(when c (def x
  nil))` displays nil when it runs, where Clojure displays the var.
- `(type 1)` displays `#object[Number]`, since host types have no names yet
  ([type names](type-names.md)).
- An infinite seq prints until interrupted with Ctrl-C, as in Clojure without
  `*print-length*`.

Evidence: `cargo test -p suss-cli --locked --test repl_value_display` drives
`suss repl` through piped input. Each expected value was compared with Clojure
1.12.6's REPL, apart from the differences above.

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
