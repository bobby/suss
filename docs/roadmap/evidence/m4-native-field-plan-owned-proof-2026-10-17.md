# Native field-plan owned artifact proof — rejected runtime load

Tested source is96bc8d70 plus the four frozen Rust files and their four normal BOOT artifacts. Snapshot parent3d546a9 retains the previous diagnostic backup; this snapshot adds only the tested native proof, BOOT and handoff/evidence. Current borrowed/view prototypes are different unexecuted source and excluded.

Normal bootstrap PASS; exact32 unique native tests PASS:24 existing+3 prefix+5 field artifact. Earlier27 prefix tests overlap these32 and are not added. V1 compile E0277 and initial missing-module-path attempt remain separate failed setup attempts with no execution credit.

Both Runtime and Macro phases:615 actual completed shipped owners,1577 fields,10041 canonical keys,619690 encoded bytes. Original BYTE planner is the oracle: full row/root/key IDs, independent cold rebuild, warm traversal, and encode/decode comparisons passed. All spans/metadata remain on the actual owner-bound borrowed Form. Malformed shape/provenance/occurrence, collision, copied/reanalyzed/restored owners and native failure/retry negatives passed.

Identical actual per-phase native proxy costs:

|Stage|Units|
|---|---:|
|Original cold plan+clear|14,219,065|
|Original warm plan+clear|11,843,439|
|Snapshot storage|5,948,879|
|Encode|6,056,408|
|Owned decode+validate+storage|28,464,962|
|Decode backing (subset, not additive)|18,914,698|
|Exact correspondence replan|15,116,808|
|Bind|291,004|
|Lookup all fields|77,016|
|Template row reads|325,415|
|Reference/flat cleanup|382,233|

Decode+correspondence+bind alone43,872,774. Reject this owned per-request runtime load. No Graph field admission or runtime prefix is enabled. A future checked borrowed view would require separate source/cost/effect proof; no measured saving is inferred here.

Identity/input setup2,910,729 and comparison assertions2,575,537 are separately reported; stages are not indiscriminately additive. Catalog discovery, fixture clones/assertions and early-error/recursive last-owner cleanup exclusions remain explicit. Injected StopFuel/Irq are native surrogates, not Session fuel/epoch observations. This is a native-field proxy, not completed caller &env transport, default acceptance, whole-request savings, full baseline or CI result.

Commands and exact selection are retained in tested-run/driver.py, selection.json and raw logs: normal suss-bootstrap first, then cargo test --locked -p suss-compile --lib portable_macro_graph::raw_keys:: -- --test-threads=1 --nocapture. All4046postbootstrap inputs remained unchanged.
