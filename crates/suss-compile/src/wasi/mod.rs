//! Bundled WASI WIT definitions
//!
//! This module embeds WASI 0.2.4 WIT definitions at compile time,
//! eliminating the need for users to set up a deps/ folder.

use crate::ir::Type;

/// Bundled WASI version
pub const WASI_VERSION: &str = "0.2.4";

// ============================================================================
// WASI Function Registry
// ============================================================================

/// A known WASI function with its type signature
#[derive(Debug, Clone)]
pub struct WasiFunctionInfo {
    /// The WIT interface path (e.g., "wasi:random/random@0.2.0")
    pub wit_interface: String,
    /// Function name in the interface
    pub function_name: String,
    /// Parameter types
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
}

/// Parse a WASI call symbol like "wasi.random/get-random-u64"
/// Returns Some(WasiFunctionInfo) if this is a known WASI function
pub fn parse_wasi_call(symbol: &str) -> Option<WasiFunctionInfo> {
    // Expected format: wasi.PACKAGE/FUNCTION or wasi.PACKAGE.INTERFACE/FUNCTION
    if !symbol.starts_with("wasi.") {
        return None;
    }

    let rest = &symbol[5..]; // Skip "wasi."
    let (package_part, func_name) = rest.split_once('/')?;

    // Simple case: wasi.PACKAGE/FUNCTION -> interface is same as package
    // Complex case: wasi.PACKAGE.INTERFACE/FUNCTION (not implemented yet)
    let package = package_part;
    let interface = package; // For now, assume interface = package

    lookup_wasi_function(package, interface, func_name)
}

/// Look up a known WASI function
fn lookup_wasi_function(package: &str, interface: &str, func_name: &str) -> Option<WasiFunctionInfo> {
    match (package, interface, func_name) {
        // wasi:random/random
        ("random", "random", "get-random-u64") => Some(WasiFunctionInfo {
            wit_interface: "wasi:random/random@0.2.4".to_string(),
            function_name: "get-random-u64".to_string(),
            params: vec![],
            return_type: Type::I64, // u64 maps to i64 in WASM
        }),
        ("random", "random", "get-random-bytes") => Some(WasiFunctionInfo {
            wit_interface: "wasi:random/random@0.2.4".to_string(),
            function_name: "get-random-bytes".to_string(),
            params: vec![Type::I64], // len: u64
            return_type: Type::String, // list<u8> represented as ptr+len
        }),

        // wasi:clocks/wall-clock
        ("clocks", "clocks", "now") => Some(WasiFunctionInfo {
            wit_interface: "wasi:clocks/wall-clock@0.2.4".to_string(),
            function_name: "now".to_string(),
            params: vec![],
            return_type: Type::I64, // datetime as i64 nanoseconds (simplified)
        }),

        // Add more WASI functions as needed
        _ => None,
    }
}

/// Collect all WASI calls from an expression
pub fn collect_wasi_calls(expr: &suss_core::Edn) -> Vec<WasiFunctionInfo> {
    let mut calls = Vec::new();
    collect_wasi_calls_recursive(expr, &mut calls);
    // Deduplicate by wit_interface + function_name
    calls.sort_by(|a, b| {
        (&a.wit_interface, &a.function_name).cmp(&(&b.wit_interface, &b.function_name))
    });
    calls.dedup_by(|a, b| {
        a.wit_interface == b.wit_interface && a.function_name == b.function_name
    });
    calls
}

fn collect_wasi_calls_recursive(expr: &suss_core::Edn, calls: &mut Vec<WasiFunctionInfo>) {
    use suss_core::Edn;

    match expr {
        Edn::List(items) if !items.is_empty() => {
            // Check if the first item is a WASI call
            if let Edn::Symbol(sym) = &items[0] {
                // Handle namespaced symbols (e.g., namespace="wasi.random", name="get-random-u64")
                let full_name = if let Some(ns) = &sym.namespace {
                    format!("{}/{}", ns, sym.name)
                } else {
                    sym.name.clone()
                };
                if let Some(info) = parse_wasi_call(&full_name) {
                    calls.push(info);
                }
            }
            // Recurse into all items
            for item in items {
                collect_wasi_calls_recursive(item, calls);
            }
        }
        Edn::Vector(items) => {
            for item in items {
                collect_wasi_calls_recursive(item, calls);
            }
        }
        Edn::Map(pairs) => {
            for (k, v) in pairs {
                collect_wasi_calls_recursive(k, calls);
                collect_wasi_calls_recursive(v, calls);
            }
        }
        Edn::Set(items) => {
            for item in items {
                collect_wasi_calls_recursive(item, calls);
            }
        }
        _ => {}
    }
}

