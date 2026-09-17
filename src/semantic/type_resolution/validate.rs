//! Type-reference validation: walks each `Type` and reports references to
//! undefined / mis-kinded names, generic-arity mismatches, and constraint
//! violations. Also exposes graph-building helpers used by the cycle pass.

use super::super::module_resolver::ModuleResolver;
use super::super::type_graph::TypeGraph;
use super::super::SemanticAnalyzer;
use crate::ast::Type;
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a type reference (recursive over compound shapes).
    /// `span` locates the declaration this type belongs to. A `Type`
    /// carries no span of its own except on `Ident` and `Generic`, so
    /// the caller supplies one for the shapes that lack it.
    pub(in crate::semantic) fn validate_type(&mut self, ty: &Type, span: Span) {
        match ty {
            Type::Primitive(_) => {}
            Type::Ident(ident) => self.validate_ident_type(ident),
            Type::Array(element_ty) => self.validate_type(element_ty, span),
            Type::Optional(inner_ty) => self.validate_type(inner_ty, span),
            Type::Tuple(fields) => {
                for field in fields {
                    self.validate_type(&field.ty, field.span);
                }
            }
            Type::Generic {
                name,
                args,
                span: generic_span,
            } => self.validate_generic_type(name, args, *generic_span),
            Type::Dictionary { key, value } => {
                self.validate_dictionary_key(key, span);
                self.validate_type(key, span);
                self.validate_type(value, span);
            }
            Type::Closure { params, ret } => {
                for (_, param) in params {
                    self.validate_type(param, span);
                }
                self.validate_type(ret, span);
            }
        }
    }

    /// Reject `F32` and `F64` in a dictionary key position.
    ///
    /// A float compares badly: `NaN != NaN`, so a key can never be
    /// found again, and `0.0 == -0.0`, so two distinct-looking keys
    /// collide. Rust refuses the same thing, because `f64` is not
    /// `Eq`. Every other key type has a total equality the backend can
    /// hash.
    fn validate_dictionary_key(&mut self, key: &Type, span: Span) {
        let key_type = match key {
            Type::Primitive(crate::ast::PrimitiveType::F32) => "F32",
            Type::Primitive(crate::ast::PrimitiveType::F64) => "F64",
            Type::Primitive(_)
            | Type::Ident(_)
            | Type::Generic { .. }
            | Type::Array(_)
            | Type::Optional(_)
            | Type::Tuple(_)
            | Type::Dictionary { .. }
            | Type::Closure { .. } => return,
        };
        self.errors.push(CompilerError::FloatDictionaryKey {
            key_type: key_type.to_string(),
            span,
        });
    }

    /// Validate a simple identifier type (handles module paths and plain names).
    fn validate_ident_type(&mut self, ident: &crate::ast::Ident) {
        if ident.name.contains("::") {
            let parts: Vec<&str> = ident.name.split("::").collect();
            if parts.len() >= 2 {
                if let Some(error_msg) = self.resolve_nested_module_type(&parts, ident.span) {
                    self.errors.push(CompilerError::UndefinedType {
                        name: error_msg,
                        span: ident.span,
                    });
                }
            } else {
                self.errors.push(CompilerError::UndefinedType {
                    name: format!("invalid module path: {}", ident.name),
                    span: ident.span,
                });
            }
        } else if self.symbols.is_trait(&ident.name) {
            // A trait cannot be the type of a value (`let s: Shape`,
            // `[Shape]`, a trait-typed field or parameter). Every one
            // of those needs a vtable, and a machine-code backend has
            // to invent one per trait plus an indirect call at every
            // site. Traits stay compile-time constraints: `<T: Shape>`
            // for one concrete type, an enum for a mixed collection.
            //
            // `validate_type` is reached only from value positions —
            // field, parameter, return, let annotation, type argument
            // — so a generic constraint never lands here.
            self.errors.push(CompilerError::TraitUsedAsValueType {
                trait_name: ident.name.clone(),
                span: ident.span,
            });
        } else if self.symbols.is_type(&ident.name) || self.is_type_parameter(&ident.name) {
            // Valid struct/enum type or generic type parameter — OK.
        } else if ident.name.len() == 1 && ident.name.chars().next().is_some_and(char::is_uppercase)
        {
            self.errors.push(CompilerError::OutOfScopeTypeParameter {
                param: ident.name.clone(),
                span: ident.span,
            });
        } else {
            self.errors.push(CompilerError::UndefinedType {
                name: ident.name.clone(),
                span: ident.span,
            });
        }
    }

    /// Validate a generic type application (e.g., `Container<T, U>`).
    /// Recurses into nested arguments so violations at any depth are caught.
    fn validate_generic_type(
        &mut self,
        name: &crate::ast::Ident,
        args: &[Type],
        span: crate::location::Span,
    ) {
        if self.symbols.is_trait(&name.name) {
            // `Trait<X>` in a value position is also banned (no dynamic
            // dispatch). The fix is `<T: Trait<X>>` (currently unsupported,
            // see the generic-trait deferred PR).
            self.errors.push(CompilerError::TraitUsedAsValueType {
                trait_name: name.name.clone(),
                span: name.span,
            });
            return;
        }
        if !self.symbols.is_type(&name.name) {
            self.errors.push(CompilerError::UndefinedType {
                name: name.name.clone(),
                span: name.span,
            });
            return;
        }
        if let Some(expected_params) = self.symbols.get_generics(&name.name) {
            let expected = expected_params.len();
            let actual = args.len();
            if expected != actual {
                self.errors.push(CompilerError::GenericArityMismatch {
                    name: name.name.clone(),
                    expected,
                    actual,
                    span,
                });
            }
        }
        if let Some(expected_params) = self.symbols.get_generics(&name.name) {
            for (i, arg) in args.iter().enumerate() {
                if let Some(param) = expected_params.get(i) {
                    for constraint in &param.constraints {
                        let crate::ast::GenericConstraint::Trait {
                            name: trait_ref, ..
                        } = constraint;
                        if !self.type_satisfies_trait_constraint(arg, &trait_ref.name) {
                            self.errors.push(CompilerError::GenericConstraintViolation {
                                arg: Self::type_to_string(arg),
                                constraint: trait_ref.name.clone(),
                                span,
                            });
                        }
                    }
                }
            }
        }
        // `Dictionary<K, V>` is the same type as `[K: V]`, so the key
        // takes the same rule. Only the sugar reached
        // `validate_dictionary_key`, so `[F64: I32]` was rejected and
        // `Dictionary<F64, I32>` was not, and the float key reached the
        // backend.
        if name.name == "Dictionary" {
            if let Some(key) = args.first() {
                self.validate_dictionary_key(key, span);
            }
        }

        // Recurse into each argument; `validate_type` re-enters this for any
        // nested `Type::Generic` so inner constraints are checked too.
        for arg in args {
            self.validate_type(arg, span);
        }
    }

    /// Resolve a nested module type path like `["outer", "inner", "Type"]`.
    /// Returns `Some(error_message)` if the type doesn't exist, `None` if valid.
    pub(in crate::semantic) fn resolve_nested_module_type(
        &self,
        parts: &[&str],
        _span: Span,
    ) -> Option<String> {
        if parts.is_empty() {
            return Some("empty module path".to_string());
        }

        let Some((type_name, module_parts)) = parts.split_last() else {
            return Some("empty module path".to_string());
        };

        let mut current_symbols = &self.symbols;
        let mut path_so_far = String::new();

        for (i, module_name) in module_parts.iter().enumerate() {
            if i > 0 {
                path_so_far.push_str("::");
            }
            path_so_far.push_str(module_name);

            if let Some(module_info) = current_symbols.modules.get(*module_name) {
                current_symbols = &module_info.symbols;
            } else {
                return Some(format!("module '{path_so_far}' not found"));
            }
        }

        if !current_symbols.is_type(type_name) && !current_symbols.is_trait(type_name) {
            return Some(format!(
                "type '{type_name}' not found in module '{path_so_far}'"
            ));
        }

        None
    }

    /// Add type dependencies from a type expression to the cycle-detection
    /// graph. Recurses through arrays, optionals, tuples, generics, dicts,
    /// and closure parameter/return types.
    pub(in crate::semantic) fn add_type_dependencies(graph: &mut TypeGraph, from: &str, ty: &Type) {
        match ty {
            Type::Primitive(_) => {}
            Type::Ident(ident) => {
                graph.add_dependency(from.to_string(), ident.name.clone());
            }
            Type::Array(element_ty) => {
                // Arrays don't break cycles, so [Node] still creates Node -> Node.
                Self::add_type_dependencies(graph, from, element_ty);
            }
            Type::Optional(inner_ty) => {
                Self::add_type_dependencies(graph, from, inner_ty);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    Self::add_type_dependencies(graph, from, &field.ty);
                }
            }
            Type::Generic { name, args, .. } => {
                graph.add_dependency(from.to_string(), name.name.clone());
                for arg in args {
                    Self::add_type_dependencies(graph, from, arg);
                }
            }
            Type::Dictionary { key, value } => {
                Self::add_type_dependencies(graph, from, key);
                Self::add_type_dependencies(graph, from, value);
            }
            Type::Closure { params, ret } => {
                for (_, param) in params {
                    Self::add_type_dependencies(graph, from, param);
                }
                Self::add_type_dependencies(graph, from, ret);
            }
        }
    }
}
