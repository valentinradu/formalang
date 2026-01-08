//! FormaLang Compiled (.fvc) binary format writer.
//!
//! The .fvc format is the compiled output of FormaLang, containing:
//! - Type metadata (structs, traits, enums)
//! - Compiled shader code (SPIR-V or platform-native)
//! - Flattened UI element data
//! - Asset references
//!
//! # Format Structure
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ Header (24 bytes)                       │
//! │   Magic: "FVC\0" (4 bytes)              │
//! │   Version: u32 (4 bytes)                │
//! │   Flags: u32 (4 bytes)                  │
//! │   Struct count: u32 (4 bytes)           │
//! │   Shader size: u32 (4 bytes)            │
//! │   Element count: u32 (4 bytes)          │
//! ├─────────────────────────────────────────┤
//! │ String Table                            │
//! │   Count: u32                            │
//! │   Offsets: [u32; count]                 │
//! │   Data: UTF-8 strings                   │
//! ├─────────────────────────────────────────┤
//! │ Type Metadata                           │
//! │   Structs, traits, enums                │
//! ├─────────────────────────────────────────┤
//! │ Shader Code (SPIR-V)                    │
//! ├─────────────────────────────────────────┤
//! │ Element Data                            │
//! │   Flattened UI tree                     │
//! └─────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::io::{self, Write};

/// Magic bytes for the FVC format: "FVC\0"
pub const FVC_MAGIC: [u8; 4] = [b'F', b'V', b'C', 0];

/// Current format version
pub const FVC_VERSION: u32 = 1;

/// Format flags
#[derive(Debug, Clone, Copy)]
pub struct FvcFlags(pub u32);

impl FvcFlags {
    /// No flags set
    pub const NONE: FvcFlags = FvcFlags(0);

    /// Contains SPIR-V shader code
    pub const SPIRV_SHADER: FvcFlags = FvcFlags(1 << 0);

    /// Contains WGSL shader code
    pub const WGSL_SHADER: FvcFlags = FvcFlags(1 << 1);

    /// Contains MSL shader code
    pub const MSL_SHADER: FvcFlags = FvcFlags(1 << 2);

    /// Debug information included
    pub const DEBUG_INFO: FvcFlags = FvcFlags(1 << 8);

    /// Combine flags
    pub fn combine(self, other: FvcFlags) -> FvcFlags {
        FvcFlags(self.0 | other.0)
    }

    /// Check if flag is set
    pub fn has(self, flag: FvcFlags) -> bool {
        (self.0 & flag.0) == flag.0
    }
}

/// String table for deduplicating strings.
#[derive(Debug, Default)]
pub struct StringTable {
    strings: Vec<String>,
    indices: HashMap<String, u32>,
}

impl StringTable {
    /// Create a new string table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a string and return its index.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.indices.get(s) {
            return idx;
        }

        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.indices.insert(s.to_string(), idx);
        idx
    }

    /// Get a string by index.
    pub fn get(&self, idx: u32) -> Option<&str> {
        self.strings.get(idx as usize).map(|s| s.as_str())
    }

    /// Write string table to binary.
    pub fn write<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let mut written = 0;

        // Count
        written += write_u32(writer, self.strings.len() as u32)?;

        // Calculate offsets
        let mut offset = 0u32;
        let mut offsets = Vec::with_capacity(self.strings.len());
        for s in &self.strings {
            offsets.push(offset);
            offset += s.len() as u32 + 1; // +1 for null terminator
        }

        // Write offsets
        for off in &offsets {
            written += write_u32(writer, *off)?;
        }

        // Write strings (null-terminated)
        for s in &self.strings {
            writer.write_all(s.as_bytes())?;
            writer.write_all(&[0])?;
            written += s.len() + 1;
        }

        Ok(written)
    }
}

/// Struct metadata for the binary format.
#[derive(Debug, Clone)]
pub struct FvcStruct {
    /// Name (string table index)
    pub name_idx: u32,

    /// Number of fields
    pub field_count: u32,

    /// Field names (string table indices)
    pub field_names: Vec<u32>,

    /// Field type tags
    pub field_types: Vec<u32>,
}

impl FvcStruct {
    /// Write struct to binary.
    pub fn write<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let mut written = 0;