// ============================================================================
// wasi:random
// ============================================================================

pub mod random {
    pub const RANDOM: &str = include_str!("random/random.wit");
    pub const INSECURE: &str = include_str!("random/insecure.wit");
    pub const INSECURE_SEED: &str = include_str!("random/insecure-seed.wit");
    pub const WORLD: &str = include_str!("random/world.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("random/random.wit", RANDOM),
            ("random/insecure.wit", INSECURE),
            ("random/insecure-seed.wit", INSECURE_SEED),
            ("random/world.wit", WORLD),
        ]
    }
}

// ============================================================================
// wasi:cli
// ============================================================================

pub mod cli {
    pub const COMMAND: &str = include_str!("cli/command.wit");
    pub const ENVIRONMENT: &str = include_str!("cli/environment.wit");
    pub const EXIT: &str = include_str!("cli/exit.wit");
    pub const IMPORTS: &str = include_str!("cli/imports.wit");
    pub const RUN: &str = include_str!("cli/run.wit");
    pub const STDIO: &str = include_str!("cli/stdio.wit");
    pub const TERMINAL: &str = include_str!("cli/terminal.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("cli/command.wit", COMMAND),
            ("cli/environment.wit", ENVIRONMENT),
            ("cli/exit.wit", EXIT),
            ("cli/imports.wit", IMPORTS),
            ("cli/run.wit", RUN),
            ("cli/stdio.wit", STDIO),
            ("cli/terminal.wit", TERMINAL),
        ]
    }

    /// Interface-only files (excludes worlds with cross-package includes)
    /// Use this when loading CLI as a dependency without pulling in all WASI packages
    pub fn interface_files() -> &'static [(&'static str, &'static str)] {
        &[
            // command.wit and imports.wit are excluded because they have
            // `include wasi:*` statements that require all other packages
            ("cli/environment.wit", ENVIRONMENT),
            ("cli/exit.wit", EXIT),
            ("cli/run.wit", RUN),
            ("cli/stdio.wit", STDIO),
            ("cli/terminal.wit", TERMINAL),
        ]
    }
}

// ============================================================================
// wasi:clocks
// ============================================================================

pub mod clocks {
    pub const MONOTONIC_CLOCK: &str = include_str!("clocks/monotonic-clock.wit");
    pub const TIMEZONE: &str = include_str!("clocks/timezone.wit");
    pub const WALL_CLOCK: &str = include_str!("clocks/wall-clock.wit");
    pub const WORLD: &str = include_str!("clocks/world.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("clocks/monotonic-clock.wit", MONOTONIC_CLOCK),
            ("clocks/timezone.wit", TIMEZONE),
            ("clocks/wall-clock.wit", WALL_CLOCK),
            ("clocks/world.wit", WORLD),
        ]
    }
}

// ============================================================================
// wasi:io
// ============================================================================

pub mod io {
    pub const ERROR: &str = include_str!("io/error.wit");
    pub const POLL: &str = include_str!("io/poll.wit");
    pub const STREAMS: &str = include_str!("io/streams.wit");
    pub const WORLD: &str = include_str!("io/world.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("io/error.wit", ERROR),
            ("io/poll.wit", POLL),
            ("io/streams.wit", STREAMS),
            ("io/world.wit", WORLD),
        ]
    }
}

// ============================================================================
// wasi:filesystem
// ============================================================================

pub mod filesystem {
    pub const PREOPENS: &str = include_str!("filesystem/preopens.wit");
    pub const TYPES: &str = include_str!("filesystem/types.wit");
    pub const WORLD: &str = include_str!("filesystem/world.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("filesystem/preopens.wit", PREOPENS),
            ("filesystem/types.wit", TYPES),
            ("filesystem/world.wit", WORLD),
        ]
    }
}

// ============================================================================
// wasi:sockets
// ============================================================================

