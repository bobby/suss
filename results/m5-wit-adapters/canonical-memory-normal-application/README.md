# Normal CLI application evidence

These captures execute published application source `2a779949`, using its immutable `--profile test` compiler and Rust host binaries. They do not execute this branch as a separate application checkout. `manifest.json` verifies that the allocator's complete Rust source is byte-identical on both branches (SHA256 `9c1c163df78e4af12cd868eb6577463197cb10a192ddfe99e6f7e2d0e8d9d609`). The original failed allocator and intermediate application probes remain in their earlier evidence directories.

## Commands and source qualification

- `cargo build --profile test --locked -p suss-cli -p suss-usermanager-host --bins`: exit0,6.698s;14,320 unchanged recorded inputs/HEAD.
- `python3 examples/usermanager/measure.py --suss target/probe/usermanager-final-bin/suss --host target/probe/usermanager-final-bin/suss-usermanager-host --measure target/probe/usermanager-final-bin/measure --output target/probe/usermanager-measurements-final`: exit0,1,342.408s. Three initially empty explicit compiler caches, three cached repeats, actual JSON CLI write/reopen, and three native/JVM process repetitions for each of16/1,000/10,000users all pass. The driver's own audit verifies unchanged production sources and native binaries. The outer14,323-input receipt honestly records the concurrent append to the separate focused-test log; HEAD and prerequisite tools are unchanged.
- `python3 target/probe/run-usermanager-large-canonical-observer.py`: exit0,154.930s. This separately identified standalone Rust harness links the saved host library and the checked-in test-only read-only observer. It validates and instruments the normal CLI-produced component while retaining its exact original byte prefix. Five1,000-row and five10,000-row joined/sorted responses, typed host errors, GC and final close all return live canonical blocks and bytes to zero; host resources finish opened1/closed1/live0. Host-side seeding isolates large transfers; the unmodified measurement driver instead performs setup through guest CRUD. The harness's ignored-input/library/component receipt has no changed inputs. The outer receipt records only two concurrent evidence-log appends. This is not a Cargo/workspace baseline.

All three fresh and cached components are4,585,724bytes, SHA256 `9202f3c8b81f2149c1a85861ea5eaf4faf27a519de38c2ee8a12e31962004bc5`. Every raw component, stdout/stderr, process receipt, source/binary audit, machine/load/cgroup record and statistical spread is compressed individually under `measurements/`. The source and observer-specific receipt are under `observer-inputs/`. Native executable hashes/sizes are retained; the executable cache itself is not a release artifact.

## Measurements and limits

Fresh compilation median73.864s(range55.615–91.993s,n3); cached compilation median0.310s(range0.168–0.412s,n3). Native10,000-user process-to-first-response median63.561s(range63.310–89.289s,n3), JVM6.646s(range6.559–7.063s,n3). Native peak RSS median1,705,968KiB(range1,641,976–1,732,840KiB); JVM164,792KiB(range159,800–231,372KiB).

| 10,000-user operation | Native median [min,max] | JVM median [min,max] | Samples |
| --- | --- | --- | --- |
| Joined/sorted list |2.642s [2.316,16.662] |0.114s [0.060,0.365] |15each |
|100create/read/update/delete cycles |2.387s [2.098,2.807] |0.029s [0.016,0.087] |15each |
|1,000change-counter calls |8.011ms [7.281,31.486] |2.844ms [1.439,13.755] |15each |

All observations are inclusive; callback time is separately retained without subtraction. Native uses workspace test optimization1/dependencies2, debug assertions on and Wasmtime `OptLevel::None`. JVM uses the shared application namespace and a persistent in-memory Clojure-map adapter; Rust uses BTreeMaps and serializes the entire database before every write for rollback even in its in-memory path. Explicit CLI scenarios include actual file I/O. OS caches were not dropped. Concurrent baselines, CPU quota2cores and memory limits are recorded. This is a machine-specific baseline, not an optimization gate or a universal speed comparison.

Request fuel is the documented2,000,000,000 host policy; the unchanged100,000,000-fuel allocator regressions independently establish the bookkeeping repair. Healthy10,000-row calls consume about1.09billion fuel. Raw memory-growth events include multiple component memories and are not GC-heap accounting. Canonical live allocations returning to zero does not mean committed memory shrinks: the allocator retains reusable storage. Observer assumptions remain restricted to the checked allocator module and canonical alignments at most16.

Foundation/bootstrap, the workspace baseline, independent review and final-head CI remain required before issue closure or draft promotion; these application captures do not substitute for those checks.
