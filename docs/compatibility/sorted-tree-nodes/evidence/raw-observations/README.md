# Supplemental independent tree-node observations

This repairs P1/P2 in the independent review of frozen `0c98b4f7523aaf369fce6813c63ea8c13b2a2205`. The original 86-case Boolean corpus and all assertions remain supplementary and unchanged. No source implementation defect was identified by that review.

P1: both native `unwrap_f64()` assertions now convert to bits before comparing against `u64` expected bits. Rustfmt parsing and inspection of locked Wasmtime 49.0.1 APIs are authoring checks, not compilation.

P2: the separate 52-case raw corpus has prewritten host expectations. Its fresh pinned driver observes actual upstream BlackNode/RedNode own fields, canonical constructors, Reduced payloads, number bits, Error UTF-16 messages, thrown-versus-returned status and scalar ordered traces. It does not use the guest shape helper, equality, printing, or the companion balance function for RedNode method expectations. Node expectations include both keys and values, both children, class, and hash slot. The six RedNode method expectations are explicit trees matching independently observed pinned outputs.

Coverage includes ten insertion balance branches, six deletion shapes, six RedNode balance methods, seven comparable classes (plus cross-node comparisons), two live delegates, seven ordered comparable rejection paths, deletion/nth/invoke/redden errors, three reduction outcomes, original thrown 73, and operand expression order through a rotation. Comparable rejection traces observe both live str_ calls before both delayed conversions (1122). The numeric trace is inspected independently of the returned tree/error. Recovery 60 is independently checked after each native case.

The supplemental authored native test reads actual canonical descriptor references and owner types, exact descriptor/source schemas, actual node fields and Reduced payloads after GC. Runtime Error must retain canonical identity, descriptor number 7, exact owner layout, nil data/cause, and UTF-16 message storage. Unknown classes, cycles, non-tree children, exhausted traversal budget, excessive depth and oversized Error messages fail closed. Shared children are allowed when not ancestors. Native rejection controls restore mutated slots explicitly and observe the restored object after GC. These controls and all native cases are **uncompiled and unexecuted**.

Commands:

- `sh scripts/test-tree-node-raw-oracle.sh`: fresh pinned CLJS compile/Node, 52 exact raw observations passed; full log retained in `reference.log`.
- `python3 -B -m unittest discover -s scripts`: 255 tests passed in 13.388s; retained in `python.log`. Includes closed-shape/type/message/order/count/depth negative controls. These are Python authoring checks, not native proof.
- `rustfmt --edition 2024 crates/suss-cli/tests/portable_sorted_tree_nodes.rs` and `git diff --check`: passed.

`receipt.json` hashes retained inputs/observations/log and generated reference JS. Generated JS files are not shipped or committed; their hashes record this execution only. Native compilation, execution, bootstrap regeneration, focused gates, full baseline and final-head CI remain pending. No Cargo/bootstrap/push occurred. Full tree collections authoring is proceeding separately in the same isolated worktree; its uncommitted source does not establish any native acceptance.
