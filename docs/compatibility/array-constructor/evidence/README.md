# Canonical Array constructor reference evidence

The retained six-case observations cover identity, callable arities and holes.
The later13-case files add single string, negative-zero length and negative,
fractional, NaN, infinite and uint32-overflow length errors, including argument
evaluation exactly once before each error. Fresh pinned runner exits0 and the
strict closed ordered comparison passes. File SHA receipts are retained.

These are reference prerequisites, not native constructor execution. Native
regression and implementation remain uncompiled/unexecuted. Retained constructor
GC/reload, full effects and capacity/storage behavior still need proof.

Later17-case files add multiple-argument effect order and array/object/undefined
single-element behavior. Fresh runner and strict comparison pass all17.
Undefined reads do not prove absent indexed properties; exact RangeError
identity/independent payload and large valid sparse lengths remain unproven.
A retained-constructor GC/cross-fragment native test is authored, uncompiled.

Later22-case files distinguish absent indexed properties from explicitly stored
undefined, test valid sparse lengths1000001 and4294967295 with last-index
writes, and verify4294967295 is an ordinary property that does not grow length.
Fresh pinned runner exited0 with all22 ordered Boolean results; three strict
harness regressions pass. Existing historical evidence is preserved. These are
reference results only; native tests remain uncompiled and Array storage and
RangeError identity still require implementation and execution.
