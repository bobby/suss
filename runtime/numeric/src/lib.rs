//! Original primitive numeric conversions for the portable runtime.
//! Semantics: ECMAScript StringToNumber, and Java's `Double.toString`, which
//! Clojure prints doubles with. Decimal parsing uses Rust core's correctly
//! rounded, allocator-free parser; the shortest digits come from the
//! separately licensed ryu-js dependency.
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

/// Java's `Double.toString` layout of the shortest round-trip digits that
/// ryu-js gives: plain decimal with at least one fraction digit for
/// 10^-3 <= |v| < 10^7 (`1.0`, `0.001`, `1234567.0`), and otherwise one digit,
/// a fraction and an exponent (`1.0E21`, `1.2345678E7`, `1.0E-4`). Clojure
/// prints doubles this way (ADR-0003). Java also widens a one-digit result to
/// the closest two digits, which differs only for some subnormals
/// (`4.9E-324` where this gives `5.0E-324`).
fn java(value: f64, js: &[u8], out: &mut [u8; 32]) -> usize {
    let mut length = 0;
    let mut push = |byte: u8, length: &mut usize| {
        out[*length] = byte;
        *length += 1;
    };
    if value.is_nan() {
        for &byte in b"NaN" {
            push(byte, &mut length);
        }
        return length;
    }
    if value.is_sign_negative() {
        push(b'-', &mut length);
    }
    if value.is_infinite() {
        for &byte in b"Infinity" {
            push(byte, &mut length);
        }
        return length;
    }
    if value == 0.0 {
        for &byte in b"0.0" {
            push(byte, &mut length);
        }
        return length;
    }
    // Significant digits d1 d2 ... dn and exponent e, value = d1.d2...dn × 10^e.
    let js = js.strip_prefix(b"-").unwrap_or(js);
    let (mantissa, exponent) = match js.iter().position(|&byte| byte == b'e') {
        Some(at) => {
            let (sign, digits) = match js[at + 1] {
                b'-' => (-1, &js[at + 2..]),
                b'+' => (1, &js[at + 2..]),
                _ => (1, &js[at + 1..]),
            };
            let magnitude = digits.iter().fold(0i32, |n, &d| n * 10 + i32::from(d - b'0'));
            (&js[..at], Some(sign * magnitude))
        }
        None => (js, None),
    };
    let point = mantissa.iter().position(|&byte| byte == b'.').unwrap_or(mantissa.len());
    // Leading zeros, the one before the point included, aren't significant.
    let mut digits = [0u8; 24];
    let mut count = 0;
    let mut leading = 0i32;
    for &byte in mantissa {
        if byte == b'.' {
            continue;
        }
        if count == 0 && byte == b'0' {
            leading += 1;
        } else {
            digits[count] = byte;
            count += 1;
        }
    }
    while count > 1 && digits[count - 1] == b'0' {
        count -= 1;
    }
    let exponent = match exponent {
        // ryu-js writes one digit before the point of an exponent form.
        Some(exponent) => exponent,
        // 0.000123 is 1.23 × 10^-4: minus its leading zeros, 0. included.
        None if mantissa[0] == b'0' => -leading,
        // 123.45 is 1.2345 × 10^2.
        None => point as i32 - 1,
    };
    let digits = &digits[..count];
    if (-3..7).contains(&exponent) {
        if exponent >= 0 {
            let whole = exponent as usize + 1;
            for index in 0..whole {
                push(*digits.get(index).unwrap_or(&b'0'), &mut length);
            }
            push(b'.', &mut length);
            if digits.len() > whole {
                for &digit in &digits[whole..] {
                    push(digit, &mut length);
                }
            } else {
                push(b'0', &mut length);
            }
        } else {
            push(b'0', &mut length);
            push(b'.', &mut length);
            for _ in 0..(-exponent - 1) {
                push(b'0', &mut length);
            }
            for &digit in digits {
                push(digit, &mut length);
            }
        }
    } else {
        push(digits[0], &mut length);
        push(b'.', &mut length);
        if digits.len() > 1 {
            for &digit in &digits[1..] {
                push(digit, &mut length);
            }
        } else {
            push(b'0', &mut length);
        }
        push(b'E', &mut length);
        if exponent < 0 {
            push(b'-', &mut length);
        }
        let mut magnitude = exponent.unsigned_abs();
        let mut text = [0u8; 4];
        let mut width = 0;
        loop {
            text[width] = b'0' + (magnitude % 10) as u8;
            width += 1;
            magnitude /= 10;
            if magnitude == 0 {
                break;
            }
        }
        for index in (0..width).rev() {
            push(text[index], &mut length);
        }
    }
    length
}

/// Caller provides at least 32 bytes of scratch; output is ASCII, not GC storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn suss_format_number(value: f64, output: *mut u8) -> usize {
    let mut buffer = ryu_js::Buffer::new();
    let text = buffer.format(value);
    let mut java_text = [0u8; 32];
    let length = java(value, text.as_bytes(), &mut java_text);
    unsafe { core::ptr::copy_nonoverlapping(java_text.as_ptr(), output, length) };
    length
}