        written += write_u32(writer, self.name_idx)?;
        written += write_u32(writer, self.field_count)?;

        for &name_idx in &self.field_names {
            written += write_u32(writer, name_idx)?;
        }

        for &type_tag in &self.field_types {
            written += write_u32(writer, type_tag)?;
        }

        Ok(written)
    }
}

/// Element data for the binary format.
#[derive(Debug, Clone)]
pub struct FvcElement {
    /// Type tag (from dispatch generator)
    pub type_tag: u32,

    /// Depth in tree
    pub depth: u32,

    /// Parent index (u32::MAX for root)
    pub parent_index: u32,

    /// Child count
    pub child_count: u32,

    /// First child index (u32::MAX if none)
    pub child_start: u32,

    /// Struct instance data offset
    pub data_offset: u32,
}

impl FvcElement {
    /// Size of element header in bytes
    pub const HEADER_SIZE: usize = 24;

    /// Write element to binary.
    pub fn write<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let mut written = 0;

        written += write_u32(writer, self.type_tag)?;
        written += write_u32(writer, self.depth)?;
        written += write_u32(writer, self.parent_index)?;
        written += write_u32(writer, self.child_count)?;
        written += write_u32(writer, self.child_start)?;
        written += write_u32(writer, self.data_offset)?;

        Ok(written)
    }
}

/// FVC file writer.
pub struct FvcWriter {
    strings: StringTable,
    structs: Vec<FvcStruct>,
    shader_spirv: Vec<u8>,
    shader_wgsl: String,
    elements: Vec<FvcElement>,
    flags: FvcFlags,
}

impl FvcWriter {
    /// Create a new FVC writer.
    pub fn new() -> Self {
        Self {
            strings: StringTable::new(),
            structs: Vec::new(),
            shader_spirv: Vec::new(),
            shader_wgsl: String::new(),
            elements: Vec::new(),
            flags: FvcFlags::NONE,
        }
    }

    /// Add a struct definition.
    pub fn add_struct(&mut self, name: &str, fields: &[(String, u32)]) -> u32 {
        let name_idx = self.strings.intern(name);
        let field_names: Vec<u32> = fields.iter().map(|(n, _)| self.strings.intern(n)).collect();
        let field_types: Vec<u32> = fields.iter().map(|(_, t)| *t).collect();

        let idx = self.structs.len() as u32;
        self.structs.push(FvcStruct {
            name_idx,
            field_count: fields.len() as u32,
            field_names,
            field_types,
        });

        idx
    }

    /// Set SPIR-V shader code.
    pub fn set_spirv_shader(&mut self, spirv: Vec<u8>) {
        self.shader_spirv = spirv;
        self.flags = self.flags.combine(FvcFlags::SPIRV_SHADER);
    }

    /// Set WGSL shader code.
    pub fn set_wgsl_shader(&mut self, wgsl: String) {
        self.shader_wgsl = wgsl;
        self.flags = self.flags.combine(FvcFlags::WGSL_SHADER);
    }

    /// Add an element.
    pub fn add_element(&mut self, element: FvcElement) {
        self.elements.push(element);
    }

    /// Write the complete FVC file.
    pub fn write<W: Write>(&self, writer: &mut W) -> io::Result<usize> {
        let mut written = 0;

        // Header
        writer.write_all(&FVC_MAGIC)?;
        written += 4;

        written += write_u32(writer, FVC_VERSION)?;
        written += write_u32(writer, self.flags.0)?;
        written += write_u32(writer, self.structs.len() as u32)?;
        written += write_u32(writer, self.shader_spirv.len() as u32)?;
        written += write_u32(writer, self.elements.len() as u32)?;

        // String table
        written += self.strings.write(writer)?;

        // Structs
        for s in &self.structs {
            written += s.write(writer)?;
        }

        // Shader (SPIR-V if available, else WGSL)
        if !self.shader_spirv.is_empty() {
            writer.write_all(&self.shader_spirv)?;
            written += self.shader_spirv.len();
        } else if !self.shader_wgsl.is_empty() {
            let wgsl_bytes = self.shader_wgsl.as_bytes();
            written += write_u32(writer, wgsl_bytes.len() as u32)?;
            writer.write_all(wgsl_bytes)?;
            written += wgsl_bytes.len();
        }

        // Elements
        for e in &self.elements {
            written += e.write(writer)?;
        }

        Ok(written)
    }

