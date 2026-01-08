//! Dispatch code generation for trait implementations.
//!
//! This module generates dispatch functions that route calls to the correct
//! implementation based on concrete type. For GPU rendering, this typically
//! means generating switch statements or conditional branches.
//!
//! # Architecture
//!
//! FormaLang's trait system allows structs to implement trait requirements.
//! At render time, when we have a collection of View elements, we need to
//! dispatch to the correct rendering code for each concrete type.
//!
//! The dispatch generator:
//! 1. Collects all structs implementing a given trait
//! 2. Assigns type tags to each implementor
//! 3. Generates a dispatch function that switches on the type tag

use crate::ir::{IrModule, StructId, TraitId};
use std::collections::HashMap;

/// Information about a trait's implementors.
#[derive(Debug, Clone)]
pub struct TraitDispatchInfo {
    /// The trait being dispatched
    pub trait_id: TraitId,

    /// Trait name
    pub trait_name: String,

    /// Structs that implement this trait, with their type tags
    pub implementors: Vec<ImplementorInfo>,
}

/// Information about a single trait implementor.
#[derive(Debug, Clone)]
pub struct ImplementorInfo {
    /// The implementing struct
    pub struct_id: StructId,

    /// Struct name
    pub struct_name: String,

    /// Assigned type tag (0, 1, 2, ...)
    pub type_tag: u32,
}

/// Dispatcher generates dispatch information and code for traits.
pub struct DispatchGenerator<'a> {
    module: &'a IrModule,
}

impl<'a> DispatchGenerator<'a> {
    /// Create a new dispatch generator for the given IR module.
    pub fn new(module: &'a IrModule) -> Self {
        Self { module }
    }

    /// Collect dispatch information for a trait.
    ///
    /// Finds all structs that implement the given trait and assigns
    /// type tags to each.
    pub fn collect_trait_dispatch(&self, trait_name: &str) -> Option<TraitDispatchInfo> {
        let trait_id = self.module.trait_id(trait_name)?;

        let mut implementors = Vec::new();
        let mut type_tag = 0u32;

        for (idx, s) in self.module.structs.iter().enumerate() {
            if s.traits.contains(&trait_id) {
                implementors.push(ImplementorInfo {
                    struct_id: StructId(idx as u32),
                    struct_name: s.name.clone(),
                    type_tag,
                });
                type_tag += 1;
            }
        }

        Some(TraitDispatchInfo {
            trait_id,
            trait_name: trait_name.to_string(),
            implementors,
        })
    }

    /// Collect dispatch info for all traits in the module.
    pub fn collect_all_trait_dispatch(&self) -> Vec<TraitDispatchInfo> {
        self.module
            .traits
            .iter()
            .filter_map(|t| self.collect_trait_dispatch(&t.name))
            .collect()
    }

    /// Generate WGSL type tag enum for a trait.
    ///
    /// Creates an enum with variants for each implementor.
    pub fn gen_type_tag_enum(&self, info: &TraitDispatchInfo) -> String {
        let mut output = String::new();

        // WGSL doesn't have real enums, so we use constants
        output.push_str(&format!(
            "// Type tags for {} implementors\n",
            info.trait_name
        ));

        for imp in &info.implementors {
            output.push_str(&format!(
                "const {}_TAG_{}: u32 = {}u;\n",
                info.trait_name.to_uppercase(),
                imp.struct_name.to_uppercase(),
                imp.type_tag
            ));
        }

        output
    }

    /// Generate a dispatch function with full body.
    ///
    /// This generates a function that takes a type tag and delegates
    /// to type-specific implementations with proper parameter passing.
    ///
    /// # Parameters
    ///
    /// - `info`: Trait dispatch information
    /// - `method_name`: Name of the method to dispatch
    /// - `params`: Additional parameters beyond self (name, type)
    /// - `return_type`: Return type, if any
    /// - `default_return`: Value to return for unknown type tags
    pub fn gen_dispatch_function(
        &self,
        info: &TraitDispatchInfo,
        method_name: &str,
        params: &[(String, String)], // (name, type)
        return_type: Option<&str>,
        default_return: Option<&str>,
    ) -> String {
        let mut output = String::new();

        // Function signature
        let ret = return_type
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default();

        // Build parameter list: type_tag, self_data, then additional params
        let mut param_list: Vec<String> = vec![
            "type_tag: u32".to_string(),
            "self_data: ptr<function, ElementData>".to_string(),
        ];
        param_list.extend(params.iter().map(|(name, ty)| format!("{}: {}", name, ty)));

        output.push_str(&format!(
            "fn dispatch_{}_{}({}){}",
            info.trait_name.to_lowercase(),
            method_name,
            param_list.join(", "),
            ret
        ));

        output.push_str(" {\n");
        output.push_str("    switch type_tag {\n");

        for imp in &info.implementors {
            // Build the call arguments: load struct from data, then pass additional params
            let call_args: Vec<String> = std::iter::once(format!(
                "load_{}(self_data)",
                imp.struct_name.to_lowercase()
            ))
            .chain(params.iter().map(|(name, _)| name.clone()))
            .collect();

            output.push_str(&format!(
                "        case {}u: {{ return {}_{}({}); }}\n",
                imp.type_tag,
                imp.struct_name,
                method_name,
                call_args.join(", ")
            ));
        }

        // Default case
        if let Some(default_val) = default_return {
            output.push_str(&format!("        default: {{ return {}; }}\n", default_val));
        } else {
            output.push_str("        default: { }\n");
        }

        output.push_str("    }\n");
        output.push_str("}\n");

        output
    }

