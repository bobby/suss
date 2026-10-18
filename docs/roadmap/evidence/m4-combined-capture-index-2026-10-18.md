# Combined native-field and capture-index experiment

This snapshot extends fdefa0f with compact capture indexes and generation-marked source-boundary traversal. It is the combined experiment, not the production PR268 transplant (which excludes native-field tables and has stronger dormant-rollover tests). Exact six changed source/bootstrap inputs are captured; all other compiler inputs are inherited from fdefa0f.

Normal bootstrap PASS. Three native tests PASS in26.37s; padded511-root DAG uses108440units versus1666978 for the exact old full-clear algorithm using the SAME new compact-index helper. That comparison isolates reset work, not historical total cost. It includes two generation rollovers but shared captured aliases do not expose stale dormant marks; the stronger production fixture is separate and receives no credit here. Typed fuel/host-interrupt retry and a cycle below a captured alias pass.

Unchanged complete source-child fixture PASS in42.64s, both source phases/five variants and final physical/memo negatives. This preserves a previously passing criterion rather than establishing a newly resolved issue.

Unchanged default-budget unordered-literal acceptance remains0PASS/1fuelFAIL/0ignored in46.67s, caller-&env transport510..531 before required values/effects. Actual lookups478hit/122miss events,312declarations started/311finished, compared with286/285 in fdefa0f's native-field run. These cover different completed prefixes and do not establish a whole-request win. All3643 captured file hashes remained unchanged through the runs.

Initial zero-test selection, three missing-core setup failures and intentionally cancelled bootstrap are preserved without execution credit. Normal bootstrap regeneration and corrected core-equipped test fixtures precede the successful tuple. Existing full-DAG cycle/missing/unfinished guards remain unchanged. No quota increase, tariff cut, fixture pruning or callback suppression.

No full baseline, CI success, closure, promotion or merge claimed. The bounded compact-index/generation delta is being tested separately on current Draft268 base96bc8d7 for publication there. Remaining preparation work still blocks the default request; original M4 criteria and other supporting issues remain required.
