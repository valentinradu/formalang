use super::super::module_resolver::ModuleResolver;
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
                            self.validate_impl_trait_methods(
                                &impl_def.functions,
                                &trait_ident.name,
                                &impl_def.trait_args,
                                &impl_def.name.name,
                                impl_def.span,
                            );
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
                    {
                        let expected = required_return
                            .as_ref()
                            .map_or_else(|| "()".to_string(), Self::type_to_string);
                        let actual = impl_fn
                            .return_type
                            .as_ref()
                            .map_or_else(|| "()".to_string(), Self::type_to_string);
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
