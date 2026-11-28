#!/bin/sh
# Rebuild the independent Rust guest, then execute it through Suss.
set -eu
interop_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$interop_root"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_BUILD_JOBS
interop_guest_target="${SUSS_WIT_RUST_TARGET_DIR:-$interop_root/target/probe/wit-rust-guest}"
interop_output="$interop_root/target/probe/wit-rust-component"
mkdir -p "$interop_output"
rustup target add wasm32-unknown-unknown
cargo build --locked --manifest-path tests/fixtures/wit-interop-rust/Cargo.toml --target wasm32-unknown-unknown --target-dir "$interop_guest_target"
cargo run --profile test --locked -p suss-compile --example wit_interop_componentize -- "$interop_guest_target/wasm32-unknown-unknown/debug/suss_wit_rust_guest_fixture.wasm" "$interop_output/provider.wasm"
SUSS_WIT_RUST_GUEST_COMPONENT="$interop_output/provider.wasm" cargo test -p suss-compile --locked --test wit_interop_rust_guest -- --test-threads=2 --nocapture
