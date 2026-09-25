//! Generic-parameter scoping, let-binding type inference, and per-pattern
//! type resolution.
//!
//! Owns:
//! - Pass 1.5 (`validate_generic_parameters`) — duplicate-parameter and
//!   constraint-trait-existence checks, and the checks on the type
//!   parameters of a method.
//! - Pass 1.6 (`infer_let_types`) — folds the inferred or annotated value
//!   type into each binding produced by a let pattern.
//! - The generic-scope stack (`push_generic_scope` / `pop_generic_scope`
//!   and friends) consulted by later passes when resolving type references
//!   inside generic-aware contexts.

use super::module_resolver::ModuleResolver;
use super::sem_type::SemType;
use super::SemanticAnalyzer;
use crate::ast::{Definition, File, Statement};
use crate::error::CompilerError;
use std::collections::{HashMap, HashSet};

/// Tracks generic parameters in scope for a definition
#[derive(Debug, Clone)]
pub(super) struct GenericScope {
    /// Generic parameter names and their constraints
    pub(super) params: HashMap<String, Vec<String>>, // name -> list of trait constraints
    /// The trait arguments of each bound: `I32` in `T: Container<I32>`.
    /// Keyed by the parameter, then by the trait.
    pub(super) bound_args: HashMap<String, Bounds>,
}

/// The bounds of one type parameter: each trait, with its arguments.
pub(super) type Bounds = Vec<(String, Vec<crate::ast::Type>)>;

