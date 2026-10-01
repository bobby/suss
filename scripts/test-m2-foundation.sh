#!/bin/sh
# Executing M2 foundation acceptance suites; the full workspace remains required.
set -eu
m2_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$m2_root"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
cargo test -p suss-reader --locked -- --test-threads=2
cargo test -p suss-compile --locked \
  --test portable_resolution --test portable_modules --test portable_pipeline \
  --test portable_closures --test portable_nominal --test portable_exceptions \
  --test portable_dynamic_bindings --test compile_expr --test runtime_abi \
  -- --test-threads=2
cargo test -p suss-cli --locked \
  --test persistent_session --test portable_collection_literals \
  --test portable_native_protocols --test portable_object_methods \
  --test portable_exceptions --test portable_dynamic_bindings \
  -- --test-threads=2