pub mod sockets {
    pub const INSTANCE_NETWORK: &str = include_str!("sockets/instance-network.wit");
    pub const IP_NAME_LOOKUP: &str = include_str!("sockets/ip-name-lookup.wit");
    pub const NETWORK: &str = include_str!("sockets/network.wit");
    pub const TCP_CREATE_SOCKET: &str = include_str!("sockets/tcp-create-socket.wit");
    pub const TCP: &str = include_str!("sockets/tcp.wit");
    pub const UDP_CREATE_SOCKET: &str = include_str!("sockets/udp-create-socket.wit");
    pub const UDP: &str = include_str!("sockets/udp.wit");
    pub const WORLD: &str = include_str!("sockets/world.wit");

    pub fn all_files() -> &'static [(&'static str, &'static str)] {
        &[
            ("sockets/instance-network.wit", INSTANCE_NETWORK),
            ("sockets/ip-name-lookup.wit", IP_NAME_LOOKUP),
            ("sockets/network.wit", NETWORK),
            ("sockets/tcp-create-socket.wit", TCP_CREATE_SOCKET),
            ("sockets/tcp.wit", TCP),
            ("sockets/udp-create-socket.wit", UDP_CREATE_SOCKET),
            ("sockets/udp.wit", UDP),
            ("sockets/world.wit", WORLD),
        ]
    }
}

// ============================================================================
// Package lookup
// ============================================================================

/// Get all WIT files for a given WASI package name
pub fn get_package_files(package: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match package {
        "random" => Some(random::all_files()),
        "cli" => Some(cli::all_files()),
        "clocks" => Some(clocks::all_files()),
        "io" => Some(io::all_files()),
        "filesystem" => Some(filesystem::all_files()),
        "sockets" => Some(sockets::all_files()),
        _ => None,
    }
}


/// Combine all WIT files for a package into a single content string
/// The package declaration is only included once
///
/// For CLI, this uses interface_files() to avoid loading worlds with
/// cross-package `include` statements that would require all WASI packages.
pub fn get_combined_package(package: &str) -> Option<String> {
    // For CLI, use interface-only files to avoid cross-package includes
    let files = if package == "cli" {
        cli::interface_files()
    } else {
        get_package_files(package)?
    };

    let mut combined = String::new();
    let mut seen_package_decl = false;

    // For CLI, we need to add the package declaration since interface files don't have it
    if package == "cli" {
        combined.push_str("package wasi:cli@0.2.4;\n\n");
        seen_package_decl = true;
    }

    for (_, content) in files {
        for line in content.lines() {
            // Skip duplicate package declarations
            if line.trim().starts_with("package wasi:") {
                if seen_package_decl {
                    continue;
                }
                seen_package_decl = true;
            }
            combined.push_str(line);
            combined.push('\n');
        }
        combined.push('\n');
    }

    Some(combined)
}

/// Detect which WASI packages are needed based on world.wit content
pub fn detect_needed_packages(wit_content: &str) -> Vec<&'static str> {
    let mut needed = Vec::new();

    // Check for wasi:* imports in the WIT content
    if wit_content.contains("wasi:random") {
        needed.push("random");
    }
    if wit_content.contains("wasi:cli") {
        needed.push("cli");
        // cli depends on io
        if !wit_content.contains("wasi:io") {
            needed.push("io");
        }
    }
    if wit_content.contains("wasi:clocks") {
        needed.push("clocks");
        // clocks depends on io
        if !wit_content.contains("wasi:io") {
            needed.push("io");
        }
    }
    if wit_content.contains("wasi:io") {
        needed.push("io");
    }
    if wit_content.contains("wasi:filesystem") {
        needed.push("filesystem");
        // filesystem depends on io and clocks
        if !wit_content.contains("wasi:io") {
            needed.push("io");
        }
        if !wit_content.contains("wasi:clocks") {
            needed.push("clocks");
        }
    }
    if wit_content.contains("wasi:sockets") {
        needed.push("sockets");
        // sockets depends on io and clocks
        if !wit_content.contains("wasi:io") {
            needed.push("io");
        }
        if !wit_content.contains("wasi:clocks") {
            needed.push("clocks");
        }
    }

    // Deduplicate
    needed.sort();
    needed.dedup();
    needed
}
