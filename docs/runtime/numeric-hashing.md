# Retained scalar numeric hashing

This slice retains pinned `hash-double` and `hash-combine` with fixed defn patches,
original composition and EPL provenance. It does not select public `hash`, nor
claim complete numeric/collection hashing or JavaScript typed-buffer support.

The nonescaping hash-double Float64Array/DataView storage is adapted to a boxed
binary64 temporary. One checked scalar ToNumber conversion precedes both reads;
the original live hash-long call remains after their evaluation. Each word reads
the exact four bytes at offsets0/4 as signed big-endian, modeling little-endian
Float64Array storage. Simply XORing the un-swapped binary64 halves is incorrect
for the pinned reference. The explicit byte order is a portable adaptation of a
host representation dependency, not a general typed-array interoperability claim.

Three original private compiler/runtime primitives normalize the scalar and read
the two words. They retain raw binary64 bits (including signed zero/NaN payloads)
and validate Number storage before casts. Scalar coercion supports the existing
numbers/nil/booleans/UTF-16 string domain; arbitrary object ToPrimitive remains
unsupported with typed errors. HIR/IR check unary arity and numeric result types;
these primitives are not public core cells or bitwise macro names.

52 fresh primary/native observations match, preserving the original42. Three
source guards cover scalar coercion, captures/live dependencies, effect order,
checked runtime arity/errors/GC recovery and located compile-atomic private
primitive arities with both-phase validation. Compiler HIR/IR2 and ABI25 pass;
the new ABI guard checks267 binary64 encodings including signed zero, infinities,
signaling/quiet NaN payloads and malformed storage with typed recovery. Python76
and inventory/import/WIT/numeric/bitwise/offline roadmap gates pass. Independent
review, full workspace baseline and exact final-head CI remain pending.
Source recipe selects45 forms/49 licensed files; overlay132 partial/933 unassessed.
Shared GC layouts, runtime ABI version and native core cell count are unchanged.
Cached/public/string/numeric/collection hashing, public equality, persistent
sequences and compiled macro bootstrap remain incomplete.
