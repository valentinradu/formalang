//! Generic monomorphization for GPU code generation.
//!
//! WGSL and other GPU shading languages don't support generics. This module
//! transforms generic types into concrete, monomorphized variants.
//!
//! # Example
//!
//! ```formalang
//! struct Box<T> { value: T }
//! let a: Box<f32> = Box(value: 1.0)
//! let b: Box<vec4> = Box(value: vec4(1.0))
//! ```
//!
//! Becomes:
//! ```wgsl
//! struct Box_f32 { value: f32 }
//! struct Box_vec4 { value: vec4<f32> }
//! ```

use crate::ir::{
    walk_module, IrExpr, IrField, IrModule, IrStruct, IrVisitor, ResolvedType, StructId,
};
use std::collections::{HashMap, HashSet};

/// A monomorphized type instance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonomorphKey {
    /// The original generic struct
    pub base_id: StructId,
    /// Concrete type arguments
    pub args: Vec<ResolvedType>,
}

impl MonomorphKey {
    /// Generate a mangled name for this monomorphized type.
    pub fn mangled_name(&self, module: &IrModule) -> String {
        let base_name = &module.get_struct(self.base_id).name;
        let arg_names: Vec<String> = self.args.iter().map(|a| mangle_type(a, module)).collect();
        format!("{}_{}", base_name, arg_names.join("_"))
    }
}

/// Result of monomorphization.
#[derive(Clone, Debug)]
pub struct MonomorphResult {
    /// New struct ID for the monomorphized type
    pub struct_id: StructId,
    /// Mangled name
    pub name: String,
}

/// Monomorphizer collects generic instantiations and creates concrete types.
pub struct Monomorphizer<'a> {
    module: &'a IrModule,
    /// Collected generic instantiations
    instantiations: HashSet<MonomorphKey>,
}

impl<'a> Monomorphizer<'a> {
    /// Create a new monomorphizer for the given module.
    pub fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            instantiations: HashSet::new(),
        }
    }

    /// Collect all generic instantiations from the module.
    pub fn collect_instantiations(&mut self) {
        let mut collector = InstantiationCollector {
            instantiations: &mut self.instantiations,
        };
        walk_module(&mut collector, self.module);
    }

    /// Get all collected instantiations.
    pub fn instantiations(&self) -> &HashSet<MonomorphKey> {
        &self.instantiations
    }

    /// Generate monomorphized structs for all instantiations.
    ///
    /// Returns a mapping from MonomorphKey to the new struct definition.
    pub fn generate_monomorphized_structs(&self) -> Vec<(MonomorphKey, IrStruct)> {
        self.instantiations
            .iter()
            .map(|key| {
                let mono_struct = self.monomorphize_struct(key);
                (key.clone(), mono_struct)
            })
            .collect()
    }

    /// Create a monomorphized struct for a specific instantiation.
    fn monomorphize_struct(&self, key: &MonomorphKey) -> IrStruct {
        let base = self.module.get_struct(key.base_id);
        let name = key.mangled_name(self.module);

        // Build type substitution map
        let subst: HashMap<String, ResolvedType> = base
            .generic_params
            .iter()
            .zip(key.args.iter())
            .map(|(param, arg)| (param.name.clone(), arg.clone()))
            .collect();

        // Substitute types in fields
        let fields: Vec<IrField> = base
            .fields
            .iter()
            .map(|f| IrField {
                name: f.name.clone(),
                ty: substitute_type(&f.ty, &subst),
                mutable: f.mutable,
                optional: f.optional,
                default: f.default.clone(),
            })
            .collect();

        let mount_fields: Vec<IrField> = base
            .mount_fields
            .iter()
            .map(|f| IrField {
                name: f.name.clone(),
                ty: substitute_type(&f.ty, &subst),
                mutable: f.mutable,
                optional: f.optional,
                default: f.default.clone(),
            })
            .collect();

        IrStruct {
            name,
            visibility: base.visibility,
            traits: base.traits.clone(),
            fields,
            mount_fields,
            generic_params: Vec::new(), // Monomorphized = no more generics
        }
    }

    /// Generate WGSL struct definitions for all monomorphized types.
    pub fn generate_wgsl_structs(&self) -> String {
        let mut output = String::new();

        for key in &self.instantiations {
            let mono = self.monomorphize_struct(key);
            output.push_str(&format!("struct {} {{\n", mono.name));
            for field in &mono.fields {
                let ty_str = type_to_wgsl(&field.ty, self.module);
                output.push_str(&format!("    {}: {},\n", field.name, ty_str));
            }
            output.push_str("}\n\n");
        }

        output
    }
}

