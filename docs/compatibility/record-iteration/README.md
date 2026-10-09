# Record iteration prerequisites

Complete pinned `not-empty` and `RecordIter` forms are retained with declaration
and packaged-file SHA-256 receipts and EPL notices. The import recipe now selects both complete forms with hash-bound adaptations.
Generated artifacts are source evidence; bootstrap and native execution remain
unverified.

`RecordIter` preserves all five fields, mutable index, declared-field order,
index increment before lookup, MapEntry construction, short-circuit `hasNext`,
extension iterator delegation and returned Error behavior for `remove`.
Execution must cover throwing lookup after index advancement, nil/false fields,
extension delegation, live dependencies and retained iterator storage after GC.
A map-backed fixture can establish this iterator algorithm but cannot establish
nominal record construction, equality, hashing, factories or persistence.

`nil-iter` depends on the full `reify` macro contract, including generated
metadata protocols and captures. Replacing it with a simplified named type
would omit behavior. That dependency and the complete `defrecord` helper graph
remain required. No record acceptance criterion is complete from this staging.

Fresh pinned observations pass all fourteen cases; the retained pre-import native
binary fails on unresolved RecordIter. Next: regenerate bootstrap and execute
both phases when the single Cargo slot is available, then execute the authored independently decoded Error and live dependency
probes. Full record support is pending.
