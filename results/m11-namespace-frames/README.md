# Namespace loading size probes

These probes measure complete, validated components and their executable code.
The before compiler is main ef882633. The after compiler source identity is
`7558bfb75df8fb4b212017db8d1c31659d4178f8e2fb692e81f057d5d58af6fa`.
The JSON files retain executable, source and component SHA256 receipts, exact
commands, sizes and defined-function counts. Both use Cargo stable 1.99.1's test
profile, without Binaryen. Each source has 4 or 24 ordinary function definitions.

| Measurement | Before | After |
| --- | ---: | ---: |
| Complete component growth, empty scope, bytes/form | 4,906.8 | 3,801.6 |
| Complete component growth, 100 excluded names, bytes/form | 9,173.8 | 3,801.6 |
| Extra executable code per split form, against grouped control | 589 B | 96 B |

`components.json` records the direct Rust component API's measurements, with no
source path. `before.json` and `after.json` record CLI file compilation. Source
paths add artifact data to the latter's total sizes; compare their executable
code independently. Both CLI runs use identical source hashes. The grouped
control puts the same definitions in a single top-level `do`, paying for one
namespace frame. Its code growth stays 3,580 bytes. Split-form growth falls from
15,360 to 5,500 bytes over the 20 additional definitions. The difference includes
each split eval function and loader call, not only namespace assignments.

To reproduce with frozen before and after executables, choose fresh output
paths (the probe refuses to overwrite evidence):

```sh
python3 results/m11-namespace-frames/probe.py --suss /path/to/before-suss --output target/probe/frame-before
python3 results/m11-namespace-frames/probe.py --suss /path/to/after-suss --output target/probe/frame-after
```

The CLI validates the components. The Python parser counts actual function body
bytes; the regression independently uses wasmparser and executes the normal
source/AOT tests. CLI probes alone do not establish runtime behavior.

The retained focused logs record 24 CLI passes, one shared-runtime failed-load
cleanup pass, seven async import passes, 16 option passes, and the selected list
and mixed-schema passes. The failed-load case checks payload, effect order,
namespace identity after GC and the dynamic frame chain. Foundation checks
record 358 Python passes plus unchanged inventory, reviews and generated sources.
These focused results do not establish the complete workspace or final-head CI.

Negative observations are preserved: the unchanged base fails the repeated-scope
regression; the exploratory 3,000-byte total bound fails at 3,801 bytes/form; and
the initial boundary test build fails before execution on test-only signatures
and reference types. The corrected runs are separate logs. This is a code-size
measurement, with no walltime or instruction-count performance claim.
