//! Original lossless UUID data and UTF-16 parsing, following executed
//! Clojure 1.12.6 / OpenJDK 21 behavior. No Java code or runtime is shipped.
//! The reference commands, bytecode, complete BMP digit scan and error-unit
//! observations are retained under results/m12-reader-followup.
use std::fmt;

/// Immutable signed halves, in java.util.UUID's comparison order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uuid {
    pub most: i64,
    pub least: i64,
}

/// The message stays UTF-16, including invalid input's lone surrogates. A
/// frontend requiring UTF-8 can use Display's explicitly lossy projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: Vec<u16>,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf16_lossy(&self.message))
    }
}
impl std::error::Error for Error {}

/// Generated behavioral ranges: Character.digit(char, 16), not Unicode's
/// general numeric property. Supplementary code points are two invalid units.
pub const DECIMAL_STARTS: [u16; 37] = [
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664, 3792,
    3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528, 43216,
    43264, 43472, 43504, 43600, 44016, 65296,
];

pub fn hex_digit(unit: u16) -> Option<u8> {
    for start in DECIMAL_STARTS {
        if unit >= start && unit - start < 10 {
            return Some((unit - start) as u8);
        }
    }
    for start in [65, 97, 65313, 65345] {
        if unit >= start && unit - start < 6 {
            return Some((unit - start + 10) as u8);
        }
    }
    None
}

fn message(text: &str) -> Error {
    Error {
        message: text.encode_utf16().collect(),
    }
}
fn group_error(group: &[u16], index: usize) -> Error {
    let mut error = message(&format!("Error at index {index} in: \""));
    error.message.extend_from_slice(group);
    error.message.push(34);
    error
}
fn group_value(group: &[u16]) -> Result<u64, Error> {
    // Long.parseLong(CharSequence, begin, end, 16) uses an empty message for
    // an empty group, and retains a leading plus in error-index accounting.
    if group.is_empty() {
        return Err(message(""));
    }
    let first = usize::from(group[0] == 43);
    if first == group.len() {
        return Err(group_error(group, first));
    }
    let mut value = 0u64;
    for (index, &unit) in group.iter().enumerate().skip(first) {
        let digit = hex_digit(unit).ok_or_else(|| group_error(group, index))?;
        value = value
            .checked_mul(16)
            .and_then(|n| n.checked_add(u64::from(digit)))
            .filter(|&n| n <= i64::MAX as u64)
            .ok_or_else(|| group_error(group, index))?;
    }
    Ok(value)
}

