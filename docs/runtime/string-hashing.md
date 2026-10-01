# Retained UTF-16 string hashing

This slice retains pinned `m3-hash-unencoded-chars`, `hash-string*` and `pos?`
with explicit fixed defn bootstrap patches, original algorithms, metadata,
docstrings and EPL provenance. It does not select cached `hash-string` or public
`hash`; numeric, collection and general object hashing remain unfinished.

The original runtime implements a bounded String `charCodeAt` member adapter.
Named lookup returns a shared unbound builtin closure; existing member invocation
anchors the physical receiver and evaluates lookup before index arguments.
A detached ordinary call raises a language error. User-defined Object methods
with the same name retain normal dispatch. The singleton root retains no owner.

The receiver must be native UTF-16 String storage. Scalar indices use checked
number coercion and ToIntegerOrInfinity: NaN becomes zero, fractions truncate,
negative/out-of-range/infinite indices return NaN without an integer-cast trap.
This is not general JavaScript ToString, object index coercion, prototype mutation
or full host interoperability. Existing native member invocation fills a missing
index with Undefined (coerced to zero) and ignores evaluated surplus indices,
matching the pinned builtin. Core function and macro wrong-arity diagnostics
remain unchanged.

The bounded `pos?` macro compares one operand with zero; the retained function
value uses its own canonical cell. Full compiled macro execution remains open.

82 fresh pinned observations match independently decoded validated native Wasm
following forced GC, preserving the original 71 probes. Three native guards cover
located compile-atomic predicate diagnostics, both phases/aliases/exclusions,
member domain failures, detached calls and recovery after GC. Missing/extra index
observations are separate from core function arity behavior. Source-level warnings
about repeated development probe definitions remain visible.

Four additional partial reviews bring the overlay to 130 reviewed/935 unassessed.
The source recipe selects 43 forms and retains 47 licensed files. No shared GC
layout, ABI version or native bootstrap cell change is introduced. The original
checked builtin scalar and singleton root are internal runtime adapters.
Independent PR review, full workspace baseline and exact final-head CI remain
required before readiness. Full cached string/public/numeric/collection hashing,
persistent sequences and compiled macro bootstrap remain unfinished.