/// The bounds of `param`, each as its trait and its trait arguments.
fn bounds_of(param: &crate::ast::GenericParam) -> Bounds {
    param
        .constraints
        .iter()
        .map(|c| match c {
            crate::ast::GenericConstraint::Trait { name, args } => {
                (name.name.clone(), args.clone())
            }
        })
        .collect()
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 1.5: Validate generic parameters
    /// Check for duplicate parameters and validate constraints
    pub(super) fn validate_generic_parameters(&mut self, file: &File) {
        let definitions = file
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Definition(def) => Some(&**def),
                Statement::Use(_) | Statement::Let(_) => None,
            });
        self.validate_generics_of(definitions);
    }

    /// Check the generic parameters of each definition in
    /// `definitions`, and of each definition in an inline module. The
    /// module's own names are in scope inside it, so a constraint there
    /// may name a trait of the module.
    fn validate_generics_of<'d>(&mut self, definitions: impl Iterator<Item = &'d Definition>) {
        for def in definitions {
            match def {
                Definition::Trait(trait_def) => {
                    self.validate_generic_list(&trait_def.generics, &[]);
                    for method in &trait_def.methods {
                        if let Some(first) = method.generics.first() {
                            self.errors.push(CompilerError::GenericTraitMethod {
                                method: method.name.name.clone(),
                                span: first.span,
                            });
                        }
                    }
                }
                Definition::Struct(struct_def) => {
                    self.validate_generic_list(&struct_def.generics, &[]);
                }
                Definition::Impl(impl_def) => {
                    self.validate_generic_list(&impl_def.generics, &[]);
                    // `impl Box { ... }` may leave the type's parameters
                    // out; the type's own declaration names them then.
                    let outer = if impl_def.generics.is_empty() {
                        let name = &impl_def.name.name;
                        self.symbols
                            .structs
                            .get(name)
                            .map(|s| s.generics.clone())
                            .or_else(|| self.symbols.enums.get(name).map(|e| e.generics.clone()))
                            .unwrap_or_default()
                    } else {
                        impl_def.generics.clone()
                    };
                    for method in &impl_def.functions {
                        self.validate_method_generics(method, &outer);
                    }
                }
                Definition::Enum(enum_def) => {
                    self.validate_generic_list(&enum_def.generics, &[]);
                }
                Definition::Function(func_def) => {
                    self.validate_generic_list(&func_def.generics, &[]);
                }
                Definition::Module(module_def) => {
                    let shadowed = self.enter_module_scope(module_def);
                    self.validate_generics_of(module_def.definitions.iter());
                    self.leave_module_scope(shadowed);
                }
            }
        }
    }

    /// Check the type parameters that a method declares.
    ///
    /// A method call takes no `<...>`, so each type parameter must
    /// appear in the type of a parameter with no default: the arguments
    /// that every call gives must give it its type. A name that the impl block or its type declares already is
    /// a duplicate, because the method could not name the outer one.
    fn validate_method_generics(
        &mut self,
        method: &crate::ast::FnDef,
        impl_generics: &[crate::ast::GenericParam],
    ) {
        self.validate_generic_list(&method.generics, impl_generics);
        for generic in &method.generics {
            let name = std::slice::from_ref(&generic.name.name);
            // A parameter with a default may be left out, and then it
            // gives no type.
            let mentioned = method.params.iter().any(|p| {
                p.default.is_none()
                    && p.ty.as_ref().is_some_and(|ty| {
                        super::validation::type_names::type_mentions_any(ty, name)
                    })
            });
            if !mentioned {
                self.errors
                    .push(CompilerError::UninferableMethodTypeParameter {
                        param: generic.name.name.clone(),
                        method: method.name.name.clone(),
                        span: generic.span,
                    });
            }
        }
    }

    /// Report a parameter of `generics` that repeats a name of
    /// `generics` or of `outer`, and a constraint that names no trait.
    fn validate_generic_list(
        &mut self,
        generics: &[crate::ast::GenericParam],
        outer: &[crate::ast::GenericParam],
    ) {
        use crate::ast::GenericConstraint;

        let mut seen_params: HashSet<&String> = outer.iter().map(|p| &p.name.name).collect();
        for param in generics {
            if !seen_params.insert(&param.name.name) {
                self.errors.push(CompilerError::DuplicateGenericParam {
                    param: param.name.name.clone(),
                    span: param.span,
                });
            }

            for constraint in &param.constraints {
                let GenericConstraint::Trait { name, args } = constraint;
                if self.symbols.is_trait(&name.name) {
                    self.check_trait_arity(&name.name, args.len(), name.span);
                } else if self.symbols.is_struct(&name.name) || self.symbols.is_enum(&name.name) {
                    let actual_kind = if self.symbols.is_struct(&name.name) {
                        "struct"
                    } else {
                        "enum"
                    };
                    self.errors.push(CompilerError::NotATrait {
                        name: name.name.clone(),
                        actual_kind: actual_kind.to_string(),
                        span: name.span,
                    });
                } else {
                    self.errors.push(CompilerError::UndefinedTrait {
                        name: name.name.clone(),
                        span: name.span,
                    });
                }
            }
        }
    }

    /// Check that `actual` trait arguments fit the type parameters of
    /// the trait `trait_name`: a generic trait needs all of them, and
    /// a trait with none takes none.
    pub(super) fn check_trait_arity(
        &mut self,
        trait_name: &str,
        actual: usize,
        span: crate::location::Span,
    ) {
        let expected = self
            .symbols
            .get_trait(trait_name)
            .map_or(0, |info| info.generics.len());
        if actual == expected {
            return;
        }
        if actual == 0 {
            self.errors.push(CompilerError::MissingGenericArguments {
                name: trait_name.to_string(),
                span,
            });
        } else {
            self.errors.push(CompilerError::GenericArityMismatch {
                name: trait_name.to_string(),
                expected,
                actual,
                span,
            });
        }
    }

    /// Pass 1.6: Infer let binding types
    /// Infer the type of each let binding from its value expression, preferring
    /// the explicit type annotation when one is present. Each binding in a
    /// destructuring pattern picks up the [`SemType`] at its pattern position
    /// (array element, tuple field, struct field) via
    /// [`Self::pattern_binding_types`]; simple patterns get the full source type.
    pub(super) fn infer_let_types(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Let(let_binding) = statement {
                let source_ty = let_binding.type_annotation.as_ref().map_or_else(
                    || self.infer_type_sem(&let_binding.value, file),
                    SemType::from_ast,
                );
                for (name, ty) in self.pattern_binding_types(&let_binding.pattern, &source_ty, file)
                {
                    self.symbols.set_let_type(&name, ty);
                }
            }
        }
    }

    /// Push a generic scope for a definition with generic parameters
    pub(super) fn push_generic_scope(&mut self, generics: &[crate::ast::GenericParam]) {
        let mut scope = GenericScope {
            params: HashMap::new(),
            bound_args: HashMap::new(),
        };

        for param in generics {
            scope
                .bound_args
                .insert(param.name.name.clone(), bounds_of(param));
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
            bound_args: HashMap::new(),
        };
        // Start with the impl's own generic param names (often constraint-less).
        for param in impl_generics {
            scope
                .bound_args
                .insert(param.name.name.clone(), bounds_of(param));
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
            scope
                .bound_args
                .entry(param.name.name.clone())
                .or_default()
                .extend(bounds_of(param));
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