impl Uuid {
    pub fn parse(units: &[u16]) -> Result<Self, Error> {
        if units.len() > 36 {
            return Err(message("UUID string too large"));
        }
        let groups = units.split(|&unit| unit == 45).collect::<Vec<_>>();
        if groups.len() != 5 {
            let mut error = message("Invalid UUID string: ");
            error.message.extend_from_slice(units);
            return Err(error);
        }
        let mut parts = [0u64; 5];
        // Validate and check each signed-long magnitude before masking its
        // field width. Wider accepted groups are not silently rejected.
        for (part, group) in parts.iter_mut().zip(groups) {
            *part = group_value(group)?;
        }
        let most =
            ((parts[0] & 0xffff_ffff) << 32) | ((parts[1] & 0xffff) << 16) | (parts[2] & 0xffff);
        let least = ((parts[3] & 0xffff) << 48) | (parts[4] & 0xffff_ffff_ffff);
        Ok(Self {
            most: most as i64,
            least: least as i64,
        })
    }
    pub fn hash_code(self) -> i32 {
        let bits = (self.most ^ self.least) as u64;
        (bits as u32 ^ (bits >> 32) as u32) as i32
    }
}
impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let most = self.most as u64;
        let least = self.least as u64;
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            most >> 32,
            (most >> 16) & 0xffff,
            most & 0xffff,
            least >> 48,
            least & 0xffff_ffff_ffff
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case", deny_unknown_fields)]
    struct Primary {
        units: Vec<u16>,
        bits: Option<[i64; 2]>,
        canonical: Option<String>,
        hash: Option<i32>,
        error_units: Option<Vec<u16>>,
    }
    #[test]
    fn parser_matches_fresh_primary_bits_hashes_canonical_text_and_lossless_errors() {
        let rows: Vec<Primary> = crate::edn::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../results/m12-reader-followup/uuid-parser-cases.edn"
        )))
        .unwrap();
        assert!(rows.len() > 200);
        let mut accepted = 0;
        let mut rejected = 0;
        for row in rows {
            match (Uuid::parse(&row.units), row.bits) {
                (Ok(value), Some(bits)) => {
                    accepted += 1;
                    assert_eq!([value.most, value.least], bits, "{:?}", row.units);
                    assert_eq!(Some(value.hash_code()), row.hash);
                    assert_eq!(Some(value.to_string()), row.canonical);
                    assert!(row.error_units.is_none());
                }
                (Err(error), None) => {
                    rejected += 1;
                    assert_eq!(Some(error.message), row.error_units, "{:?}", row.units);
                    assert!(row.hash.is_none() && row.canonical.is_none());
                }
                (result, bits) => panic!("Input {:?}: {result:?} versus JVM {bits:?}", row.units),
            }
        }
        assert!(accepted > 100 && rejected > 20);
    }
    #[test]
    fn every_utf16_hex_digit_matches_the_executed_jvm_scan() {
        let primary = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../results/m12-reader-followup/uuid-hex-digit-jvm.bin"
        ));
        assert_eq!(primary.len(), 65536);
        for unit in 0..=u16::MAX {
            assert_eq!(
                hex_digit(unit).map_or(0, |n| n + 1),
                primary[usize::from(unit)],
                "{unit:04x}"
            );
        }
    }
    #[test]
    fn ordering_compares_each_signed_half_without_subtraction_overflow() {
        let parse = |text: &str| Uuid::parse(&text.encode_utf16().collect::<Vec<_>>()).unwrap();
        assert!(
            parse("80000000-0000-0000-0000-000000000000")
                < parse("7fffffff-ffff-ffff-ffff-ffffffffffff")
        );
        assert!(
            parse("00000000-0000-0000-8000-000000000000")
                < parse("00000000-0000-0000-7fff-ffffffffffff")
        );
    }
    #[test]
    fn native_edn_preserves_uuid_values_canonical_keys_and_reader_precedence() {
        use crate::{
            Symbol,
            edn::{self, Options, Value},
        };
        use std::collections::BTreeMap;
        let canonical = "#uuid \"00000001-0002-0003-0004-000000000005\"";
        let value = edn::read_one("#uuid \"1-2-3-4-5\"", &Options::default()).unwrap();
        let Value::Uuid(bits) = &value else {
            panic!("typed UUID data")
        };
        assert_eq!([bits.most, bits.least], [0x100020003, 0x4000000000005]);
        assert_eq!(edn::print(&value).unwrap(), canonical);
        let projected = edn::source::value_to_form(&value);
        assert_eq!(edn::source::form_to_value(&projected).unwrap(), value);
        assert_eq!(
            edn::read_one(canonical, &Options::default()).unwrap(),
            value
        );
        assert_eq!(
            edn::read_one(
                "#uuid  ;comment\n #_skip \"+1-+2-+3-+4-+5\"",
                &Options::default()
            )
            .unwrap(),
            value
        );
        for source in [
            "#{#uuid \"1-2-3-4-5\" #uuid \"00000001-0002-0003-0004-000000000005\"}",
            "{#uuid \"1-2-3-4-5\" 1 #uuid \"00000001-0002-0003-0004-000000000005\" 2}",
        ] {
            assert!(
                edn::read_one(source, &Options::default()).is_err(),
                "canonical duplicate: {source}"
            );
        }
        assert!(edn::read_one("^:tag #uuid \"1-2-3-4-5\"", &Options::default()).is_err());
        assert!(
            edn::print(&Value::Meta(
                Box::new(value.clone()),
                BTreeMap::from([(Value::keyword("tag"), Value::Bool(true))])
            ))
            .is_err()
        );
        let mut options = Options::preserving_tags();
        options.readers.insert(
            Symbol::new("uuid"),
            Box::new(|_| Ok(Value::keyword("override"))),
        );
        assert_eq!(
            edn::read_one("#uuid 0", &options).unwrap(),
            Value::keyword("override")
        );
        options.readers.clear();
        options.default = Some(Box::new(|_, _| Ok(Value::keyword("default"))));
        assert_eq!(edn::read_one(canonical, &options).unwrap(), value);
        assert_eq!(
            edn::read_one("#other \"x\"", &options).unwrap(),
            Value::keyword("default")
        );
        assert_eq!(
            edn::read_one("#uuid 0", &Options::default())
                .unwrap_err()
                .message,
            "#uuid data reader expected string"
        );
        assert!(
            edn::from_value::<String>(value)
                .unwrap_err()
                .message
                .contains("UUID")
        );
    }
}
