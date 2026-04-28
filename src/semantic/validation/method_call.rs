//! Method-call validation: receiver/argument convention checks plus method
//! existence lookup across local impls, trait impls, generic constraints,
//! cached modules, and qualified-type module paths.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use super::qualified_types::{
    find_nested_module_definitions, impl_method_in_definitions, split_qualified_type,
};
use crate::ast::{Definition, Expr, File, Statement};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a method call expression
    pub(super) fn validate_expr_method_call(
        &mut self,
        receiver: &Expr,
        method: &crate::ast::Ident,
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        self.validate_expr(receiver, file);
        for (_, arg) in args {
            self.validate_expr(arg, file);
        }
        let receiver_type = self.infer_type_sem(receiver, file).display();
        if let Some(fn_def) = Self::find_method_fn_def(&receiver_type, &method.name, file) {
            let params = fn_def.params.clone();
            self.validate_fn_param_conventions_receiver(receiver, &params, span, file);
            self.validate_fn_param_conventions_args(&params, args, span, file);
        } else if !self.method_exists_on_type(&receiver_type, &method.name, file) {
            self.errors.push(CompilerError::UndefinedReference {
                name: format!("method '{}' on type '{}'", method.name, receiver_type),
                span,
            });
        }
    }

    /// Find the `FnDef` for `method_name` on the given type by scanning the file's impl blocks.
    fn find_method_fn_def<'f>(
        type_name: &str,
        method_name: &str,
        file: &'f File,
    ) -> Option<&'f crate::ast::FnDef> {
        if type_name == "Unknown" || type_name.contains("Unknown") {
            return None;
        }
        for stmt in &file.statements {
            if let crate::ast::Statement::Definition(def) = stmt {
                if let crate::ast::Definition::Impl(impl_def) = &**def {
                    if impl_def.name.name == type_name {
                        for func in &impl_def.functions {
                            if func.name.name == method_name {
                                return Some(func);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check `mut self` / `sink self` convention against the receiver expression.
    fn validate_fn_param_conventions_receiver(
        &mut self,
        receiver: &Expr,
        params: &[crate::ast::FnParam],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let Some(self_param) = params.iter().find(|p| p.name.name == "self") else {
            return;
        };
        match self_param.convention {
            ParamConvention::Mut => {
                if !self.is_expr_mutable(receiver, file) {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: "self".to_string(),
                        span,
                    });
                }
            }
            ParamConvention::Sink => {
                if let Some(root) = Self::root_binding(receiver) {
                    self.consumed_bindings.insert(root);
                }
            }
            ParamConvention::Let => {}
        }
    }

    /// Check `mut` / `sink` conventions on non-self parameters using AST `FnParam` directly.
    fn validate_fn_param_conventions_args(
        &mut self,
        params: &[crate::ast::FnParam],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        let non_self: Vec<_> = params.iter().filter(|p| p.name.name != "self").collect();
        for (i, (label_opt, arg_expr)) in args.iter().enumerate() {
            let param = label_opt.as_ref().map_or_else(
                || non_self.get(i).copied(),
                |label| {
                    non_self
                        .iter()
                        .find(|p| {
                            p.external_label
                                .as_ref()
                                .is_some_and(|l| l.name == label.name)
                                || p.name.name == label.name
                        })
                        .copied()
                },
            );
            if let Some(param) = param {
                if param.convention == ParamConvention::Mut && !self.is_expr_mutable(arg_expr, file)
                {
                    self.errors.push(CompilerError::MutabilityMismatch {
                        param: param.name.name.clone(),
                        span,
                    });
                }
                if param.convention == ParamConvention::Sink {
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
                }
            }
        }
    }

    /// Enforce closure param conventions at a call site where the callee is a closure binding.
    pub(super) fn validate_closure_call_conventions(
        &mut self,
        conventions: &[crate::ast::ParamConvention],
        args: &[(Option<crate::ast::Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        use crate::ast::ParamConvention;
        for (i, (_, arg_expr)) in args.iter().enumerate() {
            let Some(&convention) = conventions.get(i) else {
                break;
            };
            match convention {
                ParamConvention::Mut => {
                    if !self.is_expr_mutable(arg_expr, file) {
                        self.errors.push(CompilerError::MutabilityMismatch {
                            param: format!("arg{i}"),
                            span,
                        });
                    }
                }
                ParamConvention::Sink => {
                    if let Some(root) = Self::root_binding(arg_expr) {
                        self.consumed_bindings.insert(root);
                    }
                    // Escape analysis: sink-passed closure carries its captures away.
                    self.escape_closure_value(arg_expr);
                }
                ParamConvention::Let => {}
            }
        }
    }

    /// Check if a method exists on a given type
    ///
    /// Handles user-defined methods in impl blocks and trait methods available
    /// to types that implement the trait (directly or via a generic constraint).
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive lookup across local impls, trait impls, generic param constraints, cached modules, and qualified-name nested modules — splitting reduces locality without simplifying"
    )]
    pub(super) fn method_exists_on_type(
        &self,
        type_name: &str,
        method_name: &str,
        file: &File,
    ) -> bool {
        // Skip validation for unknown types (chained method calls where we can't infer intermediate types)
        if type_name == "Unknown" || type_name.contains("Unknown") {
            return true;
        }
        // Strip optional marker and generic args for lookups
        let base = type_name.trim_end_matches('?');
        let lookup = base.split_once('<').map_or(base, |(n, _)| n);

        // Check if it's a struct with an impl block containing the method
        if self.symbols.is_struct(lookup) {
            // Check impl blocks in the current file
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            // Check trait methods for traits this struct implements
            let traits = self.symbols.get_all_traits_for_struct(lookup);
            for trait_name in traits {
                if let Some(info) = self.symbols.get_trait(&trait_name) {
                    for sig in &info.methods {
                        if sig.name.name == method_name {
                            return true;
                        }
                    }
                }
            }
        }

        // Check enum impl blocks
        if self.symbols.get_enum_variants(lookup).is_some() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If the receiver type is an in-scope generic parameter, look for the
        // method on any of its trait constraints. generic_scopes is only
        // populated during type resolution, so also fall back to scanning the
        // current file's struct/impl definitions for a matching type parameter.
        if let Some(constraints) = self.get_type_parameter_constraints(lookup) {
            for trait_name in constraints {
                if let Some(info) = self.symbols.get_trait(&trait_name) {
                    for sig in &info.methods {
                        if sig.name.name == method_name {
                            return true;
                        }
                    }
                }
            }
        }
        if self.type_param_has_method(lookup, method_name, file) {
            return true;
        }

        // Cross-module lookup: the receiver's type may have been imported
        // via `use mod::Type`, in which case the impl lives in the module's
        // cached AST. Scan every cached module for a matching impl.
        for (cached_file, _) in self.module_cache.values() {
            for statement in &cached_file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Impl(impl_def) = &**def {
                        if impl_def.name.name == lookup {
                            for func in &impl_def.functions {
                                if func.name.name == method_name {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // qualified-type lookup — when the receiver type is
        // `m::Foo`, walk into the nested module path (inline modules in
        // the current file, then cached imported modules) and check for
        // an impl of `Foo` with the requested method. The bare-name
        // checks above don't handle qualified receivers, so prior to
        // this fix `f.method()` on an imported-module type silently
        // returned "not defined".
        if let Some((module_segments, bare_name)) = split_qualified_type(lookup) {
            // Inline modules in the current file.
            if let Some(defs) = find_nested_module_definitions(&file.statements, &module_segments) {
                if impl_method_in_definitions(defs, bare_name, method_name) {
                    return true;
                }
            }
            // Imported modules in the cache.
            for (cached_file, _) in self.module_cache.values() {
                if let Some(defs) =
                    find_nested_module_definitions(&cached_file.statements, &module_segments)
                {
                    if impl_method_in_definitions(defs, bare_name, method_name) {
                        return true;
                    }
                }
                // Also check the cached file's top-level when only the
                // last segment is the module name (e.g. `use mod::*`
                // re-exports flatten differently; staying defensive).
                if module_segments.len() == 1 {
                    for statement in &cached_file.statements {
                        if let Statement::Definition(def) = statement {
                            if let Definition::Impl(impl_def) = &**def {
                                if impl_def.name.name == bare_name {
                                    for func in &impl_def.functions {
                                        if func.name.name == method_name {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check whether `name` is a generic type parameter on some struct/impl/enum
    /// in the file, and if so, whether any of its trait constraints provide
    /// `method_name`.
    fn type_param_has_method(&self, name: &str, method_name: &str, file: &File) -> bool {
        use crate::ast::GenericConstraint;
        let check_generics = |generics: &[crate::ast::GenericParam]| -> bool {
            for gp in generics {
                if gp.name.name != name {
                    continue;
                }
                for constraint in &gp.constraints {
                    let GenericConstraint::Trait {
                        name: trait_ref, ..
                    } = constraint;
                    if let Some(info) = self.symbols.get_trait(&trait_ref.name) {
                        for sig in &info.methods {
                            if sig.name.name == method_name {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        };
        for stmt in &file.statements {
            if let Statement::Definition(def) = stmt {
                match &**def {
                    Definition::Struct(s) if check_generics(&s.generics) => return true,
                    Definition::Impl(i) if check_generics(&i.generics) => return true,
                    Definition::Enum(e) if check_generics(&e.generics) => return true,
                    Definition::Trait(t) if check_generics(&t.generics) => return true,
                    Definition::Struct(_)
                    | Definition::Impl(_)
                    | Definition::Enum(_)
                    | Definition::Trait(_)
                    | Definition::Module(_)
                    | Definition::Function(_) => {}
                }
            }
        }
        false
    }
}
