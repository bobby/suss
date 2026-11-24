# Namespace-local compiled macros

Source modules prepare top-level `defmacro` declarations in the isolated compiled
Macro session. Following forms use the resulting transformer through the same
reader, analysis, IR and Wasm pipeline as imported macros and the compiled REPL.
Script and AOT batches share this declaration preparation path.

The transformer sees the relevant namespace declaration's imports in Macro
phase. Ordinary required functions and referred macros load under the existing
phase rules. Runtime initializers remain staged in Runtime and execute in their
original order. Multiple namespace declarations in a script retain their own
import scopes. Original forms, source origins, UTF-16 locations, metadata and
source filenames reach the transformer without printing or rereading its forms.
Reader rewrites retain their original input identity; unrelated source files with
matching byte ranges cannot select another module's declaration context.

Declaration journals retain previous compiled function bindings, macro exports,
compiler-catalog policy and provenance. Preparation failures restore those records.
Native module loading also retains the journal through artifact verification and
actual instantiation, so a linking failure cannot publish replacements. Installed
state remains after an initializer failure, following the existing session contract.
Successfully initialized Macro dependencies and arbitrary expansion-time effects
retain their established behavior; generated code may remain resident until reset.
Cache keys retain the loaded helper sources and each declaration's provenance;
incomplete helper initialization disables emission reuse. Native preparation and
source identity work use the configured compilation allowance.

The executing regressions are in `inline_module_macros`, the source metadata and
native frontend suites, and `portable_session::tests::namespace_local_macros_roll_back_on_real_link_failure`.
`results/m12-reader-followup/inline-source-origin-and-cache-work` records the67
focused passing tests and exact inputs. The handoff and GitHub issues retain the
full-suite, workspace, CI and review evidence and remaining limitations.
