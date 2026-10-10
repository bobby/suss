# Record iteration prerequisites

Complete pinned `not-empty` and `RecordIter` forms are retained with declaration
and packaged-file SHA-256 receipts and EPL notices. The import recipe selects both complete forms with hash-bound adaptations.
Generated artifacts are source evidence; bootstrap has been regenerated for this
source identity and native execution is verified below.

`RecordIter` preserves all five fields, mutable index, declared-field order,
index increment before lookup, MapEntry construction, short-circuit `hasNext`,
extension iterator delegation and returned Error behavior for `remove`.
Execution must cover throwing lookup after index advancement, nil/false fields,
extension delegation, live dependencies and retained iterator storage after GC.
A map-backed fixture can establish this iterator algorithm but cannot establish
nominal record construction, equality, hashing, factories or persistence.

`nil-iter` depends on the full `reify` macro contract, including generated
metadata protocols and captures. Replacing it with a simplified named type
would omit behavior. The genuine `reify` compiler lowering now executes at the
integrated head, but `nil-iter` itself remains unimported in this checkpoint
and the complete `defrecord` helper graph remains required. No record
acceptance criterion is complete from this staging.

Fresh pinned observations pass all fourteen cases. At the integrated head the
three native tests execute and pass: pinned order/effects in both phases,
host-handle survival with cross-fragment mutation after GC, and the
independently decoded returned Error for `remove`. The earlier RecordIter
Language errors were resolved by the canonical Object-field/rest-dispatch
munging retained with the str/StringBuffer prerequisite. Full record support
is pending.