    /// Generate the trait-specific data struct for storing element field data.
    ///
    /// This struct is a union-like storage that can hold data for any
    /// concrete type that implements the trait. The struct name is derived
    /// from the trait name (e.g., `Fill` -> `FillData`).
    pub fn gen_element_data_struct(
        &self,
        info: &TraitDispatchInfo,
        max_f32_fields: usize,
    ) -> String {
        let mut output = String::new();
        let data_struct_name = format!("{}Data", info.trait_name);

        output.push_str(&format!(
            "// {} data storage (union-like struct for all implementors)\n",
            info.trait_name
        ));
        output.push_str(&format!("struct {} {{\n", data_struct_name));
        output.push_str("    type_tag: u32,\n");
        output.push_str("    element_index: u32,\n");
        output.push_str(&format!("    data: array<f32, {}>,\n", max_f32_fields));
        output.push_str("}\n\n");

        // Generate accessor constants for each type's fields
        for imp in &info.implementors {
            let struct_def = self.module.get_struct(imp.struct_id);
            output.push_str(&format!("// {} field offsets\n", imp.struct_name));

            let mut offset = 0u32;
            for field in &struct_def.fields {
                let field_size = self.type_size_in_f32(&field.ty);
                output.push_str(&format!(
                    "const {}_{}_OFFSET: u32 = {}u;\n",
                    imp.struct_name.to_uppercase(),
                    field.name.to_uppercase(),
                    offset
                ));
                offset += field_size;
            }
            output.push('\n');
        }

        output
    }

    /// Generate a load function for a specific struct type.
    ///
    /// This reads field values from the trait-specific data struct and
    /// constructs a struct instance.
    pub fn gen_struct_load_function(&self, imp: &ImplementorInfo, trait_name: &str) -> String {
        let mut output = String::new();
        let struct_def = self.module.get_struct(imp.struct_id);
        let struct_name_lower = imp.struct_name.to_lowercase();
        let data_struct_name = format!("{}Data", trait_name);

        output.push_str(&format!(
            "fn load_{}(data: ptr<function, {}>) -> {} {{\n",
            struct_name_lower, data_struct_name, imp.struct_name
        ));
        output.push_str(&format!("    var result: {};\n", imp.struct_name));

        let mut offset = 0u32;
        for field in &struct_def.fields {
            let field_size = self.type_size_in_f32(&field.ty);
            let load_expr = self.gen_field_load_expr(&field.ty, "data", offset);
            output.push_str(&format!("    result.{} = {};\n", field.name, load_expr));
            offset += field_size;
        }

        output.push_str("    return result;\n");
        output.push_str("}\n");

        output
    }

    /// Generate load functions for all implementors of a trait.
    pub fn gen_all_load_functions(&self, info: &TraitDispatchInfo) -> String {
        let mut output = String::new();

        for imp in &info.implementors {
            output.push_str(&self.gen_struct_load_function(imp, &info.trait_name));
            output.push('\n');
        }

        output
    }

