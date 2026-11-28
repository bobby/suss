# JVM regex compilation observations

`python3 results/m12-regex-followup/oracle.py` records76 actual JVM Clojure1.12.6
regex compilation inputs:50 accepted and26 rejected on the recorded OpenJDK21.
Inputs, exact pattern/print/error UTF-16 units, flags, syntax descriptions and
indices remain in `pattern-cases.edn`; JAR/source/input/output hashes and the
host-version qualification are in `command.json`.

These are primary dependency probes only. No Suss regex implementation or
acceptance pass is established. The initial Python syntax error occurs before
any JVM execution; its complete source and terminal output remain separate.
