#!/usr/bin/env python3
"""Exercise the actual compiled Cargo build script's bootstrap identity inputs."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
PREFIX = "cargo:rustc-env=SUSS_COMPILER_SOURCE_SHA256="
ASSETS = (
    "crates/suss-compile/src/portable/stdlib/async.sus",
    "crates/suss-compile/src/portable/stdlib/async-macros.sus",
    "runtime/numeric/artifact/numeric.wasm", "runtime/numeric/artifact/manifest.json",
    "docs/roadmap/wasi-wit-lock.json",
    *(f"vendor/wasi/wasi-wit-0.3.1/{path}" for path in (
        "cli/cli.wit", "cli/deps/clocks.wit", "cli/deps/filesystem.wit",
        "cli/deps/random.wit", "cli/deps/sockets.wit",
    )),
)


def fingerprint(builder, root):
    result = subprocess.run(
        [str(builder)],
        env={**os.environ, "CARGO_MANIFEST_DIR": str(root / "crates/suss-compile")},
        capture_output=True, text=True, check=True,
    )
    identities = [line.removeprefix(PREFIX) for line in result.stdout.splitlines()
                  if line.startswith(PREFIX)]
    if len(identities) != 1:
        raise RuntimeError("build script did not emit exactly one compiler identity")
    return identities[0]


def copy_inputs(destination):
    for name in ("suss-core", "suss-reader", "suss-compile"):
        target = destination / "crates" / name
        shutil.copytree(ROOT / "crates" / name / "src", target / "src")
        shutil.copyfile(ROOT / "crates" / name / "Cargo.toml", target / "Cargo.toml")
    for relative in ("Cargo.toml", "Cargo.lock", "crates/suss-compile/build.rs", *ASSETS):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, target)


def main():
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    expected = json.loads((ROOT / "runtime/bootstrap/runtime.json").read_text())["compiler_source_sha256"]
    # The generator has already built this executable. Match its emitted identity
    # to avoid accidentally exercising an unrelated stale Cargo build directory.
    builders = [path for path in sorted(target.glob("debug/build/suss-compile-*/build-script-build"))
                if fingerprint(path, ROOT) == expected]
    if not builders:
        raise RuntimeError("no compiled build script matches the shipped compiler identity")
    builder = builders[0]
    with tempfile.TemporaryDirectory(prefix="suss-bootstrap-identity-") as directory:
        first = Path(directory) / "first"
        second = Path(directory) / "different-checkout"
        copy_inputs(first)
        copy_inputs(second)
        if fingerprint(builder, first) != expected or fingerprint(builder, second) != expected:
            raise RuntimeError("compiler identity depends on the checkout location")
        for relative in ASSETS:
            asset = first / relative
            original = asset.read_bytes()
            asset.write_bytes(original + b"changed runtime dependency")
            if fingerprint(builder, first) == expected:
                raise RuntimeError(f"compiler identity ignores {relative}")
            asset.write_bytes(original)
        if fingerprint(builder, first) != expected:
            raise RuntimeError("restoring runtime assets did not restore identity")
    print("compiled build-script identity: checkout-independent; embedded libraries, numeric assets and pinned command WIT graph invalidate")


if __name__ == "__main__":
    main()
