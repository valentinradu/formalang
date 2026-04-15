use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, FnDef, PrimitiveType, Statement, StructDef, Type};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 4: Validate trait implementations
    /// Check that structs implement all required fields from their traits,
    /// and that impl Trait for Struct blocks provide all required methods.
    pub(super) fn validate_trait_implementations(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Struct(struct_def) => {
                        self.validate_struct_trait_implementation(struct_def);
                    }
                    Definition::Impl(impl_def) => {
                        if let Some(trait_ident) = &impl_def.trait_name {
                            self.validate_impl_trait_methods(
                                &impl_def.functions,
                                &trait_ident.name,
                                &impl_def.name.name,
                                impl_def.span,
                            );
                        }
                    }
                    Definition::Trait(_)
                    | Definition::Enum(_)
                    | Definition::Module(_)
                    | Definition::Function(_)
                    | Definition::ExternType(_) => {}
                }
            }
        }
    }

    /// Check that an `impl Trait for Struct` block provides all methods declared in the trait.
    fn validate_impl_trait_methods(
        &mut self,
        impl_functions: &[FnDef],
        trait_name: &str,
        _struct_name: &str,
        impl_span: crate::location::Span,
    ) {
        // Collect all required methods from the trait (including composed traits)
        let required_methods = self.collect_all_trait_methods(trait_name);

        for (method_name, required_params, required_return) in required_methods {
            // Find this method in the impl block
            match impl_functions.iter().find(|f| f.name.name == method_name) {
                None => {
                    self.errors.push(CompilerError::MissingTraitMethod {
                        method: method_name.clone(),
                        trait_name: trait_name.to_string(),
                        span: impl_span,
                    });
                }
                Some(impl_fn) => {
                    // Check signature match: param count (excluding self) and return type
                    let impl_non_self_params: Vec<_> = impl_fn
                        .params
                        .iter()
                        .filter(|p| p.name.name != "self")
                        .collect();
                    let required_non_self: Vec<_> = required_params
                        .iter()
                        .filter(|p| p.name.name != "self")
                        .collect();

                    let param_count_mismatch =
                        impl_non_self_params.len() != required_non_self.len();

                    let return_type_mismatch = match (&required_return, &impl_fn.return_type) {
                        (Some(req_ret), Some(impl_ret)) => !Self::types_match(req_ret, impl_ret),
                        (None, None) => false,
                        _ => true,
                    };

                    if param_count_mismatch || return_type_mismatch {
                        let expected = required_return
                            .as_ref()
                            .map(Self::type_to_string)
                            .unwrap_or_else(|| "()".to_string());
                        let actual = impl_fn
                            .return_type
                            .as_ref()
                            .map(Self::type_to_string)
                            .unwrap_or_else(|| "()".to_string());
                        self.errors
                            .push(CompilerError::TraitMethodSignatureMismatch {
                                method: method_name.clone(),
                                trait_name: trait_name.to_string(),
                                expected,
                                actual,
                                span: impl_fn.span,
                            });
                    }
                }
            }
        }
    }

    /// Collect the methods declared directly in a trait (not inherited ones).
    ///
    /// Each `impl Trait for Struct` block must provide only the methods declared
    /// directly in that trait. Inherited methods from composed traits are expected
    /// to be covered by separate impl blocks for those base traits.
    fn collect_all_trait_methods(
        &self,
        trait_name: &str,
    ) -> Vec<(String, Vec<crate::ast::FnParam>, Option<Type>)> {
        if let Some(trait_info) = self.symbols.traits.get(trait_name) {
            trait_info
                .methods
                .iter()
                .map(|m| (m.name.name.clone(), m.params.clone(), m.return_type.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Validate that a struct implements all required fields from its traits
    pub(super) fn validate_struct_trait_implementation(&mut self, struct_def: &StructDef) {
        // For each implemented trait, check required fields via impl blocks
        // (trait field validation is handled through impl Trait for Struct)
        // Walk through trait_impls for this struct
        let struct_name = struct_def.name.name.clone();
        let trait_impls: Vec<String> = self
            .symbols
            .trait_impls
            .get(&struct_name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.trait_name)
            .collect();

        for trait_name in &trait_impls {
            // Get all required fields from this trait (including composed traits)
            let required_fields = self.symbols.get_all_trait_fields(trait_name);

            // Check each required field
            for (field_name, required_type) in required_fields {
                // Look for the field in the struct
                match struct_def.fields.iter().find(|f| f.name.name == field_name) {
                    Some(struct_field) => {
                        // Field exists, check type matches
                        if !Self::types_match(&struct_field.ty, &required_type) {
                            self.errors.push(CompilerError::TraitFieldTypeMismatch {
                                field: field_name.clone(),
                                trait_name: trait_name.clone(),
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
                            trait_name: trait_name.clone(),
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
                    format!(
                        "{} -> {}",
                        param_types.join(", "),
                        Self::type_to_string(ret)
                    )
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
