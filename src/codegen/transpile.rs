//! Shader transpilation using naga.
//!
//! This module provides functionality to validate WGSL and transpile it to
//! various shader formats using the naga shader translation library.

use naga::back::glsl;
use naga::back::hlsl;
use naga::back::msl;
use naga::back::spv;
use naga::back::wgsl as wgsl_out;
use naga::front::wgsl as wgsl_in;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;
use std::collections::HashMap;

/// Target shader format for transpilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderTarget {
    /// WebGPU Shading Language (validated/reformatted)
    Wgsl,
    /// Metal Shading Language (for Apple platforms)
    Msl,
    /// SPIR-V binary (for Vulkan)
    SpirV,
    /// High Level Shading Language (for DirectX)
    Hlsl,
    /// OpenGL Shading Language
    Glsl,
}

/// Result of shader transpilation.
#[derive(Debug)]
pub struct TranspileResult {
    /// The transpiled shader code (or binary for SPIR-V)
    pub code: ShaderOutput,
    /// Any warnings generated during transpilation
    pub warnings: Vec<String>,
}

/// Output format for transpiled shaders.
#[derive(Debug)]
pub enum ShaderOutput {
    /// Text-based shader code (WGSL, MSL, HLSL, GLSL)
    Text(String),
    /// Binary shader code (SPIR-V)
    Binary(Vec<u8>),
}

impl ShaderOutput {
    /// Get the output as text, if available.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ShaderOutput::Text(s) => Some(s),
            ShaderOutput::Binary(_) => None,
        }
    }

    /// Get the output as binary, if available.
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            ShaderOutput::Text(_) => None,
            ShaderOutput::Binary(b) => Some(b),
        }
    }
}

/// Error during shader transpilation.
#[derive(Debug)]
pub enum TranspileError {
    /// WGSL parsing failed
    ParseError(String),
    /// Shader validation failed
    ValidationError(String),
    /// Backend transpilation failed
    BackendError(String),
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranspileError::ParseError(msg) => write!(f, "WGSL parse error: {}", msg),
            TranspileError::ValidationError(msg) => write!(f, "Shader validation error: {}", msg),
            TranspileError::BackendError(msg) => write!(f, "Transpilation error: {}", msg),
        }
    }
}

impl std::error::Error for TranspileError {}

/// Validate WGSL code without transpiling.
///
/// Returns Ok(()) if the WGSL is valid, or an error describing what's wrong.
///
/// # Example
///
/// ```
/// use formalang::codegen::validate_wgsl;
///
/// let wgsl = r#"
/// struct Point {
///     x: f32,
///     y: f32,
/// }
/// "#;
///
/// assert!(validate_wgsl(wgsl).is_ok());
/// ```
pub fn validate_wgsl(wgsl: &str) -> Result<(), TranspileError> {
    // Parse WGSL
    let module =
        wgsl_in::parse_str(wgsl).map_err(|e| TranspileError::ParseError(e.emit_to_string(wgsl)))?;

    // Validate
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    validator
        .validate(&module)
        .map_err(|e| TranspileError::ValidationError(format!("{:?}", e)))?;

    Ok(())
}

