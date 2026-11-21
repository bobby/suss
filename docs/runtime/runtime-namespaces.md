# Runtime namespaces

Running programs see namespaces as Clojure's do (#309). Before this, only the
compiler's analyzer knew namespaces. `read-string` (#300) needs them to resolve
`::k` and to expand syntax quote at read time.

## Behaviour

`*ns*` is a dynamic var holding a `suss.core.Namespace`. It follows Clojure:

| Where code runs | `*ns*` |
| --- | --- |
| A namespace's top-level forms, in a source file, the REPL or an AOT component | that namespace, bound while they load, as Clojure's `load` binds it |
| A macro body during expansion | the namespace being compiled |
| The REPL | the REPL's namespace, following `ns` and `:in-ns`; `user` at start |
| A script file before its `ns` form | `user` |
| Anything else, such as an AOT component's export called by the host | the root value, `suss.core`, as Clojure's root is `clojure.core` |

`binding` rebinds it, and a function called later sees its caller's `*ns*`.
The runtime always has the `suss.core` and `user` namespaces. Each phase has its
own namespaces, as it has its own core.

| Function | Behaviour |
| --- | --- |
| `find-ns` | the namespace named by a symbol, or nil |
| `the-ns` | a namespace, or the namespace a symbol names; otherwise throws `ex-info` "No namespace: x found" |
| `ns-name` | the namespace's symbol |
| `all-ns` | every namespace loaded in this phase |
| `ns-aliases` | the aliases a namespace declares, each mapped to its namespace |
| `ns-refers` | each referred name, mapped to the fully qualified symbol it refers to |

A namespace prints as `#object[suss.core.Namespace 0x1f "user"]`, Clojure's
shape with Suss's identity hash. Its `str` is its name, and two namespaces are
equal only if identical.

### Differences from Clojure

- **No Vars.** Suss has no runtime `Var` objects, so `ns-refers` maps names to
  qualified symbols instead of Vars.
- **Not implemented here:** `intern`, `ns-publics`, `ns-interns`, `ns-resolve`,
  `resolve`, `remove-ns` and `create-ns`.
- **`in-ns`** is not a function. The REPL's `:in-ns` command switches namespaces.
- **Printed identity hash.** The hash in a namespace's printed form is Suss's
  identity hash, not the JVM's.

## Symbol resolution

A private core function, `reader-resolve-symbol`, resolves a symbol against
`*ns*` exactly as the compiler's syntax quote does (`syntax_quote.rs`
`resolve`):

1. A name containing `.` is left as is.
2. A qualified symbol has its namespace replaced through the namespace's aliases,
   then its macro aliases.
3. An unqualified symbol resolves through the first of these that applies:
   - its refers;
   - its own declarations;
   - core's declarations, unless the name is excluded;
   - its macro refers;
   - core's bootstrap macros and the pinned reader-name catalog, unless excluded;
   - otherwise the current namespace.

`suss.core` and `clojure.core` are written as `cljs.core`, as the compiler writes
them.

A corpus test (`runtime_resolution_agrees_with_syntax_quote`) compares this
function with the compiler's syntax quote, in both phases. It covers refers,
renamed refers, aliases, macro aliases and refers, excluded core names, own
declarations, core declarations, bootstrap macros, class-like names, and
qualified and `js/` symbols. It also covers a redefinition and a reloaded `ns`
form.

## How the compiler publishes namespaces

Core holds a registry of `Namespace` objects (`docs/compatibility/bootstrap/
exact-integers.sus`, generated into `core.sus`). The compiler keeps it current
from `hir/namespaces.rs`, without a host import:

- **Which fragments publish.** A compiled fragment publishes when its namespace
  is new to the runtime, when its scope changed, or when it declared a name
  that changes resolution.
  - What it publishes: aliases, refers, macro aliases and refers, excluded core
    names, and declarations.
  - Declarations matter only where a name would otherwise resolve elsewhere: to
    core, a bootstrap macro, the reader catalog or a macro refer. Other
    namespaces publish only those, so an ordinary `def` publishes nothing. Core
    publishes all of its own.
  - When the scope changed, the list replaces what the runtime had, including
    bindings the compiler provided before the namespace's first fragment.
    Otherwise it adds to it.
  - Whether the runtime has the namespace at all is decided when a fragment
    runs, not when it compiles. A fragment whose namespace cell is unbound
    publishes in full. An earlier fragment may have compiled and never run,
    for example when a dependency failed to load. Core's bootstrap always
    publishes `suss.core` first, so a later core fragment, such as a core
    macro compiled from source, only adds its own declarations and does not
    carry core's full lists.
  - AOT source preparation initializes every prepared Runtime fragment in
    order, aborting initialization on an exception. In this mode a namespace's
    first fragment publishes its full scope, and subsequent fragments publish
    only changed scopes or newly relevant declarations. They omit the unbound
    recovery branch and its duplicate scope tables. Independently executed
    session fragments retain that branch, including recovery after a dependency
    failed before a prepared fragment could run.
  - Ordered AOT assembly runs all source initializers under one `*ns*` binding
    in a small shared loader module. Each fragment selects its namespace by
    assigning that binding after publication, rather than constructing a thunk
    and a new binding frame. The existing `dynamic-invoke` helper restores the
    original binding on both normal return and a language exception. Core runs
    first; schema factories are captured before user initializers. Component,
    official command and async assembly use the same loader plan.
  - The scope travels as an array in slot order, with nil for an empty part,
    and a long name list as an array of bucket strings. A quoted map would
    cost far more to build than the publication itself, and a vector longer
    than 32 needs core's privileged constructors, which core evaluated as
    ordinary source lacks.
  - The frame's forms carry the fragment's own source extent, so a fragment's
    IR, and its cache key, do not change when another form in its file
    changes.
