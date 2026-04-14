use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, PrimitiveType, Statement, StructDef, Type};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 4: Validate trait implementations
    /// Check that structs implement all required fields from their traits
    pub(super) fn validate_trait_implementations(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    self.validate_struct_trait_implementation(struct_def);
                }
            }
        }
    }

    /// Validate that a struct implements all required fields from its traits
    pub(super) fn validate_struct_trait_implementation(&mut self, struct_def: &StructDef) {
        // For each implemented trait
        for trait_ref in &struct_def.traits {
            // Get all required fields from this trait (including composed traits)
            let required_fields = self.symbols.get_all_trait_fields(&trait_ref.name);

            // Check each required field
            for (field_name, required_type) in required_fields {
                // Look for the field in the struct
                match struct_def.fields.iter().find(|f| f.name.name == field_name) {
                    Some(struct_field) => {
                        // Field exists, check type matches
                        if !Self::types_match(&struct_field.ty, &required_type) {
                            self.errors.push(CompilerError::TraitFieldTypeMismatch {
                                field: field_name.clone(),
                                trait_name: trait_ref.name.clone(),
                                expected: Self::type_to_string(&required_type),
                                actual: Self::type_to_string(&struct_field.ty),
                                span: struct_field.span,
                            });
                        }
                    }
                    None => {
                        // Field is missing
                        self.errors.push(CompilerError::MissingTraitField {
                            field: field_name.clone(),
                            trait_name: trait_ref.name.clone(),
                            span: struct_def.span,
                        });
                    }
                }
            }

            // Get all required mounting points from this trait (including composed traits)
            let required_mounts = self.symbols.get_all_trait_mounting_points(&trait_ref.name);

            // Check each required mounting point
            for (mount_name, required_type) in required_mounts {
                match struct_def
                    .mount_fields
                    .iter()
                    .find(|f| f.name.name == mount_name)
                {
                    Some(mount_field) => {
                        // Mounting point exists, check type matches
                        // Special case: `Never` type satisfies any mount point requirement.
                        // `Never` is a terminal type indicating "no child content", used by
                        // terminal components like Empty, EmptyShape, etc.
                        let is_never =
                            matches!(&mount_field.ty, Type::Primitive(PrimitiveType::Never));
                        if !is_never && !Self::types_match(&mount_field.ty, &required_type) {
                            self.errors
                                .push(CompilerError::TraitMountingPointTypeMismatch {
                                    mount: mount_name.clone(),
                                    trait_name: trait_ref.name.clone(),
                                    expected: Self::type_to_string(&required_type),
                                    actual: Self::type_to_string(&mount_field.ty),
                                    span: mount_field.span,
                                });
                        }
                    }
                    None => {
                        // Mounting point is missing
                        self.errors.push(CompilerError::MissingTraitMountingPoint {
                            mount: mount_name.clone(),
                            trait_name: trait_ref.name.clone(),
                            span: struct_def.span,
                        });
                    }
                }
            }
        }
    }

    /// Check if two types match (structural equality)
    pub(super) fn types_match(ty1: &Type, ty2: &Type) -> bool {
        match (ty1, ty2) {
            (Type::Primitive(p1), Type::Primitive(p2)) => p1 == p2,
            (Type::Ident(i1), Type::Ident(i2)) => i1.name == i2.name,
            (Type::Array(elem1), Type::Array(elem2)) => Self::types_match(elem1, elem2),
            (Type::Optional(inner1), Type::Optional(inner2)) => Self::types_match(inner1, inner2),
            (
                Type::Generic {
                    name: n1, args: a1, ..
                },
                Type::Generic {
                    name: n2, args: a2, ..
                },
            ) => {
                // Generic types match if they have the same base type and matching arguments
                n1.name == n2.name
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2.iter())
                        .all(|(t1, t2)| Self::types_match(t1, t2))
            }
            (Type::TypeParameter(p1), Type::TypeParameter(p2)) => p1.name == p2.name,
            _ => false,
        }
    }

    /// Convert a type to a string for error messages
    pub(super) fn type_to_string(ty: &Type) -> String {
        match ty {
            Type::Primitive(prim) => match prim {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::Number => "Number".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Path => "Path".to_string(),
                PrimitiveType::Regex => "Regex".to_string(),
                PrimitiveType::Never => "Never".to_string(),
                // GPU scalar types
                PrimitiveType::F32 => "f32".to_string(),
                PrimitiveType::I32 => "i32".to_string(),
                PrimitiveType::U32 => "u32".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                // GPU vector types (float)
                PrimitiveType::Vec2 => "vec2".to_string(),
                PrimitiveType::Vec3 => "vec3".to_string(),
                PrimitiveType::Vec4 => "vec4".to_string(),
                // GPU vector types (signed int)
                PrimitiveType::IVec2 => "ivec2".to_string(),
                PrimitiveType::IVec3 => "ivec3".to_string(),
                PrimitiveType::IVec4 => "ivec4".to_string(),
                // GPU vector types (unsigned int)
                PrimitiveType::UVec2 => "uvec2".to_string(),
                PrimitiveType::UVec3 => "uvec3".to_string(),
                PrimitiveType::UVec4 => "uvec4".to_string(),
                // GPU matrix types
                PrimitiveType::Mat2 => "mat2".to_string(),
                PrimitiveType::Mat3 => "mat3".to_string(),
                PrimitiveType::Mat4 => "mat4".to_string(),
            },
            Type::Ident(ident) => ident.name.clone(),
            Type::Array(element_type) => {
                format!("[{}]", Self::type_to_string(element_type))
            }
            Type::Optional(inner_type) => {
                format!("{}?", Self::type_to_string(inner_type))
            }
            Type::Tuple(fields) => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, Self::type_to_string(&f.ty)))
                    .collect();
                format!("({})", field_types.join(", "))
            }
            Type::Generic { name, args, .. } => {
                if args.is_empty() {
                    name.name.clone()
                } else {
                    let arg_types: Vec<String> =
                        args.iter().map(|arg| Self::type_to_string(arg)).collect();
                    format!("{}<{}>", name.name, arg_types.join(", "))
                }
            }
            Type::TypeParameter(param) => param.name.clone(),
            Type::Dictionary { key, value } => {
                format!(
                    "[{}: {}]",
                    Self::type_to_string(key),
                    Self::type_to_string(value)
                )
            }
            Type::Closure { params, ret } => {
                if params.is_empty() {
                    format!("() -> {}", Self::type_to_string(ret))
                } else if let Some(only_param) = params.first().filter(|_| params.len() == 1) {
                    format!(
                        "{} -> {}",
                        Self::type_to_string(only_param),
                        Self::type_to_string(ret)
                    )
                } else {
                    let param_types: Vec<String> =
                        params.iter().map(|p| Self::type_to_string(p)).collect();
                    format!("{} -> {}", param_types.join(", "), Self::type_to_string(ret))
                }
            }
        }
    }

    /// Check if two type strings are compatible
    ///
    /// This handles:
    /// - Exact matches
    /// - Number/f32/i32/u32 compatibility
    /// - Unknown/placeholder type params
    /// - `InferredEnum` matching enum types
    pub(super) fn type_strings_compatible(&self, expected: &str, actual: &str) -> bool {
        // Exact match
        if expected == actual {
            return true;
        }

        // Allow placeholder types to match anything (incomplete type inference)
        if actual == "Unknown" || actual.ends_with("Result") || actual.starts_with("FunctionResult")
        {
            return true;
        }

        // InferredEnum is compatible with any declared enum type
        // This handles `.variant(...)` syntax where the enum type is inferred from context
        if actual == "InferredEnum" && self.symbols.enums.contains_key(expected) {
            return true;
        }

        // Number is compatible with f32/i32/u32 for GPU types
        if expected == "Number" && (actual == "f32" || actual == "i32" || actual == "u32") {
            return true;
        }
        if actual == "Number" && (expected == "f32" || expected == "i32" || expected == "u32") {
            return true;
        }

        // Boolean and bool are compatible
        if (expected == "Boolean" && actual == "bool")
            || (expected == "bool" && actual == "Boolean")
        {
            return true;
        }

        false
    }

    /// Check if a type satisfies a trait constraint
    ///
    /// A type satisfies a trait constraint if:
    /// 1. It's a struct that implements the trait (via : Trait or impl Trait for Struct)
    /// 2. It's an enum that implements the trait
    /// 3. It's a type parameter that has the constraint in scope
    pub(super) fn type_satisfies_trait_constraint(&self, ty: &Type, trait_name: &str) -> bool {
        match ty {
            Type::Ident(ident) => {
                // Check if struct implements the trait
                if let Some(struct_info) = self.symbols.get_struct(&ident.name) {
                    // Check inline traits (struct Foo: Trait)
                    if struct_info.traits.iter().any(|t| t == trait_name) {
                        return true;
                    }
                }
                // Check trait impls (impl Trait for Struct)
                let all_traits = self.symbols.get_all_traits_for_struct(&ident.name);
                if all_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                // Check if enum implements the trait
                let enum_traits = self.symbols.get_all_traits_for_enum(&ident.name);
                if enum_traits.contains(&trait_name.to_string()) {
                    return true;
                }
                false
            }
            Type::Generic { name, .. } => {
                // For generic types, check if the base type implements the trait
                if let Some(struct_info) = self.symbols.get_struct(&name.name) {
                    if struct_info.traits.iter().any(|t| t == trait_name) {
                        return true;
                    }
                }
                let all_traits = self.symbols.get_all_traits_for_struct(&name.name);
                all_traits.contains(&trait_name.to_string())
            }
            Type::TypeParameter(param) => {
                // Check if the type parameter has the constraint in scope
                if let Some(constraints) = self.get_type_parameter_constraints(&param.name) {
                    return constraints.contains(&trait_name.to_string());
                }
                false
            }
            // Primitives, arrays, optionals, tuples, etc. don't implement user-defined traits
            Type::Primitive(_)
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. } => false,
        }
    }
}
