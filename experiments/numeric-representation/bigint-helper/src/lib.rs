//! The helper side of the BigInt placement measurement (#338): magnitude
//! addition and schoolbook multiplication over little-endian 32-bit limbs in
//! linear memory, the same algorithms as `src/bigint.wat`'s GC kernels.
#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

/// `out` = `a` + `b`; returns the result's length without leading zeros.
///
/// # Safety
/// The slices are valid and `out` holds `max(la, lb) + 1` limbs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn add(a: *const u32, la: usize, b: *const u32, lb: usize, out: *mut u32) -> usize {
    let (a, b) = unsafe { (core::slice::from_raw_parts(a, la), core::slice::from_raw_parts(b, lb)) };
    let (long, short) = if la >= lb { (a, b) } else { (b, a) };
    let out = unsafe { core::slice::from_raw_parts_mut(out, long.len() + 1) };
    let mut carry = 0u64;
    for i in 0..long.len() {
        let sum = long[i] as u64 + short.get(i).copied().unwrap_or(0) as u64 + carry;
        out[i] = sum as u32;
        carry = sum >> 32;
    }
    out[long.len()] = carry as u32;
    trim(out)
}

/// `out` = `a` × `b`; returns the result's length without leading zeros.
///
/// # Safety
/// The slices are valid and `out` holds `la + lb` limbs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mul(a: *const u32, la: usize, b: *const u32, lb: usize, out: *mut u32) -> usize {
    let (a, b) = unsafe { (core::slice::from_raw_parts(a, la), core::slice::from_raw_parts(b, lb)) };
    let out = unsafe { core::slice::from_raw_parts_mut(out, la + lb) };
    // Short outer loop amortizes row setup and the invariant input-limb read.
    let (a, b) = if la <= lb { (a, b) } else { (b, a) };
    out.fill(0);
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = out[i + j] as u64 + x as u64 * y as u64 + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        out[i + b.len()] = carry as u32;
    }
    trim(out)
}

fn trim(out: &[u32]) -> usize {
    out.iter().rposition(|&limb| limb != 0).map_or(0, |last| last + 1)
}
