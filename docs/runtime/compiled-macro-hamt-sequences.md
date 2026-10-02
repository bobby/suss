# Compiled hash-map sequence transport

Canonical NodeSeq and ArrayNodeSeq values now cross the compiled macro boundary
as reader list syntax containing entry vectors. Their descriptor roots remain
valid after class redefinition and GC. Transport reads the actual current child
sequence and remaining node cursor in pinned order; it does not execute arbitrary
collection protocols or rebuild a sequence from the original map.

Field widths, source-array ownership, cursor bounds and pair alignment are
checked. Sparse unused slots honor the pinned nil? treatment of nil and undefined;
undefined remains unsupported as a standalone syntax value. The shared traversal
budget and explicit recursive sequence depth limit reject corrupt or cyclic child
cursors with the original supplied source location. Entry children retain the
existing syntax depth and metadata rules.

The collision corpus exposed a separate missing dependency: persistent array-map
assoc at its ninth entry called an uninitialized `into`. Complete pinned public
`conj`, `transduce` and `into` forms are now retained with all arities, docstrings,
branches and metadata, using only explicit defn bootstrap patches. Their original
source and EPL provenance are manifest-bound. Reviews remain `:in-progress`.

Twenty colliding canonical Symbol keys now exercise actual persistent array-map
conversion, HAMT collision storage and old values. The development oracle has 27
fresh pinned observations: exact sequence entries/order for 9/16/17/32/33/65/128
entries, next/nnext cursors, nil keys, packed nodes and collisions, plus the new
public functions' arities, metadata, nil/noneditable targets, transducer completion
and reduced termination. Native tests independently decode forms and numeric bits
in both Stores. Additional regressions retain old descriptors/metadata across GC
and reject foreign layouts, invalid cursors and a forged self-cycle, while
accepting valid sparse tails.

Suss preserves textual evaluation order for unordered literals, following the
accepted design and user decision. The pinned compiler's reader hash iteration
reorders constructor expressions in map literals. For colliding keys this can also
change the resulting sequence order: the observed constructor-literal example
started with key5, while explicit sequential assoc started with key0. The collision
corpus therefore uses explicit ordered assoc calls to give both implementations
the same construction order. This documented literal-order variance is not a
change to the retained HAMT traversal algorithm.

Larger cases still use a bounded 100M fuel allowance. This is not proof of frontend
performance under the default budget. Iterator/reify dependencies, general member
munging, optional undefined method arguments, complete collection acceptance and
the original M3 macro environment/bootstrap/cache/lifecycle gates remain open.

Commands:

```sh
scripts/test-hamt-sequence-oracle.sh
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test -p suss-cli --test compiled_macro_hamt_sequences --locked -- --test-threads=2
cargo test --workspace --locked -- --test-threads=2
```

Actual terminal results and remaining validation gates are in
[the handoff](../roadmap/handoff.md).