    /// Calculate the size of a type in f32 units (for data packing).
    fn type_size_in_f32(&self, ty: &crate::ir::ResolvedType) -> u32 {
        use crate::ast::PrimitiveType;
        use crate::ir::ResolvedType;

        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32
                | PrimitiveType::I32
                | PrimitiveType::U32
                | PrimitiveType::Bool => 1,
                PrimitiveType::Vec2 | PrimitiveType::IVec2 | PrimitiveType::UVec2 => 2,
                PrimitiveType::Vec3 | PrimitiveType::IVec3 | PrimitiveType::UVec3 => 3,
                PrimitiveType::Vec4 | PrimitiveType::IVec4 | PrimitiveType::UVec4 => 4,
                PrimitiveType::Mat2 => 4,
                PrimitiveType::Mat3 => 9,
                PrimitiveType::Mat4 => 16,
                // Non-GPU types default to 1
                _ => 1,
            },
            ResolvedType::Struct(id) => {
                let s = self.module.get_struct(*id);
                s.fields.iter().map(|f| self.type_size_in_f32(&f.ty)).sum()
            }
            _ => 1, // Default for unknown types
        }
    }

    /// Generate an expression to load a field from ElementData.
    fn gen_field_load_expr(
        &self,
        ty: &crate::ir::ResolvedType,
        data_ptr: &str,
        offset: u32,
    ) -> String {
        use crate::ast::PrimitiveType;
        use crate::ir::ResolvedType;

        match ty {
            ResolvedType::Primitive(p) => match p {
                PrimitiveType::F32 => format!("(*{}).data[{}]", data_ptr, offset),
                PrimitiveType::I32 => {
                    format!("i32(bitcast<i32>((*{}).data[{}]))", data_ptr, offset)
                }
                PrimitiveType::U32 => {
                    format!("u32(bitcast<u32>((*{}).data[{}]))", data_ptr, offset)
                }
                PrimitiveType::Bool => format!("(*{}).data[{}] != 0.0", data_ptr, offset),
                PrimitiveType::Vec2 => format!(
                    "vec2<f32>((*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1
                ),
                PrimitiveType::Vec3 => format!(
                    "vec3<f32>((*{}).data[{}], (*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1,
                    data_ptr,
                    offset + 2
                ),
                PrimitiveType::Vec4 => format!(
                    "vec4<f32>((*{}).data[{}], (*{}).data[{}], (*{}).data[{}], (*{}).data[{}])",
                    data_ptr,
                    offset,
                    data_ptr,
                    offset + 1,
                    data_ptr,
                    offset + 2,
                    data_ptr,
                    offset + 3
                ),
                _ => format!("(*{}).data[{}]", data_ptr, offset),
            },
            _ => format!("(*{}).data[{}]", data_ptr, offset),
        }
    }
}

/// Default data size for external trait placeholders (in f32 units).
pub const DEFAULT_EXTERNAL_TRAIT_DATA_SIZE: usize = 16;

impl<'a> DispatchGenerator<'a> {
    /// Generate a placeholder data struct for an external trait.
    ///
    /// When we reference a trait from an imported module, we don't have
    /// the implementor information available. This generates a placeholder
    /// data struct that can store trait dispatch data at runtime.
    ///
    /// # Parameters
    ///
    /// - `trait_name`: The simple name of the trait (without module path)
    /// - `data_size`: Size of the data array in f32 units
    pub fn gen_external_trait_data_struct(trait_name: &str, data_size: usize) -> String {
        let mut output = String::new();
        let data_struct_name = format!("{}Data", trait_name);

        output.push_str(&format!(
            "// {} data storage (external trait placeholder)\n",
            trait_name
        ));
        output.push_str(&format!("struct {} {{\n", data_struct_name));
        output.push_str("    type_tag: u32,\n");
        output.push_str("    element_index: u32,\n");
        output.push_str(&format!("    data: array<f32, {}>,\n", data_size));
        output.push_str("}\n");

        output
    }

    /// Generate complete dispatch code for a trait.
    ///
    /// This generates:
    /// - Type tag constants
    /// - ElementData struct
    /// - Load functions for each implementor
    /// - Dispatch function
    pub fn gen_complete_dispatch(
        &self,
        info: &TraitDispatchInfo,
        method_name: &str,
        params: &[(String, String)],
        return_type: Option<&str>,
        default_return: Option<&str>,
    ) -> String {
        let mut output = String::new();

        // Calculate max data size needed
        let max_size: u32 = info
            .implementors
            .iter()
            .map(|imp| {
                let s = self.module.get_struct(imp.struct_id);
                s.fields.iter().map(|f| self.type_size_in_f32(&f.ty)).sum()
            })
            .max()
            .unwrap_or(16);

        // Type tag constants
        output.push_str(&self.gen_type_tag_enum(info));
        output.push('\n');

        // Element data struct
        output.push_str(&self.gen_element_data_struct(info, max_size as usize));

        // Load functions
        output.push_str(&self.gen_all_load_functions(info));

        // Dispatch function
        output.push_str(&self.gen_dispatch_function(
            info,
            method_name,
            params,
            return_type,
            default_return,
        ));

        output
    }
}