- **Where the object lives.** The namespace object is held in an internal cell,
  `suss.internal.namespaces/<ns>`. Each independent session fragment runs inside
  `(binding [*ns* <cell>] ...)`, so a fragment that publishes nothing costs a
  check that the cell is bound, a cell read and a binding, about 700 fuel.
  Ordered AOT fragments instead read this cell and update the loader's shared
  binding. Prepared AOT source fragments must be initialized as an ordered batch
  through the AOT assemblers, which provide that scope.
  This execution requirement is recorded in a versioned custom section covered
  by the artifact's integrity hash. Assembly rejects malformed or duplicate
  markers. Returned AOT and analyzer catalogs clear the transient preparation
  policy: reusing either catalog with `prepare_fragment` or `compile_in` prepares
  an independent frame, including recovery when the original AOT source never
  ran. Catalog facts do not impose an execution scope on later inputs.
- **Macros.** The macro host sets `*ns*`'s root in the macro store to the
  namespace being compiled before it calls a macro. It interns namespace objects
  once and caches them; `suss.core` and `user` are interned when the macro
  session starts, outside any request budget.
- **Name lists.**
  - A list of up to 32 names travels as one space-separated string and becomes
    a set.
  - A longer list travels as 61 bucket strings, `" a b c "`, each name placed by
    a hash of its length and its first and last code units. A lookup runs the
    runtime's string search in one bucket. Character-by-character work in Suss
    costs hundreds of fuel per unit, and the string search costs about 35.
  - Refers are keyed by name string, which is cheaper to look up than a symbol.
- **Core's own lists.** Core's declarations, its bootstrap macros and the reader
  catalog are large, and only resolution reads them. So core's bootstrap
  defers publishing them until the first resolution or the next publication
  to `suss.core`, whichever comes first, so names added to core later are kept.
  Publishing other namespaces does not run it, so a program that never resolves
  a symbol at runtime never pays for it. The
  bootstrap-macro and
  reader-name lists omit names core declares, because the declarations are
  checked first and give the same answer.

Bootstrap, cached and AOT-compiled modules restore the same namespace data.
The ordered AOT preparation policy is separate from direct linking; analyzer
preparation uses it too, while Macro sessions retain independent execution.

## Costs

Measured on 2026-10-31 against `main` (00f700d), test profile. Fuel figures
are for a REPL session (`Session::new_repl`).

| | `main` | With runtime namespaces |
| --- | --- | --- |
| Bootstrap module (`runtime.wasm`) | 2,205,349 B | 2,284,678 B (+79 KB, +3.6%) |
| AOT component, one small namespace | 2,482,496 B | 2,571,207 B (+89 KB, +3.6%) |
| AOT component, public records fixture | 2,632,502 B | 2,727,964 B (+95 KB, +3.6%) |
| Core initialization fuel | 1,662,312 | 1,728,197 (+4%) |
| `suss run` of a component (median, interleaved runs) | about 70 ms | about 3 ms more (noisy) |

| Per operation | `main` | With runtime namespaces |
| --- | --- | --- |
| Evaluate `x` | 15 | 708 |
| `(def y 2)` | 28 | 722 |
| A namespace's first fragment (publishes its scope) | — | about 40,000, once per namespace |
| First resolution in a session (core's deferred publication) | — | 200,000–250,000, once |
| Each later resolution of an unqualified symbol | — | 85,000–130,000 |

The fixed cost per fragment, about 700 fuel, is the check that the namespace is published and the `*ns*` binding.

### Known limitation

Suppose a namespace defines a name that core does not declare, so the
declaration is not published, and core later declares the same name, through a
definition in `suss.core` at the REPL. The runtime would then resolve the name
to `cljs.core` where the compiler resolves it to the namespace's own definition.

## Long string literals

Without one prerequisite repair, the published name lists would have added about
300 KB to the bootstrap. The compiler used to emit a string literal longer than
64 code units as one `array.set` per unit, about 10 bytes of code per character.
It now emits such literals from passive data segments with `array.new_data`,
deduplicated per module. The runtime ABI version and the shared GC types are
unchanged.

Each distinct literal is now one global of its module (#442), so evaluating a
literal reads that global instead of allocating a string. Core's field reads
and method calls pass their names as literals, so before this every
`(.-arr m)` allocated its name. A literal of 64 units or fewer is an immutable
global whose constant initializer is `array.new_fixed`. A longer one is a
mutable global, filled from its data segment by `array.new_data` on its first
evaluation. Evaluations of one literal share its string, so
`(identical? s s)` holds for two evaluations of the same literal. Strings are
written only while they're built (`string-new` and `string-set-unit`, in the
host adapters), never afterwards.
