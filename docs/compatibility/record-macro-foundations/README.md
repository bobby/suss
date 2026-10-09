# Complete record macro prerequisites

Twelve complete pinned forms are retained with source ranges, declaration and
file hashes. These are planning inputs, not imported or executing macros.

`reify` captures every lexical local, generates IMeta and IWithMeta methods,
transfers reader metadata after elision and guards type publication with exists?.
Its namespace-derived generated name requires actual munging behavior. A named
type or metadata-free substitute would omit the accepted behavior.

`defrecord` requires all emit-defrecord methods, declared fields plus metadata,
extension map and mutable hash, protocol masks, full method annotation and
positional/map factories. Factory extmap processing uses not-empty and cond->>;
record input conversion and metadata handling remain required. Full source
transport and nominal record storage must preserve all branches.

Compiler/analyzer dependencies include resolve-var, namespace/local facts,
reader-meta elision and munging. The exists? symbol path emits JS typeof checks;
portable adaptation needs equivalent defined-cell semantics and nested property
existence, not a constant true/false. No source branch is excluded by staging.

Next: execute iterator prerequisites, then implement these complete helper
contracts and generic nominal record construction. Prove captured-local GC,
metadata, type redefinition, factories, ordered effects, equality/hash and
persistence against fresh pinned execution. Original M4 acceptance stays open.