/// Visitor that collects all generic type instantiations.
struct InstantiationCollector<'a> {
    instantiations: &'a mut HashSet<MonomorphKey>,
}

impl<'a> IrVisitor for InstantiationCollector<'a> {
    fn visit_expr(&mut self, e: &IrExpr) {
        // Check the type of this expression
        self.collect_from_type(e.ty());

        // Also check struct instantiations
        if let IrExpr::StructInst { type_args, .. } = e {
            for arg in type_args {
                self.collect_from_type(arg);
            }
        }

        // Walk children (not walk_expr which would cause infinite recursion)
        crate::ir::walk_expr_children(self, e);
    }
}

impl<'a> InstantiationCollector<'a> {
    fn collect_from_type(&mut self, ty: &ResolvedType) {
        match ty {
            ResolvedType::Generic { base, args } => {
                // Record this instantiation
                self.instantiations.insert(MonomorphKey {
                    base_id: *base,
                    args: args.clone(),
                });
                // Also check nested types in args
                for arg in args {
                    self.collect_from_type(arg);
                }
            }
            ResolvedType::Array(inner) => self.collect_from_type(inner),
            ResolvedType::Optional(inner) => self.collect_from_type(inner),
            ResolvedType::Tuple(fields) => {
                for (_, ty) in fields {
                    self.collect_from_type(ty);
                }
            }
            ResolvedType::Dictionary { key_ty, value_ty } => {
                self.collect_from_type(key_ty);
                self.collect_from_type(value_ty);
            }
            ResolvedType::EventMapping {
                param_ty,
                return_ty,
            } => {
                if let Some(ty) = param_ty {
                    self.collect_from_type(ty);
                }
                self.collect_from_type(return_ty);
            }
            _ => {}
        }
    }
}

/// Substitute type parameters with concrete types.
fn substitute_type(ty: &ResolvedType, subst: &HashMap<String, ResolvedType>) -> ResolvedType {
    match ty {
        ResolvedType::TypeParam(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        ResolvedType::Array(inner) => ResolvedType::Array(Box::new(substitute_type(inner, subst))),
        ResolvedType::Optional(inner) => {
            ResolvedType::Optional(Box::new(substitute_type(inner, subst)))
        }
        ResolvedType::Tuple(fields) => ResolvedType::Tuple(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_type(ty, subst)))
                .collect(),
        ),
        ResolvedType::Generic { base, args } => ResolvedType::Generic {
            base: *base,
            args: args.iter().map(|a| substitute_type(a, subst)).collect(),
        },
        ResolvedType::Dictionary { key_ty, value_ty } => ResolvedType::Dictionary {
            key_ty: Box::new(substitute_type(key_ty, subst)),
            value_ty: Box::new(substitute_type(value_ty, subst)),
        },
        ResolvedType::EventMapping {
            param_ty,
            return_ty,
        } => ResolvedType::EventMapping {
            param_ty: param_ty
                .as_ref()
                .map(|t| Box::new(substitute_type(t, subst))),
            return_ty: Box::new(substitute_type(return_ty, subst)),
        },
        // These don't contain type parameters
        _ => ty.clone(),
    }
}

/// Generate a mangled name for a type (for use in struct names).
fn mangle_type(ty: &ResolvedType, module: &IrModule) -> String {
    match ty {
        ResolvedType::Primitive(p) => format!("{:?}", p).to_lowercase(),
        ResolvedType::Struct(id) => module.get_struct(*id).name.clone(),
        ResolvedType::Enum(id) => module.get_enum(*id).name.clone(),
        ResolvedType::Trait(id) => module.get_trait(*id).name.clone(),
        ResolvedType::Array(inner) => format!("arr_{}", mangle_type(inner, module)),
        ResolvedType::Optional(inner) => format!("opt_{}", mangle_type(inner, module)),
        ResolvedType::Generic { base, args } => {
            let base_name = module.get_struct(*base).name.clone();
            let arg_names: Vec<String> = args.iter().map(|a| mangle_type(a, module)).collect();
            format!("{}_{}", base_name, arg_names.join("_"))
        }
        ResolvedType::TypeParam(name) => name.clone(),
        ResolvedType::Tuple(fields) => {
            let field_names: Vec<String> = fields
                .iter()
                .map(|(n, t)| format!("{}_{}", n, mangle_type(t, module)))
                .collect();
            format!("tup_{}", field_names.join("_"))
        }
        ResolvedType::External { name, .. } => name.clone(),
        ResolvedType::Dictionary { key_ty, value_ty } => {
            format!(
                "dict_{}_{}",
                mangle_type(key_ty, module),
                mangle_type(value_ty, module)
            )
        }
        ResolvedType::EventMapping { .. } => "event".to_string(),
    }
}

