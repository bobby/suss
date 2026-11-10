#!/bin/sh
# Every calls.wat measurement for ADR-0004, as checked in under results/.
# Usage: calls.sh > results/run_calls.txt
cd "$(dirname "$0")" || exit 1
cargo build -q --release --bin calls || exit 1
./target/release/calls --emit target/calls.wasm
uptime
echo "wasmtime 49.0.1, n = 2e7, best of 5"
./target/release/calls 20000000 5
echo
echo "V8 $(node --version), default tiering, n = 2e7, best of 5"
node calls.mjs target/calls.wasm 20000000 5
echo
echo "User-space instructions, (count at n - count at 0) / n, on wasmtime"
python3 calls.py 20000000
echo
echo "On V8 with --single-threaded --no-liftoff: TurboFan without call feedback"
python3 calls.py 20000000 --node
uptime