    /// Write to a byte vector.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write(&mut buffer)?;
        Ok(buffer)
    }
}

impl Default for FvcWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Write a u32 in little-endian format.
fn write_u32<W: Write>(writer: &mut W, value: u32) -> io::Result<usize> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(4)
}

/// Read a u32 in little-endian format.
pub fn read_u32(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Validate FVC magic bytes.
pub fn validate_magic(data: &[u8]) -> bool {
    data.len() >= 4 && data[..4] == FVC_MAGIC
}

/// FVC header for reading.
#[derive(Debug, Clone)]
pub struct FvcHeader {
    pub version: u32,
    pub flags: FvcFlags,
    pub struct_count: u32,
    pub shader_size: u32,
    pub element_count: u32,
}

impl FvcHeader {
    /// Parse header from bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 24 {
            return None;
        }

        if !validate_magic(data) {
            return None;
        }

        Some(Self {
            version: read_u32(&data[4..])?,
            flags: FvcFlags(read_u32(&data[8..])?),
            struct_count: read_u32(&data[12..])?,
            shader_size: read_u32(&data[16..])?,
            element_count: read_u32(&data[20..])?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_table() {
        let mut table = StringTable::new();

        let idx1 = table.intern("hello");
        let idx2 = table.intern("world");
        let idx3 = table.intern("hello"); // duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // Same as idx1

        assert_eq!(table.get(0), Some("hello"));
        assert_eq!(table.get(1), Some("world"));
        assert_eq!(table.get(2), None);
    }

    #[test]
    fn test_fvc_writer_empty() {
        let writer = FvcWriter::new();
        let bytes = writer.to_bytes().unwrap();

        assert!(bytes.len() >= 24);
        assert!(validate_magic(&bytes));

        let header = FvcHeader::parse(&bytes).unwrap();
        assert_eq!(header.version, FVC_VERSION);
        assert_eq!(header.struct_count, 0);
        assert_eq!(header.element_count, 0);
    }

    #[test]
    fn test_fvc_writer_with_struct() {
        let mut writer = FvcWriter::new();

        writer.add_struct("Point", &[("x".to_string(), 0), ("y".to_string(), 0)]);

        let bytes = writer.to_bytes().unwrap();
        let header = FvcHeader::parse(&bytes).unwrap();

        assert_eq!(header.struct_count, 1);
    }

    #[test]
    fn test_fvc_writer_with_elements() {
        let mut writer = FvcWriter::new();

        writer.add_element(FvcElement {
            type_tag: 0,
            depth: 0,
            parent_index: u32::MAX,
            child_count: 2,
            child_start: 1,
            data_offset: 0,
        });

        writer.add_element(FvcElement {
            type_tag: 1,
            depth: 1,
            parent_index: 0,
            child_count: 0,
            child_start: u32::MAX,
            data_offset: 0,
        });

        let bytes = writer.to_bytes().unwrap();
        let header = FvcHeader::parse(&bytes).unwrap();

        assert_eq!(header.element_count, 2);
    }

    #[test]
    fn test_fvc_flags() {
        let flags = FvcFlags::SPIRV_SHADER.combine(FvcFlags::DEBUG_INFO);

        assert!(flags.has(FvcFlags::SPIRV_SHADER));
        assert!(flags.has(FvcFlags::DEBUG_INFO));
        assert!(!flags.has(FvcFlags::WGSL_SHADER));
    }

    #[test]
    fn test_fvc_with_spirv_shader() {
        let mut writer = FvcWriter::new();

        // Fake SPIR-V data (just for testing structure)
        let spirv = vec![0x03, 0x02, 0x23, 0x07, 0x00, 0x00, 0x00, 0x00];
        writer.set_spirv_shader(spirv);

        let bytes = writer.to_bytes().unwrap();
        let header = FvcHeader::parse(&bytes).unwrap();

        assert!(header.flags.has(FvcFlags::SPIRV_SHADER));
        assert_eq!(header.shader_size, 8);
    }
}
