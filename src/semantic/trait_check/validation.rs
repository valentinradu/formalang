use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, File, FnDef, Statement, StructDef, Type};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Pass 4: Validate trait implementations
    /// Check that structs implement all required fields from their traits,
    /// and that impl Trait for Struct blocks provide all required methods.
    pub(in crate::semantic) fn validate_trait_implementations(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                match &**def {
                    Definition::Struct(struct_def) => {
                        self.validate_struct_trait_implementation(struct_def);
                    }
                    Definition::Impl(impl_def) => {
                        if let Some(trait_ident) = &impl_def.trait_name {
                            if self.symbols.get_trait(&trait_ident.name).is_some() {
                                self.check_trait_arity(
                                    &trait_ident.name,
                                    impl_def.trait_args.len(),
                                    trait_ident.span,
                                );
                            }
                            self.validate_impl_trait_methods(
                                &impl_def.functions,
                                &trait_ident.name,
                                &impl_def.trait_args,
                                &impl_def.name.name,
                                impl_def.span,
                            );
                            self.validate_impl_trait_scope(impl_def, &trait_ident.name);
                        }
                    }
                    Definition::Trait(_)
                    | Definition::Enum(_)
                    | Definition::Module(_)
                    | Definition::Function(_) => {}
                }
            }
        }
    }

    /// Check that an `impl Trait for Struct` block provides all methods declared in the trait.
    ///
    /// Generic-traits PR: when the impl is `impl Foo<X, Y> for Z`, the
    /// `trait_args` slot carries the concrete arg types and the
    /// trait's required-method signatures get their generic params
    /// substituted before comparison. Without this, `impl Eq<I32>
    /// for Foo` would always report a `TraitMethodSignatureMismatch`
    /// because the trait declares `fn eq(self, other: T)` and the
    /// impl declares `fn eq(self, other: I32)`.
    fn validate_impl_trait_methods(
        &mut self,
        impl_functions: &[FnDef],
        trait_name: &str,
        trait_args: &[Type],
        _struct_name: &str,
        impl_span: crate::location::Span,
    ) {
        // Collect all required methods from the trait (including composed traits)
        let required_methods = self.collect_all_trait_methods(trait_name);

        // Build trait-param to concrete-arg substitution map. Empty
        // when the trait isn't generic or no args were supplied.
        let trait_generic_params: Vec<String> = self
            .symbols
            .get_trait(trait_name)
            .map(|info| info.generics.iter().map(|g| g.name.name.clone()).collect())
            .unwrap_or_default();
        let subs: std::collections::HashMap<String, Type> = trait_generic_params
            .iter()
            .zip(trait_args.iter())
            .map(|(name, arg)| (name.clone(), arg.clone()))
            .collect();

        for (method_name, required_params, required_return) in required_methods {
            let required_params: Vec<crate::ast::FnParam> = required_params
                .into_iter()
                .map(|mut p| {
                    if let Some(t) = &mut p.ty {
                        Self::substitute_type_params(t, &subs);
                    }
                    p
                })
                .collect();
            let required_return = required_return.map(|mut t| {
                Self::substitute_type_params(&mut t, &subs);
                t
            });
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
                    // Check: param count (excluding self), conventions, and return type
                    let required_non_self: Vec<_> = required_params
                        .iter()
                        .filter(|p| p.name.name != "self")
                        .collect();
                    let impl_non_self: Vec<_> = impl_fn
                        .params
                        .iter()
                        .filter(|p| p.name.name != "self")
                        .collect();

                    let param_count_mismatch = impl_non_self.len() != required_non_self.len();

                    let convention_mismatch = !param_count_mismatch
                        && required_non_self
                            .iter()
                            .zip(impl_non_self.iter())
                            .any(|(req, imp)| req.convention != imp.convention);

                    // also compare parameter *types*.
                    // Previously only arity and conventions were checked, so
                    // an impl could return `fn foo(x: Int)` for a trait
                    // method declared `fn foo(x: String)` without error.
                    // A label is part of the requirement: a call through
                    // a bound names the trait's label.
                    let label_mismatch = !param_count_mismatch
                        && required_non_self
                            .iter()
                            .zip(impl_non_self.iter())
                            .any(|(req, imp)| call_label(req) != call_label(imp));

                    let param_type_mismatch = !param_count_mismatch
                        && required_non_self
                            .iter()
                            .zip(impl_non_self.iter())
                            .any(|(req, imp)| match (&req.ty, &imp.ty) {
                                (Some(req_ty), Some(imp_ty)) => !Self::types_match(req_ty, imp_ty),
                                (None, None) => false,
                                _ => true,
                            });

                    // Also check self convention if both have self
                    let self_convention_mismatch = {
                        let req_self = required_params.iter().find(|p| p.name.name == "self");
                        let imp_self = impl_fn.params.iter().find(|p| p.name.name == "self");
                        match (req_self, imp_self) {
                            (Some(r), Some(i)) => r.convention != i.convention,
                            _ => false,
                        }
                    };

                    let return_type_mismatch = match (&required_return, &impl_fn.return_type) {
                        (Some(req_ret), Some(impl_ret)) => !Self::types_match(req_ret, impl_ret),
                        (None, None) => false,
                        _ => true,
                    };

                    if param_count_mismatch
                        || convention_mismatch
                        || self_convention_mismatch
                        || return_type_mismatch
                        || param_type_mismatch
                        || label_mismatch
                    {
                        // The whole signatures: the part that differs may
                        // be a parameter, not the return type.
                        let expected = signature(&required_params, required_return.as_ref());
                        let actual = signature(&impl_fn.params, impl_fn.return_type.as_ref());
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

    /// Check what an `impl Trait for T` block covers beyond the
    /// trait's own methods.
    ///
    /// Each trait of a hierarchy has its own impl block. So a trait
    /// that `trait_name` composes needs its own impl for the type, and
    /// a method that only a composed trait declares does not belong in
    /// this block. An enum has no fields, so it cannot meet a field
    /// requirement.
    fn validate_impl_trait_scope(&mut self, impl_def: &crate::ast::ImplDef, trait_name: &str) {
        let type_name = &impl_def.name.name;
        if self.symbols.get_enum_qualified(type_name).is_some() {
            for (field, _) in self.symbols.get_all_trait_fields(trait_name) {
                self.errors.push(CompilerError::MissingTraitField {
                    field,
                    trait_name: trait_name.to_string(),
                    span: impl_def.span,
                });
            }
        }
        let own: Vec<String> = self
            .symbols
            .get_trait(trait_name)
            .map(|t| t.methods.iter().map(|m| m.name.name.clone()).collect())
            .unwrap_or_default();
        let implemented: Vec<String> = self
            .symbols
            .trait_impls
            .get(type_name)
            .map(|impls| impls.iter().map(|i| i.trait_name.clone()).collect())
            .unwrap_or_default();
        for parent in self.composed_traits_of(trait_name) {
            let parent_methods: Vec<String> = self
                .symbols
                .get_trait(&parent)
                .map(|t| t.methods.iter().map(|m| m.name.name.clone()).collect())
                .unwrap_or_default();
            for func in &impl_def.functions {
                if parent_methods.contains(&func.name.name) && !own.contains(&func.name.name) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "method '{}' of trait {parent} in the impl of {trait_name}",
                            func.name.name
                        ),
                        span: func.name.span,
                    });
                }
            }
            if !implemented.contains(&parent) {
                for method in parent_methods {
                    self.errors.push(CompilerError::MissingTraitMethod {
                        method,
                        trait_name: parent.clone(),
                        span: impl_def.span,
                    });
                }
            }
        }
    }

    /// Every trait that `trait_name` composes, directly or through
    /// another composed trait.
    fn composed_traits_of(&self, trait_name: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut pending: Vec<String> = self
            .symbols
            .get_trait(trait_name)
            .map(|t| t.composed_traits.clone())
            .unwrap_or_default();
        while let Some(next) = pending.pop() {
            if next == trait_name || out.contains(&next) {
                continue;
            }
            if let Some(info) = self.symbols.get_trait(&next) {
                pending.extend(info.composed_traits.iter().cloned());
            }
            out.push(next);
        }
        out
    }

    /// Collect the methods declared directly in a trait (not inherited ones).
    ///
    /// Each `impl Trait for Struct` provides only the methods declared
    /// directly in that trait. Methods inherited from composed traits are
    /// covered by separate impl blocks for those base traits; this is a
    /// deliberate design choice documented in the language reference.
    fn collect_all_trait_methods(
        &self,
        trait_name: &str,
    ) -> Vec<(String, Vec<crate::ast::FnParam>, Option<Type>)> {
        self.symbols
            .traits
            .get(trait_name)
            .map_or_else(Vec::new, |trait_info| {
                trait_info
                    .methods
                    .iter()
                    .map(|m| (m.name.name.clone(), m.params.clone(), m.return_type.clone()))
                    .collect()
            })
    }

    /// Validate that a struct implements all required fields from its traits
    pub(in crate::semantic) fn validate_struct_trait_implementation(
        &mut self,
        struct_def: &StructDef,
    ) {
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
}

/// The label a call gives a parameter: its external label, or its name.
fn call_label(param: &crate::ast::FnParam) -> &str {
    param
        .external_label
        .as_ref()
        .map_or(param.name.name.as_str(), |l| l.name.as_str())
}

/// A method signature as a message shows it: `(self, by: I32) -> I32`.
fn signature(params: &[crate::ast::FnParam], ret: Option<&Type>) -> String {
    let rendered: Vec<String> = params
        .iter()
        .map(|p| {
            if p.name.name == "self" {
                return "self".to_string();
            }
            let ty =
                p.ty.as_ref()
                    .map_or_else(String::new, |t| SemType::from_ast(t).display());
            format!("{}: {ty}", call_label(p))
        })
        .collect();
    let ret = ret.map_or_else(|| "()".to_string(), |t| SemType::from_ast(t).display());
    format!("({}) -> {ret}", rendered.join(", "))
}
