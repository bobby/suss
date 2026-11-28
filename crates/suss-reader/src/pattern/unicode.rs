#[path = "properties.rs"]
mod properties;
const NAMES: &[u8] = include_bytes!("unicode-names.bin");
const BLOCK: usize = 32;

fn word(at: usize) -> usize {
    u32::from_le_bytes(NAMES[at..at + 4].try_into().unwrap()) as usize
}
fn block_bounds(block: usize) -> (usize, usize) {
    let base = 8 + 4 * (word(4) + 1);
    (base + word(8 + 4 * block), base + word(12 + 4 * block))
}
fn next_name(at: &mut usize, name: &mut Vec<u8>) -> u32 {
    let prefix = NAMES[*at] as usize;
    let count = NAMES[*at + 1] as usize;
    *at += 2;
    name.truncate(prefix);
    name.extend_from_slice(&NAMES[*at..*at + count]);
    *at += count;
    let point = word(*at) as u32;
    *at += 4;
    point
}
pub(super) fn character_name(name: &str) -> Option<u32> {
    let name = name.trim_matches(|c: char| c <= '\u{20}').to_uppercase();
    let name = name.as_bytes();
    let mut lo = 0;
    let mut hi = word(4);
    let mut scratch = Vec::new();
    while lo < hi {
        let middle = (lo + hi) / 2;
        let (mut at, _) = block_bounds(middle);
        scratch.clear();
        next_name(&mut at, &mut scratch);
        if scratch.as_slice() <= name {
            lo = middle + 1;
        } else {
            hi = middle;
        }
    }
    if lo == 0 {
        return None;
    }
    let (mut at, end) = block_bounds(lo - 1);
    scratch.clear();
    for _ in 0..BLOCK {
        if at >= end {
            break;
        }
        let point = next_name(&mut at, &mut scratch);
        match scratch.as_slice().cmp(name) {
            std::cmp::Ordering::Equal => return Some(point),
            std::cmp::Ordering::Greater => return None,
            std::cmp::Ordering::Less => {}
        }
    }
    None
}
fn contains(values: &[&str], text: &str) -> bool {
    values.binary_search(&text).is_ok()
}
pub(super) fn property(name: &str) -> bool {
    use properties::*;
    if let Some((kind, value)) = name.split_once('=') {
        return match kind.to_ascii_lowercase().as_str() {
            "sc" | "script" => contains(SCRIPTS, &value.to_uppercase()),
            "blk" | "block" => contains(BLOCKS, &value.to_uppercase()),
            "gc" | "general_category" => contains(CATEGORIES, value),
            _ => false,
        };
    }
    if let Some(value) = name.strip_prefix("In") {
        return contains(BLOCKS, &value.to_uppercase());
    }
    if let Some(value) = name.strip_prefix("Is") {
        return contains(IS, value)
            || contains(IS_FOLDED, &value.to_uppercase())
            || contains(SCRIPTS, &value.to_uppercase());
    }
    contains(PLAIN, name) || contains(PLAIN_FOLDED, &name.to_uppercase())
}