/// Transpile WGSL to the specified target format.
///
/// # Example
///
/// ```
/// use formalang::codegen::{transpile_wgsl, ShaderTarget};
///
/// let wgsl = r#"
/// struct Point {
///     x: f32,
///     y: f32,
/// }
/// "#;
///
/// let result = transpile_wgsl(wgsl, ShaderTarget::Msl).unwrap();
/// let msl = result.code.as_text().unwrap();
/// assert!(msl.contains("struct Point"));
/// ```
pub fn transpile_wgsl(wgsl: &str, target: ShaderTarget) -> Result<TranspileResult, TranspileError> {
    // Parse WGSL
    let module =
        wgsl_in::parse_str(wgsl).map_err(|e| TranspileError::ParseError(e.emit_to_string(wgsl)))?;

    // Validate and get module info
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator
        .validate(&module)
        .map_err(|e| TranspileError::ValidationError(format!("{:?}", e)))?;

    let warnings = Vec::new();

    // Transpile to target
    let code = match target {
        ShaderTarget::Wgsl => {
            let output = wgsl_out::write_string(&module, &info, wgsl_out::WriterFlags::empty())
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            ShaderOutput::Text(output)
        }

        ShaderTarget::Msl => {
            let options = msl::Options::default();
            let pipeline_options = msl::PipelineOptions::default();
            let (output, _) = msl::write_string(&module, &info, &options, &pipeline_options)
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            ShaderOutput::Text(output)
        }

        ShaderTarget::SpirV => {
            let options = spv::Options::default();
            let mut output = Vec::new();
            let mut writer = spv::Writer::new(&options)
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            writer
                .write(&module, &info, None, &None, &mut output)
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            // Convert from words to bytes
            let bytes: Vec<u8> = output.iter().flat_map(|w| w.to_le_bytes()).collect();
            ShaderOutput::Binary(bytes)
        }

        ShaderTarget::Hlsl => {
            let options = hlsl::Options::default();
            let mut output = String::new();
            let mut writer = hlsl::Writer::new(&mut output, &options);
            writer
                .write(&module, &info, None)
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            ShaderOutput::Text(output)
        }

        ShaderTarget::Glsl => {
            let options = glsl::Options {
                version: glsl::Version::Desktop(450),
                ..Default::default()
            };
            let pipeline_options = glsl::PipelineOptions {
                shader_stage: ShaderStage::Compute,
                entry_point: "main".to_string(),
                multiview: None,
            };

            let mut output = String::new();
            let mut writer = glsl::Writer::new(
                &mut output,
                &module,
                &info,
                &options,
                &pipeline_options,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            writer
                .write()
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
            ShaderOutput::Text(output)
        }
    };

    Ok(TranspileResult { code, warnings })
}

/// Transpile WGSL to multiple target formats at once.
///
/// This is more efficient than calling `transpile_wgsl` multiple times
/// as it only parses and validates the WGSL once.
pub fn transpile_wgsl_multi(
    wgsl: &str,
    targets: &[ShaderTarget],
) -> Result<HashMap<ShaderTarget, TranspileResult>, TranspileError> {
    // Parse WGSL once
    let module =
        wgsl_in::parse_str(wgsl).map_err(|e| TranspileError::ParseError(e.emit_to_string(wgsl)))?;

    // Validate once
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator
        .validate(&module)
        .map_err(|e| TranspileError::ValidationError(format!("{:?}", e)))?;

    let mut results = HashMap::new();

    for &target in targets {
        let warnings = Vec::new();

        let code = match target {
            ShaderTarget::Wgsl => {
                let output = wgsl_out::write_string(&module, &info, wgsl_out::WriterFlags::empty())
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                ShaderOutput::Text(output)
            }

            ShaderTarget::Msl => {
                let options = msl::Options::default();
                let pipeline_options = msl::PipelineOptions::default();
                let (output, _) = msl::write_string(&module, &info, &options, &pipeline_options)
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                ShaderOutput::Text(output)
            }

            ShaderTarget::SpirV => {
                let options = spv::Options::default();
                let mut output = Vec::new();
                let mut writer = spv::Writer::new(&options)
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                writer
                    .write(&module, &info, None, &None, &mut output)
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                let bytes: Vec<u8> = output.iter().flat_map(|w| w.to_le_bytes()).collect();
                ShaderOutput::Binary(bytes)
            }

            ShaderTarget::Hlsl => {
                let options = hlsl::Options::default();
                let mut output = String::new();
                let mut writer = hlsl::Writer::new(&mut output, &options);
                writer
                    .write(&module, &info, None)
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                ShaderOutput::Text(output)
            }

            ShaderTarget::Glsl => {
                let options = glsl::Options {
                    version: glsl::Version::Desktop(450),
                    ..Default::default()
                };
                let pipeline_options = glsl::PipelineOptions {
                    shader_stage: ShaderStage::Compute,
                    entry_point: "main".to_string(),
                    multiview: None,
                };

                let mut output = String::new();
                let mut writer = glsl::Writer::new(
                    &mut output,
                    &module,
                    &info,
                    &options,
                    &pipeline_options,
                    naga::proc::BoundsCheckPolicies::default(),
                )
                .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                writer
                    .write()
                    .map_err(|e| TranspileError::BackendError(format!("{:?}", e)))?;
                ShaderOutput::Text(output)
            }
        };

        results.insert(target, TranspileResult { code, warnings });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_WGSL: &str = r#"
struct Point {
    x: f32,
    y: f32,
}
"#;

    #[test]
    fn test_validate_valid_wgsl() {
        assert!(validate_wgsl(SIMPLE_WGSL).is_ok());
    }

    #[test]
    fn test_validate_invalid_wgsl() {
        let invalid = "struct Point { x: invalid_type }";
        assert!(validate_wgsl(invalid).is_err());
    }

    #[test]
    fn test_transpile_to_wgsl() {
        let result = transpile_wgsl(SIMPLE_WGSL, ShaderTarget::Wgsl).unwrap();
        let output = result.code.as_text().unwrap();
        assert!(output.contains("struct Point"));
        assert!(output.contains("f32"));
    }

    #[test]
    fn test_transpile_to_msl() {
        let result = transpile_wgsl(SIMPLE_WGSL, ShaderTarget::Msl).unwrap();
        let output = result.code.as_text().unwrap();
        assert!(output.contains("struct Point"));
        // MSL uses 'float' instead of 'f32'
        assert!(output.contains("float"));
    }

    #[test]
    fn test_transpile_to_spirv() {
        let result = transpile_wgsl(SIMPLE_WGSL, ShaderTarget::SpirV).unwrap();
        let output = result.code.as_binary().unwrap();
        // SPIR-V magic number
        assert!(output.len() >= 4);
        // Check SPIR-V magic number (0x07230203 in little endian)
        assert_eq!(&output[0..4], &[0x03, 0x02, 0x23, 0x07]);
    }

    #[test]
    fn test_transpile_to_hlsl() {
        let result = transpile_wgsl(SIMPLE_WGSL, ShaderTarget::Hlsl).unwrap();
        let output = result.code.as_text().unwrap();
        assert!(output.contains("struct Point"));
        // HLSL uses 'float' instead of 'f32'
        assert!(output.contains("float"));
    }

    #[test]
    fn test_transpile_multi() {
        let targets = [ShaderTarget::Wgsl, ShaderTarget::Msl, ShaderTarget::Hlsl];
        let results = transpile_wgsl_multi(SIMPLE_WGSL, &targets).unwrap();

        assert_eq!(results.len(), 3);
        assert!(results.contains_key(&ShaderTarget::Wgsl));
        assert!(results.contains_key(&ShaderTarget::Msl));
        assert!(results.contains_key(&ShaderTarget::Hlsl));
    }
}
