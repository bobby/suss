# Native project compilation through compiled phases

Native `suss compile --config deps.sus` now uses the portable source/AOT pipeline.
Project compilation creates an isolated compiled Macro session for each selected
world and defers Runtime initialization to component instantiation. It does not
call the prototype compiler or its tree-walking macro evaluator.

A world may configure its entry namespace, selected WIT world and exact exported
paths explicitly:

```clojure
{:src-paths ["src"]
 :worlds
 {:library {:namespace app.core
            :wit "api.wit"
            :wit-world "chosen"
            :exports {"api#calculate" app.core/calculate}
            :output "library.wasm"}}}
```

Paths are relative to the configuration file. `--world :library` selects one
project entry; otherwise all entries compile in sorted name order before outputs
are replaced. `:namespace` accepts an unqualified namespace symbol or string;
`:exports` maps nonempty WIT-path strings to qualified source symbols. Interface
functions require explicit mappings. Unambiguous freestanding exports may use
source metadata shorthand, with explicit mappings taking precedence. All existing
selected-WIT unsupported-feature diagnostics remain in force.

Without `:namespace`, existing `(ns app.core (gen-world :library) ...)` groups are
discovered through the same namespace grammar as dependency and source preparation.
The target directive is a retained Suss project extension, not a ClojureScript
core form. It has no Runtime or Macro effect. Discovery accepts .sus/.cljs/.cljc,
selects portable conditionals, sorts canonical source paths, deduplicates repeated
physical roots and avoids directory symlink cycles. Dependencies initialize before
the requiring source. A selected root already prepared as a dependency is not
replayed; other selected roots retain their deterministic order and effects.

Selected input text/forms are immutable. When a selected source is also read by
dependency preparation, differing text rejects compilation. Namespace mismatches,
ambiguous sources, malformed/duplicate target declarations and conflicting configured
targets fail before that input's macro preparation. Nonempty project `:deps` is
explicitly unsupported until published dependency loading is implemented; the
configuration cannot silently drop it. Project-mode CLI WIT/source/output overrides
are rejected rather than ignored; configure those settings per world.

Evidence:

- The parent-code command regression fails because the configured namespace is
  ignored and the prototype searches only for gen-world sources. The fixed five
  command/artifact tests execute successfully (5 passed, 0 failed).
- An explicitly selected WIT world/interface and freestanding shorthand execute
  compiled macros with separate phase counters, typed42/6 and effects1->3 after GC
  in two fresh Stores. A later invalid selected world preserves both prior outputs.
- Three grouped roots across all accepted extensions and repeated physical roots
  execute shared initialization once and an independent root effect: counter11,
  typed42/12, GC and two Stores. Unix also executes the directory cycle fixture.
- Configuration/namespace/selection errors preserve output. A Runtime throw compiles;
  each fresh component Store independently decodes the original17 before later99.
- An actual skipped-core regression failed because bootstrap provisioning suppressed
  explicitly selected source. Only sources prepared during the batch now suppress
  later roots; both project and namespace core entries execute42/24 after GC in
  two Stores. All original assertions remain.
- Both phase images/manifests reproduced exactly without Java; bootstrap4, existing
  affected CLI37, configuration4, compiler module/phase14 and Python118 pass. The
  final phase image regeneration and reproduction also pass at the updated compiler
  identity, with four executing bootstrap tests.

No shared runtime layout or upstream source form was ported. Independent review,
unfiltered baseline and final-head CI remain required. Native main/official command,
component-host migration and evaluator retirement are still open. This does not
complete rich portable environments, published dependencies, pending-I/O cancellation,
live-value accounting or M3. The scalar AOT boundary is still a development subset.

Independent review reproduced selected worlds silently overwriting a shared output,
then added preflight rejection for lexical and symlink output aliases, including
absent targets. The executing regression preserves prior artifacts and validates
the allowed single-world selection with typed calls after GC in two fresh Stores.
It also reproduced project routing silently ignoring `--main` without source and
a positional configuration alongside `--config`; explicit errors now preserve
outputs. All original project assertions remain in place. These are CLI-only fixes.
