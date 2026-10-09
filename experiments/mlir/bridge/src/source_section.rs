//! Original bounded evaluation carrier for verified native source facts.
//! Custom data supplements executable lowering; it is never reconstructed from SSA.
use std::borrow::Cow;
use wasm_encoder::{CustomSection, Encode};
const NAME: &[u8] = b"suss.source-analysis";
type Result<T> = std::result::Result<T, String>;
fn leb(bytes: &[u8], at: &mut usize) -> Result<usize> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(*at).ok_or("truncated section size")?;
        *at += 1;
        if shift == 28 && byte & 0xf0 != 0 {
            return Err("section size overflow".into());
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value as usize);
        }
    }
    Err("section size overflow".into())
}
pub fn preserved<'a>(wasm: &'a [u8]) -> Result<Option<&'a [u8]>> {
    if !wasm.starts_with(b"\0asm\x01\0\0\0") {
        return Err("expected core Wasm module".into());
    }
    let mut at = 8;
    let mut source = None;
    while at < wasm.len() {
        let id = wasm[at];
        at += 1;
        let len = leb(wasm, &mut at)?;
        let end = at
            .checked_add(len)
            .filter(|end| *end <= wasm.len())
            .ok_or("section outside module")?;
        if id == 0 {
            let mut inside = 0;
            let section = &wasm[at..end];
            let len = leb(section, &mut inside)?;
            let name_end = inside
                .checked_add(len)
                .filter(|end| *end <= section.len())
                .ok_or("custom name outside section")?;
            if &section[inside..name_end] == NAME {
                if source.is_some() {
                    return Err("duplicate source-analysis section".into());
                }
                source = Some(&section[name_end..]);
            }
        }
        at = end;
    }
    Ok(source)
}
pub fn append(wasm: &mut Vec<u8>, source: &str) -> Result<()> {
    if source.len() > 1 << 20 {
        return Err("source-analysis exceeds 1 MiB".into());
    }
    if preserved(wasm)?.is_some() {
        return Err("existing source-analysis section".into());
    }
    wasm.push(0);
    CustomSection {
        name: Cow::Borrowed("suss.source-analysis"),
        data: Cow::Borrowed(source.as_bytes()),
    }
    .encode(wasm);
    if preserved(wasm)? != Some(source.as_bytes()) {
        return Err("source-analysis bytes changed during lowering".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_attachment_requires_resealing_and_survives_identity_verification() {
        use suss_compile::portable::{self, artifact_identity};
        let mut wasm = portable::compile("7").unwrap();
        let facts = r#"{"metadata":{"false":false,"nil":null},"utf16":[55296]}"#;
        append(&mut wasm, facts).unwrap();
        assert!(
            artifact_identity::verify(&wasm, Default::default())
                .unwrap_err()
                .contains("integrity")
        );
        let sealed = artifact_identity::annotate_ir(&wasm).unwrap();
        artifact_identity::verify(&sealed, Default::default()).unwrap();
        assert_eq!(preserved(&sealed).unwrap(), Some(facts.as_bytes()));
        assert_eq!(artifact_identity::annotate_ir(&sealed).unwrap(), sealed);
    }
    #[test]
    fn full_bytes_and_rejection_boundaries() {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        let facts = r#"{"metadata":[55296,0,56320],"nil":null,"false":false}"#;
        append(&mut wasm, facts).unwrap();
        assert_eq!(preserved(&wasm).unwrap(), Some(facts.as_bytes()));
        assert!(append(&mut wasm, facts).is_err());
        let mut duplicate = wasm.clone();
        duplicate.extend_from_slice(&wasm[8..]);
        assert!(preserved(&duplicate).is_err());
        assert!(preserved(&wasm[..wasm.len() - 1]).is_err());
        assert!(preserved(b"\0asm\x01\0\0\0\0\xff\xff\xff\xff\xff").is_err());
        assert!(append(&mut b"\0asm\x01\0\0\0".to_vec(), &"x".repeat((1 << 20) + 1)).is_err());
    }
}
