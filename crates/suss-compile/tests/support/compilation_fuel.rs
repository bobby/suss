//! Compilation allowance for semantic integration corpora, independent of Runtime fuel.
//!
//! Initial calibration: the complete core-environment fixture's largest traced
//! request consumed about 2.631 billion units (see the private-slot complete-cost
//! evidence dated 2026-10-18). Ten billion provides headroom without per-call
//! replenishment or retries. Remaining corpora must still execute and retain all
//! original assertions; this constant is not a performance target or a shipped
//! default. Dedicated default-budget and exhaustion tests use their own limits.
pub const COMPILATION_FUEL: u64 = 10_000_000_000;
