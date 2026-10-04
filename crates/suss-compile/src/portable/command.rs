//! Official command world resolved from the pinned upstream release graph.
//! WIT source provenance and license: docs/roadmap/wasi-wit-lock.json and
//! vendor/wasi/LICENSE.md. Binding shapes come from wit-parser, not a signature table.
use super::Diagnostic;
use sha2::{Digest, Sha256};
use wit_parser::{Resolve, UnresolvedPackageGroup, WorldId};

pub(super) mod memory;
mod shape;
mod adapter;
mod assembly;
mod exit;
pub use assembly::{component, component_with_exit};

const LOCK: &str = include_str!("../../../../docs/roadmap/wasi-wit-lock.json");
const PACKAGES: &[(&str, &str)] = &[
    (
        "cli/cli.wit",
        include_str!("../../../../vendor/wasi/wasi-wit-0.3.1/cli/cli.wit"),
    ),
    (
        "cli/deps/clocks.wit",
        include_str!("../../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/clocks.wit"),
    ),
    (
        "cli/deps/filesystem.wit",
        include_str!("../../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/filesystem.wit"),
    ),
    (
        "cli/deps/random.wit",
        include_str!("../../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/random.wit"),
    ),
    (
        "cli/deps/sockets.wit",
        include_str!("../../../../vendor/wasi/wasi-wit-0.3.1/cli/deps/sockets.wit"),
    ),
];

/// Resolve the complete official command world and dependency packages without
/// reading files at runtime. The embedded bytes must match the recorded release.
/// This resolves types; it does not itself implement capability adapters.
pub fn official_profile() -> Result<(Resolve, WorldId), Diagnostic> {
    let lock: serde_json::Value = serde_json::from_str(LOCK)
        .map_err(|error| diagnostic(format!("Invalid embedded WASI lock: {error}")))?;
    if lock["release"] != "v0.3.1" {
        return Err(diagnostic("Embedded command profile requires WASI v0.3.1"));
    }
    let mut groups = Vec::new();
    for (path, source) in PACKAGES {
        let expected = lock["files"][*path]["sha256"]
            .as_str()
            .ok_or_else(|| diagnostic(format!("Missing WASI source identity for {path}")))?;
        let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
        if actual != expected {
            return Err(diagnostic(format!(
                "Embedded WASI source identity mismatch: {path}"
            )));
        }
        groups.push(
            UnresolvedPackageGroup::parse(path, source).map_err(|(map, error)| {
                diagnostic(format!(
                    "Invalid embedded WIT {path}: {}",
                    error.render(&map)
                ))
            })?,
        );
    }
    let main = groups.remove(0);
    let mut resolve = Resolve::new();
    let package = resolve.push_groups(main, groups).map_err(|error| {
        diagnostic(format!(
            "Failed to resolve command dependency graph: {error}"
        ))
    })?;
    let world = resolve
        .select_world(&[package], Some("command"))
        .map_err(|error| {
            diagnostic(format!(
                "Failed to select official command world: {error:#}"
            ))
        })?;
    Ok((resolve, world))
}

fn diagnostic(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}
