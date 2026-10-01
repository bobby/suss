# Portable vector trie prerequisites

Retain the complete VectorNode and nine private node/path/update declarations
from ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1. The importer records
source hashes, explicit patches, original source artifacts and the EPL notice.
VectorNode is unchanged. Fixed private defn declarations use explicit def/fn
bootstrap patches; every source branch and recursive call remains present.
Private Var visibility and compiled upstream macros remain unfinished. No new
runtime primitive, host collection, field layout or ABI is introduced.

The first focused regression failed with the located compile diagnostic
`Unresolved Runtime name suss.core/pv-fresh-node`; after importing source, it passes.
The original development-only VectorFixture has only a cnt field so the pinned
helpers can execute their field reads before PersistentVector itself is retained.
It is a test fixture and is not shipped as a replacement collection.

`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
sh scripts/test-vector-trie-oracle.sh` freshly compiles and executes the pinned
reference: 23 exact raw Boolean/IEEE754 observations match. Private Var warnings
are expected from calling the actual pinned helpers. Native tests independently
decode raw i31 Booleans and one-field f64 Numbers; no opaque node wildcard or
runtime encoder is used. Nodes are observed through identity, width, field reads
and mutation effects. Tail counts 0/31/32/33/1024/1025, new paths through level 15,
existing/new insertion paths, recursive association and both removal branches
execute. Four focused native tests pass, including fragments separated by GC,
shallow clone ownership, unchanged branches, live helper exception 19 and recovery.

82 Python tests, 115 strict import artifacts and review counts 206/859 checks pass. Required full workspace
baseline passes through final reader doc tests using the shared target, build jobs2,
--workspace --locked -- --test-threads=2. Existing explicit manual ignores remain.
Independent PR review and exact reviewed-head CI are pending. Full PersistentVector, transient vectors, chunked iteration, public
collection protocols, collision nodes, maps/sets and M4/M7 acceptance remain open.
Next retain the required vector indexing/error and iteration/reduction dependencies,
then the complete PersistentVector and its source protocol methods.
