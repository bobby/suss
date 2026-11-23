# UUID and regex reader dependency observations

## Native tooling EDN UUID observations (2026-11-23)

`run-focused.py native-edn-uuid-after-source-projection` passes all four focused
UUID tests. Native EDN now retains typed128-bit values, canonical printing/key
duplicate checks, configured-reader precedence, builtin-before-default behavior
and lossless source-data projection. Serde rejects unsupported UUID conversion
rather than silently turning it into a string. Original missing match-arm
compilation failure is retained in `native-edn-uuid-gates`.

`run-reader.py native-edn-all-reader-gates` passes all84 reader-crate tests with
zero failures/ignored tests and recorded inputs unchanged. This is native tooling
EDN evidence only. Source reader selection, compiled Runtime/Macro UUID values,
public predicates/functions, macro transport, retained-value/GC and actual AOT
still require integration. Earlier native-parser-only results retain their own
source snapshots; none establishes complete UUID or BigDecimal acceptance.

`python3 results/m12-reader-followup/oracle.py` records204 fresh pinned JVM
Clojure1.12.6 expression controls. UUID inputs include short, upper-case,
plus-prefixed, over-width and Unicode hexadecimal groups; regex observations
retain raw pattern text and identity semantics. These are reference results,
not claims that Suss implements either runtime value yet.

`run-before.py` executes a copied pre-repair source2254f70c CLI with binary
SHA256969bbcd27acb249c24c2d52ed43be6727c7f1e3685a508883fca39fdd3effff3.
All13 selected native/AOT commands fail explicitly: unknown uuid tag or unsupported
regex dispatch. Input text and the exact commands/output are retained. They are
reader/compilation failures, not executions of macro or UUID runtime values.

`oracle-uuid-parser.py` independently records235 UTF-16 UUID input cases from
actual `java.util.UUID/fromString`:179 accepted and56 rejected. Complete signed
halves, canonical text, hashCode and error-message UTF-16 units are retained.
The same JVM records Character.digit(char,16) for all65536 units. Source/JAR
hashes, OpenJDK bytecode and commands qualify these primary observations. The
initial giant inlined fixture exceeded the JVM method-size limit; its source,
terminal failure and command remain under `uuid-parser-oracle-too-large`.
The subsequent recording reads the same data as EDN rather than embedding it
in one bytecode method; no primary cases were dropped.

`run-focused.py uuid-parser-gates` passes all three focused Rust tests with
unchanged inputs. The original parser retains immutable128-bit data and lossless
UTF-16 errors, masks accepted groups only after checked signed-long parsing,
compares signed halves, and exactly matches the full BMP hexadecimal scan.
It has no Java dependency. This establishes a native parser/data helper only:
compiler forms, native EDN, Runtime/Macro values, readers, public functions,
macro transport, retained-value/GC checks, AOT and affected suite execution still
need integration. No issue acceptance, workspace or CI pass follows from these
three focused tests. Original BigDecimal and suite baselines remain unchanged.
