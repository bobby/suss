//! Compilation allowance for semantic correctness corpora, independent of Runtime fuel.
//!
//! Initial 10B calibration covered a measured 2.631B core-environment expansion.
//! Later source-qualified measurements reached 14,565,794,779 units for one
//! caller-environment interval and about 18.8B for a named-function environment.
//! Use 100B of compilation headroom under #252; this is not a sufficiency bound,
//! a performance target, a shipped default, or a per-helper budget refill.
//! Caller Runtime and dedicated default/exhaustion/IRQ/recovery limits stay separate.
pub const COMPILATION_FUEL: u64 = 100_000_000_000;