/// Build a type-to-tag mapping from dispatch info.
pub fn build_type_tag_map(infos: &[TraitDispatchInfo]) -> HashMap<String, u32> {
    let mut map = HashMap::new();

    for info in infos {
        for imp in &info.implementors {
            map.insert(
                format!("{}::{}", info.trait_name, imp.struct_name),
                imp.type_tag,
            );
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_collect_trait_dispatch() {
        let source = r#"
            trait Shape { width: f32 }
            struct Circle: Shape { width: f32, radius: f32 }
            struct Rect: Shape { width: f32, height: f32 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Shape").unwrap();
        assert_eq!(info.trait_name, "Shape");
        assert_eq!(info.implementors.len(), 2);

        // Check type tags are assigned
        assert!(info
            .implementors
            .iter()
            .any(|i| i.struct_name == "Circle" && i.type_tag == 0));
        assert!(info
            .implementors
            .iter()
            .any(|i| i.struct_name == "Rect" && i.type_tag == 1));
    }

    #[test]
    fn test_gen_type_tag_enum() {
        let source = r#"
            trait Renderable { width: f32 }
            struct Label: Renderable { width: f32, text: String }
            struct Button: Renderable { width: f32, action: String }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Renderable").unwrap();
        let code = gen.gen_type_tag_enum(&info);

        assert!(code.contains("RENDERABLE_TAG_LABEL"));
        assert!(code.contains("RENDERABLE_TAG_BUTTON"));
    }

    #[test]
    fn test_gen_dispatch_function() {
        let source = r#"
            trait Fill { color: vec4 }
            struct Solid: Fill { color: vec4 }
            struct Gradient: Fill { color: vec4, color2: vec4 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Fill").unwrap();
        let code = gen.gen_dispatch_function(
            &info,
            "sample",
            &[("uv".to_string(), "vec2<f32>".to_string())],
            Some("vec4<f32>"),
            Some("vec4<f32>(0.0, 0.0, 0.0, 1.0)"),
        );

        assert!(code.contains("fn dispatch_fill_sample"));
        assert!(code.contains("switch type_tag"));
        assert!(code.contains("Solid_sample(load_solid(self_data), uv)"));
        assert!(code.contains("Gradient_sample(load_gradient(self_data), uv)"));
        assert!(code.contains("default: { return vec4<f32>(0.0, 0.0, 0.0, 1.0);"));
    }

    #[test]
    fn test_gen_load_function() {
        let source = r#"
            trait Renderable { width: f32 }
            struct Box: Renderable { width: f32, height: f32, color: vec4 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Renderable").unwrap();
        let box_imp = &info.implementors[0];
        let code = gen.gen_struct_load_function(box_imp, &info.trait_name);

        assert!(code.contains("fn load_box(data: ptr<function, RenderableData>) -> Box"));
        assert!(code.contains("var result: Box;"));
        assert!(code.contains("result.width = (*data).data[0]"));
        assert!(code.contains("result.height = (*data).data[1]"));
        assert!(code.contains("result.color = vec4<f32>"));
        assert!(code.contains("return result;"));
    }

    #[test]
    fn test_gen_element_data_struct() {
        let source = r#"
            trait Shape { area: f32 }
            struct Circle: Shape { area: f32, radius: f32 }
            struct Rect: Shape { area: f32, width: f32, height: f32 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Shape").unwrap();
        let code = gen.gen_element_data_struct(&info, 16);

        // Struct should be named after the trait (ShapeData, not ElementData)
        assert!(code.contains("struct ShapeData"));
        assert!(code.contains("type_tag: u32"));
        assert!(code.contains("data: array<f32, 16>"));
        assert!(code.contains("CIRCLE_AREA_OFFSET: u32 = 0u"));
        assert!(code.contains("CIRCLE_RADIUS_OFFSET: u32 = 1u"));
        assert!(code.contains("RECT_AREA_OFFSET: u32 = 0u"));
        assert!(code.contains("RECT_WIDTH_OFFSET: u32 = 1u"));
        assert!(code.contains("RECT_HEIGHT_OFFSET: u32 = 2u"));
    }

    #[test]
    fn test_gen_complete_dispatch() {
        let source = r#"
            trait Fill { color: vec4 }
            struct Solid: Fill { color: vec4 }
            struct Gradient: Fill { color: vec4, end_color: vec4 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Fill").unwrap();
        let code = gen.gen_complete_dispatch(
            &info,
            "sample",
            &[("uv".to_string(), "vec2<f32>".to_string())],
            Some("vec4<f32>"),
            Some("vec4<f32>(0.0)"),
        );

        // Should have all components
        assert!(code.contains("FILL_TAG_SOLID"));
        assert!(code.contains("FILL_TAG_GRADIENT"));
        // Data struct should be named after trait (FillData, not ElementData)
        assert!(code.contains("struct FillData"));
        assert!(code.contains("fn load_solid"));
        assert!(code.contains("fn load_gradient"));
        assert!(code.contains("fn dispatch_fill_sample"));
    }

    #[test]
    fn test_no_implementors() {
        let source = r#"
            trait Empty { value: f32 }
        "#;

        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        let info = gen.collect_trait_dispatch("Empty").unwrap();
        assert!(info.implementors.is_empty());
    }

    #[test]
    fn test_nonexistent_trait() {
        let source = "struct Foo { x: f32 }";
        let module = compile_to_ir(source).unwrap();
        let gen = DispatchGenerator::new(&module);

        assert!(gen.collect_trait_dispatch("DoesNotExist").is_none());
    }
}
