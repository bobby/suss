//! WASM Component encoding
//!
//! Wraps core WASM modules as WASM Components using wit-component.
//! This enables WASI 0.2 support and the Component Model.

use std::borrow::Cow;

use wasm_encoder::{CustomSection, Section};
use wit_component::{ComponentEncoder, StringEncoding};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};

/// Encode a core WASM module as a WASM Component
///
/// # Arguments
/// * `core_wasm` - The core WASM module bytes
/// * `resolve` - WIT type resolution context
/// * `world_id` - The target world for the component
///
/// # Returns
/// Component bytes ready to run with wasmtime's component API
pub fn encode_component(
    core_wasm: &[u8],
    resolve: &Resolve,
    world_id: WorldId,
) -> CompileResult<Vec<u8>> {
    // Encode WIT metadata as custom section bytes
    let metadata_bytes = wit_component::metadata::encode(
        resolve,
        world_id,
        StringEncoding::UTF8,
        None, // No extra producers metadata
    )
    .map_err(|e| CompileError::Component(format!("Failed to encode metadata: {}", e)))?;

    // Append custom section to core module
    let module_with_metadata = append_custom_section(
        core_wasm,
        "component-type:suss",
        &metadata_bytes,
    );

    // Create component from module using ComponentEncoder
    let component_bytes = ComponentEncoder::default()
        .validate(true)
        .module(&module_with_metadata)
        .map_err(|e| CompileError::Component(format!("Failed to set module: {}", e)))?
        .encode()
        .map_err(|e| CompileError::Component(format!("Failed to encode component: {}", e)))?;

    Ok(component_bytes)
}

/// Append a custom section to a WASM module
///
/// WASM modules are a sequence of sections. We append the custom section
/// at the end of the existing module bytes.
fn append_custom_section(module_bytes: &[u8], name: &str, data: &[u8]) -> Vec<u8> {
    let mut result = module_bytes.to_vec();

    // Create custom section using wasm-encoder
    let section = CustomSection {
        name: Cow::Borrowed(name),
        data: Cow::Borrowed(data),
    };

    // Append the section to the module bytes
    section.append_to(&mut result);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_custom_section() {
        // Minimal valid WASM module: magic + version
        let module = vec![
            0x00, 0x61, 0x73, 0x6d, // \0asm magic
            0x01, 0x00, 0x00, 0x00, // version 1
        ];

        let result = append_custom_section(&module, "test", b"hello");

        // Should be larger than original
        assert!(result.len() > module.len());

        // Should still start with WASM magic
        assert_eq!(&result[0..4], &[0x00, 0x61, 0x73, 0x6d]);
    }
}
