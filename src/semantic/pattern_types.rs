//! Generic-parameter scoping, let-binding type inference, and per-pattern
//! type resolution.
//!
//! Owns:
//! - Pass 1.5 (`validate_generic_parameters`) — duplicate-parameter and
//!   constraint-trait-existence checks.
//! - Pass 1.6 (`infer_let_types`) — folds the inferred or annotated value
//!   type into each binding produced by a let pattern.
//! - The generic-scope stack (`push_generic_scope` / `pop_generic_scope`
//!   and friends) consulted by later passes when resolving type references
//!   inside generic-aware contexts.

use super::helpers::{parse_tuple_field_types, strip_array_type};
use super::module_resolver::ModuleResolver;
use super::SemanticAnalyzer;
use crate::ast::{ArrayPatternElement, BindingPattern, Definition, File, Statement};
use crate::error::CompilerError;
use std::collections::{HashMap, HashSet};

/// Tracks generic parameters in scope for a definition
#[derive(Debug, Clone)]
pub(super) struct GenericScope {
    /// Generic parameter names and their constraints
    pub(super) params: HashMap<String, Vec<String>>, // name -> list of trait constraints
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 1.5: Validate generic parameters
    /// Check for duplicate parameters and validate constraints
    pub(super) fn validate_generic_parameters(&mut self, file: &File) {
        use crate::ast::GenericConstraint;

        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                let generics = match &**def {
                    Definition::Trait(trait_def) => &trait_def.generics,
                    Definition::Struct(struct_def) => &struct_def.generics,
                    Definition::Impl(impl_def) => &impl_def.generics,
                    Definition::Enum(enum_def) => &enum_def.generics,
                    Definition::Function(func_def) => &func_def.generics,
                    // Module definitions don't carry generics themselves;
                    // nested definitions are validated via their own arms.
                    Definition::Module(_) => continue,
                };

                // Check for duplicate generic parameters
                let mut seen_params = HashSet::new();
                for param in generics {
                    if !seen_params.insert(&param.name.name) {
                        self.errors.push(CompilerError::DuplicateGenericParam {
                            param: param.name.name.clone(),
                            span: param.span,
                        });
                    }

                    // Validate constraints reference valid traits
                    for constraint in &param.constraints {
                        match constraint {
                            GenericConstraint::Trait { name, .. } => {
                                if !self.symbols.is_trait(&name.name) {
                                    self.errors.push(CompilerError::UndefinedTrait {
                                        name: name.name.clone(),
                                        span: name.span,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Pass 1.6: Infer let binding types
    /// Infer the type of each let binding from its value expression, preferring
    /// the explicit type annotation when one is present.
    pub(super) fn infer_let_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                let source_type = let_binding.type_annotation.as_ref().map_or_else(
                    || self.infer_type(&let_binding.value, file),
                    Self::type_to_string,
                );
                // Each binding in a destructuring pattern gets the type of the
                // position it extracts (array element, tuple field, struct field).
                // Simple patterns get the full source type.
                let resolved = self.resolve_pattern_types(&let_binding.pattern, &source_type);
                for (name, ty) in resolved {
                    self.symbols.set_let_type(&name, ty);
                }
            }
        }
    }

    /// Resolve per-binding types for a destructuring pattern given the
    /// source type string. Falls back to the source type for bindings whose
    /// position cannot be resolved (e.g., unknown struct field).
    fn resolve_pattern_types(
        &self,
        pattern: &BindingPattern,
        source_ty: &str,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        self.collect_pattern_types_inner(pattern, source_ty, &mut out);
        out
    }

    fn collect_pattern_types_inner(
        &self,
        pattern: &BindingPattern,
        source_ty: &str,
        out: &mut Vec<(String, String)>,
    ) {
        match pattern {
            BindingPattern::Simple(ident) => {
                out.push((ident.name.clone(), source_ty.to_string()));
            }
            BindingPattern::Array { elements, .. } => {
                let element_ty = strip_array_type(source_ty).unwrap_or(source_ty);
                for element in elements {
                    match element {
                        ArrayPatternElement::Binding(inner) => {
                            self.collect_pattern_types_inner(inner, element_ty, out);
                        }
                        ArrayPatternElement::Rest(Some(ident)) => {
                            out.push((ident.name.clone(), source_ty.to_string()));
                        }
                        ArrayPatternElement::Rest(None) | ArrayPatternElement::Wildcard => {}
                    }
                }
            }
            BindingPattern::Tuple { elements, .. } => {
                let field_types = parse_tuple_field_types(source_ty);
                for (idx, element) in elements.iter().enumerate() {
                    let inner_ty = field_types
                        .get(idx)
                        .map_or(source_ty, std::string::String::as_str);
                    self.collect_pattern_types_inner(element, inner_ty, out);
                }
            }
            BindingPattern::Struct { fields, .. } => {
                for field in fields {
                    let binding_ident = field.alias.as_ref().unwrap_or(&field.name);
                    let field_ty = self
                        .symbols
                        .structs
                        .get(source_ty)
                        .and_then(|s| s.fields.iter().find(|f| f.name == field.name.name))
                        .map_or_else(|| source_ty.to_string(), |f| Self::type_to_string(&f.ty));
                    out.push((binding_ident.name.clone(), field_ty));
                }
            }
        }
    }

    /// Push a generic scope for a definition with generic parameters
    pub(super) fn push_generic_scope(&mut self, generics: &[crate::ast::GenericParam]) {
        let mut scope = GenericScope {
            params: HashMap::new(),
        };

        for param in generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait { name, .. } => name.name.clone(),
                })
                .collect();

            scope.params.insert(param.name.name.clone(), constraints);
        }

        self.generic_scopes.push(scope);
    }

    /// Push a generic scope for an impl block that combines the impl's own
    /// `<T>` parameters with the constraints declared on the target
    /// struct/enum. `impl Sum<T>` carries the param name without
    /// constraints; the constraints (`T: Foo`) live on `struct Sum<T: Foo>`.
    /// Without merging, methods inside the impl can't see the trait
    /// bounds on T.
    pub(super) fn push_impl_generic_scope(
        &mut self,
        impl_generics: &[crate::ast::GenericParam],
        target_name: &str,
    ) {
        let mut scope = GenericScope {
            params: HashMap::new(),
        };
        // Start with the impl's own generic param names (often constraint-less).
        for param in impl_generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait { name, .. } => name.name.clone(),
                })
                .collect();
            scope.params.insert(param.name.name.clone(), constraints);
        }
        // Merge constraints from the target struct/enum's own generics.
        let target_generics = if let Some(s) = self.symbols.structs.get(target_name) {
            s.generics.clone()
        } else if let Some(e) = self.symbols.enums.get(target_name) {
            e.generics.clone()
        } else {
            Vec::new()
        };
        for param in &target_generics {
            let constraints: Vec<String> = param
                .constraints
                .iter()
                .map(|c| match c {
                    crate::ast::GenericConstraint::Trait { name, .. } => name.name.clone(),
                })
                .collect();
            let entry = scope.params.entry(param.name.name.clone()).or_default();
            for c in constraints {
                if !entry.contains(&c) {
                    entry.push(c);
                }
            }
        }
        self.generic_scopes.push(scope);
    }

    /// Pop the current generic scope
    pub(super) fn pop_generic_scope(&mut self) {
        self.generic_scopes.pop();
    }

    /// Check if a name is a type parameter in the current generic scopes
    pub(super) fn is_type_parameter(&self, name: &str) -> bool {
        // Search from the most recent scope backwards
        for scope in self.generic_scopes.iter().rev() {
            if scope.params.contains_key(name) {
                return true;
            }
        }
        false
    }

    /// Get the constraints for a type parameter if it's in scope
    pub(super) fn get_type_parameter_constraints(&self, name: &str) -> Option<Vec<String>> {
        // Search from the most recent scope backwards
        for scope in self.generic_scopes.iter().rev() {
            if let Some(constraints) = scope.params.get(name) {
                return Some(constraints.clone());
            }
        }
        None
    }
}
