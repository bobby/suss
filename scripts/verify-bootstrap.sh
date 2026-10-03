#!/bin/sh
# Reproduce and execute the shipped bounded bootstrap with Java absent from PATH.
set -eu
bootstrap_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
bootstrap_tmp=$(mktemp -d)
trap 'rm -rf "$bootstrap_tmp"' EXIT HUP INT TERM
mkdir "$bootstrap_tmp/tools"
# Cargo/linker helper tools are available; Java and Node are deliberately absent.
for bootstrap_tool in cargo rustc rustdoc rustup cc clang gcc ld ar ranlib as strip xcrun dsymutil python3 sh bash pkg-config cmake ninja make perl uname env grep sed awk basename dirname readlink realpath mkdir rm cp ln touch sort tr wc cmp git; do
    bootstrap_path=$(command -v "$bootstrap_tool" || true)
    if [ -n "$bootstrap_path" ]; then ln -s "$bootstrap_path" "$bootstrap_tmp/tools/$bootstrap_tool"; fi
done
PATH="$bootstrap_tmp/tools"
export PATH
if command -v java >/dev/null 2>&1 || command -v node >/dev/null 2>&1; then
    echo 'bootstrap verification requires Java and Node absent from PATH' >&2
    exit 1
fi
cd "$bootstrap_root"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- "$bootstrap_tmp/first"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- "$bootstrap_tmp/second"
for bootstrap_phase in runtime macro; do
    for bootstrap_suffix in wasm json; do
        cmp "$bootstrap_tmp/first/$bootstrap_phase.$bootstrap_suffix" "$bootstrap_tmp/second/$bootstrap_phase.$bootstrap_suffix"
        cmp "$bootstrap_tmp/first/$bootstrap_phase.$bootstrap_suffix" "runtime/bootstrap/$bootstrap_phase.$bootstrap_suffix"
    done
done
python3 scripts/verify_bootstrap_identity.py
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test -p suss-cli --locked --test compiled_bootstrap -- --test-threads=2
