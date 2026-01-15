//! Code generation module for FormaLang
//!
//! This module provides code generators that transform IR into target formats.
//!
//! ## WGSL Generation
//!
//! Generate WGSL from FormaLang IR using [`generate_wgsl`].
//!
//! ## Shader Transpilation
//!
//! Transpile WGSL to other shader formats using [`transpile_wgsl`]:
//! - Metal Shading Language (MSL) for Apple platforms
//! - SPIR-V for Vulkan
//! - HLSL for DirectX
//! - GLSL for OpenGL
//!
//! ## Trait Dispatch
//!
//! Generate dispatch code for trait implementations using [`DispatchGenerator`].
//! This is used to route calls to the correct implementation at runtime.
//!
//! ## Tree Flattening
//!
//! Flatten UI trees into linear buffers for GPU rendering using [`flatten_tree`].
//! Each element gets depth indices and parent references.
//!
//! ## Binary Format
//!
//! Write compiled FormaLang to the .fvc binary format using [`FvcWriter`].
//! This format is optimized for runtime loading.

mod dispatch;
mod flatten;
mod fvc;
mod monomorph;
mod sourcemap;
mod transpile;
mod wgsl;

pub use dispatch::{build_type_tag_map, DispatchGenerator, ImplementorInfo, TraitDispatchInfo};
pub use flatten::{
    flatten_tree, gen_flat_buffer_type, gen_flat_element_struct, FlatElement, FlattenedTree,
    TreeFlattener,
};
pub use fvc::{
    read_u32, validate_magic, FvcElement, FvcFlags, FvcHeader, FvcStruct, FvcWriter, StringTable,
    FVC_MAGIC, FVC_VERSION,
};
pub use monomorph::{MonomorphKey, MonomorphResult, Monomorphizer};
pub use sourcemap::{SourceKind, SourceMap, SourceMapEntry};
pub use transpile::{
    transpile_wgsl, transpile_wgsl_multi, validate_wgsl, ShaderOutput, ShaderTarget,
    TranspileError, TranspileResult,
};
pub use wgsl::{
    generate_wgsl, generate_wgsl_with_imports, generate_wgsl_with_sourcemap, WgslGenerator,
};
