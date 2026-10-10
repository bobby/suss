// Copyright (c) Rich Hickey. All rights reserved.
// SPDX-License-Identifier: EPL-1.0
// Adapted complete scalar-name branch/data from the pinned source receipts in
// docs/compatibility/record-analyzer-context. Full analyzer-map munge is separate.
//! UTF16 compiler string/symbol name munging; no scalar-to-string coercion.
const CHAR_MAP: &[(&[u16], &[u16])] = &[
    (&[45], &[95]),
    (&[58], &[95, 67, 79, 76, 79, 78, 95]),
    (&[43], &[95, 80, 76, 85, 83, 95]),
    (&[62], &[95, 71, 84, 95]),
    (&[60], &[95, 76, 84, 95]),
    (&[61], &[95, 69, 81, 95]),
    (&[126], &[95, 84, 73, 76, 68, 69, 95]),
    (&[33], &[95, 66, 65, 78, 71, 95]),
    (&[64], &[95, 67, 73, 82, 67, 65, 95]),
    (&[35], &[95, 83, 72, 65, 82, 80, 95]),
    (&[39], &[95, 83, 73, 78, 71, 76, 69, 81, 85, 79, 84, 69, 95]),
    (
        &[92, 34],
        &[95, 68, 79, 85, 66, 76, 69, 81, 85, 79, 84, 69, 95],
    ),
    (&[37], &[95, 80, 69, 82, 67, 69, 78, 84, 95]),
    (&[94], &[95, 67, 65, 82, 69, 84, 95]),
    (&[38], &[95, 65, 77, 80, 69, 82, 83, 65, 78, 68, 95]),
    (&[42], &[95, 83, 84, 65, 82, 95]),
    (&[124], &[95, 66, 65, 82, 95]),
    (&[123], &[95, 76, 66, 82, 65, 67, 69, 95]),
    (&[125], &[95, 82, 66, 82, 65, 67, 69, 95]),
    (&[91], &[95, 76, 66, 82, 65, 67, 75, 95]),
    (&[93], &[95, 82, 66, 82, 65, 67, 75, 95]),
    (&[47], &[95, 83, 76, 65, 83, 72, 95]),
    (&[92, 92], &[95, 66, 83, 76, 65, 83, 72, 95]),
    (&[63], &[95, 81, 77, 65, 82, 75, 95]),
];
const RESERVED: &[&str] = &[
    "arguments",
    "abstract",
    "await",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "export",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "function",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "int",
    "interface",
    "let",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "typeof",
    "var",
    "void",
    "volatile",
    "while",
    "with",
    "yield",
    "methods",
    "null",
    "constructor",
];

/// Complete compiler.cljc scalar name path: dots, division, reserved segments,
/// then every CHAR_MAP entry. Does not pretend to implement map/shadow munging.
pub fn munge_name(input: &[u16]) -> Vec<u16> {
    munge_name_with_reserved(input, |part| {
        RESERVED
            .iter()
            .any(|name| name.encode_utf16().eq(part.iter().copied()))
    })
}

/// Scalar two-arity path. The caller supplies the exact `get`-is-non-nil fact
/// for its reserved table; false-valued entries still count as reserved.
pub fn munge_name_with_reserved(
    input: &[u16],
    mut is_reserved: impl FnMut(&[u16]) -> bool,
) -> Vec<u16> {
    let mut dots = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i..].starts_with(&[46, 46]) {
            dots.extend("_DOT__DOT_".encode_utf16());
            i += 2;
        } else {
            dots.push(input[i]);
            i += 1;
        }
    }
    let mut divided = Vec::new();
    i = 0;
    while i < dots.len() {
        // JS RegExp . matches one UTF16 unit except the four line terminators.
        if dots[i] == 47 && i + 1 < dots.len() && !matches!(dots[i + 1], 10 | 13 | 0x2028 | 0x2029)
        {
            divided.push(46);
            divided.push(dots[i + 1]);
            i += 2;
        } else {
            divided.push(dots[i]);
            i += 1;
        }
    }
    let mut segmented = Vec::new();
    for (index, part) in divided.split(|c| *c == 46).enumerate() {
        if index != 0 {
            segmented.push(46);
        }
        segmented.extend_from_slice(part);
        if is_reserved(part) {
            segmented.push(36);
        }
    }
    let mut munged = Vec::new();
    for unit in segmented {
        if let Some((_, replacement)) = CHAR_MAP.iter().find(|(key, _)| *key == [unit]) {
            munged.extend_from_slice(replacement);
        } else {
            munged.push(unit);
        }
    }
    munged
}
