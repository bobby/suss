//! Original primitive numeric conversions for the portable runtime.
//! Semantics: pinned ClojureScript arithmetic and ECMAScript StringToNumber.
//! Decimal parsing uses Rust core's correctly rounded, allocator-free parser;
//! decimal formatting uses the separately licensed ryu-js dependency.
#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

fn whitespace(unit: u16) -> bool {
    matches!(unit, 0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 |
        0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

fn radix_number(bytes: &[u8], width: u32) -> f64 {
    let radix = 1 << width;
    let mut mantissa = 0u64;
    let mut bits = 0u64;
    let mut guard = false;
    let mut sticky = false;
    for &byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return f64::NAN,
        };
        if u32::from(digit) >= radix {
            return f64::NAN;
        }
        for shift in (0..width).rev() {
            let bit = (digit >> shift) & 1;
            if bits == 0 && bit == 0 {
                continue;
            }
            bits += 1;
            if bits <= 53 {
                mantissa = (mantissa << 1) | u64::from(bit);
            } else if bits == 54 {
                guard = bit != 0;
            } else {
                sticky |= bit != 0;
            }
        }
    }
    if bits <= 53 {
        return mantissa as f64;
    }
    if guard && (sticky || mantissa & 1 != 0) {
        mantissa += 1;
    }
    if mantissa == 1 << 53 {
        mantissa >>= 1;
        bits += 1;
    }
    if bits > 1024 {
        return f64::INFINITY;
    }
    f64::from_bits(((bits - 1 + 1023) << 52) | (mantissa & ((1 << 52) - 1)))
}

fn decimal(bytes: &[u8]) -> bool {
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = index;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    let mut digits = index - start;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        digits += index - start;
    }
    if digits == 0 {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    index == bytes.len()
}

/// The bridge provides aligned scratch memory containing `len` UTF-16 units.
/// This function may compact that scratch in place; it never mutates GC strings.
/// It has no allocator or retained Rust state. Invalid numeric text returns NaN.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn suss_parse_number(ptr: *mut u16, len: usize) -> f64 {
    let mut start = 0;
    let mut end = len;
    while start < end && whitespace(unsafe { ptr.add(start).read() }) {
        start += 1;
    }
    while end > start && whitespace(unsafe { ptr.add(end - 1).read() }) {
        end -= 1;
    }
    if start == end {
        return 0.0;
    }
    let output = ptr.cast::<u8>();
    for i in start..end {
        let unit = unsafe { ptr.add(i).read() };
        if unit > 0x7f {
            return f64::NAN;
        }
        // Destination always precedes every unread UTF-16 source unit.
        unsafe { output.add(i - start).write(unit as u8) };
    }
    let bytes = unsafe { core::slice::from_raw_parts(output, end - start) };
    match bytes {
        b"Infinity" | b"+Infinity" => return f64::INFINITY,
        b"-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if bytes.len() > 2 && bytes[0] == b'0' {
        let width = match bytes[1] {
            b'x' | b'X' => Some(4),
            b'o' | b'O' => Some(3),
            b'b' | b'B' => Some(1),
            _ => None,
        };
        if let Some(width) = width {
            return radix_number(&bytes[2..], width);
        }
    }
    if !decimal(bytes) {
        return f64::NAN;
    }
    // Every byte was checked ASCII, and grammar excludes Rust-only inf/NaN.
    let text = unsafe { core::str::from_utf8_unchecked(bytes) };
    text.parse().unwrap_or(f64::NAN)
}

/// Caller provides at least 32 bytes of scratch; output is ASCII, not GC storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn suss_format_number(value: f64, output: *mut u8) -> usize {
    let mut buffer = ryu_js::Buffer::new();
    let text = buffer.format(value);
    unsafe { core::ptr::copy_nonoverlapping(text.as_ptr(), output, text.len()) };
    text.len()
}
