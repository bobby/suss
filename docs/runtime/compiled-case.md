# Compiled case dispatch

The compiler-owned Clojure1.12.6 macro in
[case.sus](../compatibility/bootstrap/case.sus) constructs balanced branches
over signed32-bit literal hashes using the existing verified control flow.
The selector evaluates once; only its chosen arm/default executes, including
tail recurrence. Constants remain quoted data, including symbols and nested
collections. Integer tables use Number.intValue then fixed core equivalence;
keyword tables use identity. Other tables use Java hashCode and fixed core
equivalence. Genuine collision buckets capture live public equality once,
with the literal first, matching pinned condp. Hash dispatch is not replaced
by a blanket sequence of public equality calls.

Duplicate detection uses ordinary Clojure equality/hash semantics independently
of matching. Decimal scales usually produce distinct Java hashes, but rare equal
hashes permit matching equivalent Decimals across scales; scalar and nested
primary controls cover this. Signed zeros are duplicate constants; repeated NaN
constants remain distinct. Macro exceptions retain their native message and
caller location without invoking a rebound public accessor or guest printer.
User macro definitions and caller arms remain unprivileged. The scalar bootstrap
remains for building core before the source expander is available.

The existing AOT multi-argument str specialization now keeps the original core
function for its unary conversions. Calls and value aliases preserve their
separate JVM-confirmed behavior. Session/Macro use the ordinary JVM oracle;
actual AOT uses JVM direct linking under the accepted design. Original failures
and both primary modes are retained.

[Provenance/license](../compatibility/bootstrap/clojure-case.json) and
[commands and limitations](../../results/m12-case-dispatch/README.md) qualify
these results. All72 vendored assertions pass host comparison with identical IDs
and zero guest judgments, failures, skips, missing assertions or failed
namespaces. Independent retained-field/GC checks cover both phases. These focused
results do not establish an unfiltered workspace or final-head CI pass, arbitrary
Java-object interop, or complete BigDecimal acceptance.


## Component input provenance

Standalone source artifacts keep their complete format2 EDN identity. Assembly
verifies every input before removing that custom section from its private copy.
The component's `suss.input-source-artifacts` format1 section retains every
original field and input order, with identical macro dependency graphs shared
through indexed references. Unknown (`nil`) and known empty graphs remain
distinct. `portable::aot::input_source_manifests` expands the records and rejects
missing/duplicate root sections, unsupported formats, incomplete records and
invalid references. Raw/expanded provenance is bounded at32/64MiB.

These manifests describe verified inputs, not signatures of Binaryen-rewritten
output. Runtime ABI sections and initialization metadata remain on the modules.
The original AOT size guard caught3985 bytes/form after adding the case macro;
sharing complete graphs reduces it to2352 with the existing3900 limit unchanged.
An actual component executes the two versions of a user macro and case dispatch;
its retained manifests match all13 input fields. Independent pinned JVM EDN
reading expands the actual section and compares all five original input records,
including both user macro versions and the case macro identity. Source hashes,
raw component hash, original failed probes and commands are retained separately.
