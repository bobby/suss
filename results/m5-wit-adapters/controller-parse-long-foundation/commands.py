import json
import subprocess
import sys

commands = [
    ["python3", "-m", "unittest", "discover", "-s", "scripts", "-p", "test_*.py"],
    ["python3", "scripts/cljs_inventory.py", "--check"],
    ["python3", "scripts/cljs_reviews.py"],
    ["python3", "scripts/core_import.py", "--check"],
    ["python3", "scripts/reader_core_catalog.py", "--check"],
    ["python3", "scripts/apply_bootstrap.py", "--check"],
    ["python3", "scripts/sequence_provenance.py"],
    ["python3", "scripts/check_transducer_provenance.py"],
    ["python3", "scripts/check_when_let_provenance.py"],
    ["python3", "scripts/bitwise_provenance.py"],
    ["python3", "scripts/wasi_lock.py"],
    ["python3", "scripts/numeric_runtime.py", "--check"],
    ["python3", "scripts/jvm_array_oracle.py", "--check"],
    ["python3", "scripts/clojure_test_suite.py"],
    ["python3", "scripts/clojure_test_suite.py", "overlap"],
    ["scripts/verify-bootstrap.sh"],
    ["cargo", "test", "--workspace", "--locked", "--lib", "--bins", "--examples", "--", "--test-threads=2"],
    ["cargo", "test", "--workspace", "--locked", "--doc", "--", "--test-threads=2"],
    ["cargo", "test", "-p", "suss-cli", "--locked", "--test", "core_import", "--test", "portable_reader_constructors", "--", "--test-threads=2"],
    ["cargo", "test", "-p", "suss-compile", "--locked", "--test", "math_context_constructors", "--", "--test-threads=2"],
]
for command in commands:
    print(json.dumps({"command": command}), flush=True)
    result = subprocess.run(command)
    print(json.dumps({"command": command, "exit_code": result.returncode}), flush=True)
    if result.returncode:
        sys.exit(result.returncode)