/// Convert a resolved type to WGSL type string.
fn type_to_wgsl(ty: &ResolvedType, module: &IrModule) -> String {
    use crate::ast::PrimitiveType;

    match ty {
        ResolvedType::Primitive(p) => match p {
            PrimitiveType::F32 => "f32".to_string(),
            PrimitiveType::I32 => "i32".to_string(),
            PrimitiveType::U32 => "u32".to_string(),
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::Vec2 => "vec2<f32>".to_string(),
            PrimitiveType::Vec3 => "vec3<f32>".to_string(),
            PrimitiveType::Vec4 => "vec4<f32>".to_string(),
            PrimitiveType::IVec2 => "vec2<i32>".to_string(),
            PrimitiveType::IVec3 => "vec3<i32>".to_string(),
            PrimitiveType::IVec4 => "vec4<i32>".to_string(),
            PrimitiveType::UVec2 => "vec2<u32>".to_string(),
            PrimitiveType::UVec3 => "vec3<u32>".to_string(),
            PrimitiveType::UVec4 => "vec4<u32>".to_string(),
            PrimitiveType::Mat2 => "mat2x2<f32>".to_string(),
            PrimitiveType::Mat3 => "mat3x3<f32>".to_string(),
            PrimitiveType::Mat4 => "mat4x4<f32>".to_string(),
            _ => "f32".to_string(), // Default for non-GPU types
        },
        ResolvedType::Struct(id) => module.get_struct(*id).name.clone(),
        ResolvedType::Array(inner) => format!("array<{}>", type_to_wgsl(inner, module)),
        ResolvedType::Generic { base, args } => {
            // Use the monomorphized name
            let key = MonomorphKey {
                base_id: *base,
                args: args.clone(),
            };
            key.mangled_name(module)
        }
        _ => "f32".to_string(), // Default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_collect_generic_instantiations() {
        let source = r#"
            struct Box<T> { value: T }
            struct Container { box: Box<f32> = Box<f32>(value: 1.0) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        assert_eq!(mono.instantiations().len(), 1);
    }

    #[test]
    fn test_monomorphize_struct() {
        let source = r#"
            struct Container<T> { item: T, count: u32 }
            struct Holder { c: Container<f32> = Container<f32>(item: 1.0, count: 1) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        let structs = mono.generate_monomorphized_structs();
        assert_eq!(structs.len(), 1);

        let (_key, mono_struct) = &structs[0];
        assert!(mono_struct.name.contains("Container"));
        assert!(mono_struct.name.contains("f32"));
        assert!(mono_struct.generic_params.is_empty());

        // Check that field types are substituted
        let item_field = mono_struct
            .fields
            .iter()
            .find(|f| f.name == "item")
            .unwrap();
        assert!(matches!(item_field.ty, ResolvedType::Primitive(_)));
    }

    #[test]
    fn test_generate_wgsl_structs() {
        let source = r#"
            struct Wrapper<T> { value: T }
            struct Holder { w: Wrapper<f32> = Wrapper<f32>(value: 1.0) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        let wgsl = mono.generate_wgsl_structs();
        assert!(wgsl.contains("struct Wrapper_f32"));
        assert!(wgsl.contains("value: f32"));
    }

    #[test]
    fn test_multiple_instantiations() {
        let source = r#"
            struct Pair<T> { first: T, second: T }
            struct HolderA { a: Pair<f32> = Pair<f32>(first: 1.0, second: 2.0) }
            struct HolderB { b: Pair<i32> = Pair<i32>(first: 1, second: 2) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        // Should have 2 distinct instantiations
        assert_eq!(mono.instantiations().len(), 2);

        let wgsl = mono.generate_wgsl_structs();
        assert!(wgsl.contains("Pair_f32") || wgsl.contains("Pair_number"));
        assert!(wgsl.contains("Pair_i32") || wgsl.contains("Pair_number"));
    }

    #[test]
    fn test_nested_generic() {
        let source = r#"
            struct Box<T> { value: T }
            struct Outer { inner: Box<f32> = Box<f32>(value: 1.0) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        // Should collect Box<f32>
        assert_eq!(mono.instantiations().len(), 1);
    }

    #[test]
    fn test_mangled_name() {
        let source = r#"
            struct Container<T> { value: T }
            struct Holder { c: Container<f32> = Container<f32>(value: 1.0) }
        "#;

        let module = compile_to_ir(source).expect("should compile");
        let mut mono = Monomorphizer::new(&module);
        mono.collect_instantiations();

        for key in mono.instantiations() {
            let name = key.mangled_name(&module);
            assert!(name.starts_with("Container_"));
        }
    }
}
