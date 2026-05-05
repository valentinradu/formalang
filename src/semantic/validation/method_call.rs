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
use std::collections::HashSet;

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
        let receiver_sem = self.infer_type_sem(receiver, file);
        // The four built-in compound shapes route to the prelude-defined
        // generic structs/enum so `xs.len()`, `opt.is_some()`, `d.len()`,
        // `r.len()` resolve through the same machinery as user types.
        // See `src/prelude.fv`. Optional is checked first so a bare
        // `opt.is_some()` (without auto-strip) lands on Optional, not on
        // its inner T.
        // Indeterminate receivers (`SemType::Unknown` or types
        // containing `Unknown` anywhere) skip method validation —
        // there's nothing to check until inference resolves them.
        if receiver_sem.is_indeterminate() {
            return;
        }
        let receiver_type = match &receiver_sem {
            crate::semantic::sem_type::SemType::Optional(_) => "Optional".to_string(),
            crate::semantic::sem_type::SemType::Array(_) => "Array".to_string(),
            crate::semantic::sem_type::SemType::Dictionary { .. } => "Dictionary".to_string(),
            crate::semantic::sem_type::SemType::Primitive(_)
            | crate::semantic::sem_type::SemType::Named(_)
            | crate::semantic::sem_type::SemType::Tuple(_)
            | crate::semantic::sem_type::SemType::Generic { .. }
            | crate::semantic::sem_type::SemType::Closure { .. }
            | crate::semantic::sem_type::SemType::Unknown
            | crate::semantic::sem_type::SemType::InferredEnum
            | crate::semantic::sem_type::SemType::Nil => receiver_sem.display(),
        };
        if let Some(fn_def) = Self::find_method_fn_def(&receiver_type, &method.name, file) {
            let params = fn_def.params.clone();
            self.validate_fn_param_conventions_receiver(receiver, &params, span, file);
            self.validate_fn_param_conventions_args(&params, args, span, file);
        } else if self.method_exists_on_type(&receiver_type, &method.name, file) {
            // Method exists in a trait/impl block — convention checks on
            // those signatures still happen via `find_method_fn_def`
            // when the impl is in-file; cross-module impls are accepted
            // without further checks here.
        } else if self.struct_field_is_closure(&receiver_type, &method.name, file) {
            // Calling a closure-typed field of a struct: `f.onPress()`
            // where `onPress: () -> E`. The convention checks for the
            // closure's own params live in the closure-binding maps,
            // populated when the field was registered.
        } else {
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
        // Strip optional marker and generic args for lookups. Callers
        // that have a `SemType` to hand are responsible for not calling
        // this on indeterminate types; the previous `== "Unknown"`
        // string sentinel was retired with the `local_let_bindings`
        // migration.
        let base = type_name.trim_end_matches('?');
        let lookup = base.split_once('<').map_or(base, |(n, _)| n);

        // Trait-typed receiver: a `let s: Shape = ...` then `s.area()` must
        // resolve against the trait's declared method set, including
        // methods inherited from any composed (super-)traits. The
        // IR/backend turns this into virtual dispatch via the trait's
        // vtable, but the semantic check has to acknowledge the method
        // exists first.
        if self.symbols.get_trait(lookup).is_some()
            && self.trait_chain_has_method(lookup, method_name, &mut HashSet::new())
        {
            return true;
        }

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

        // Primitive impl blocks (`extern impl String { fn len(self) -> I32 }`):
        // when the receiver is a primitive type name, scan impl blocks
        // whose target is that primitive name. This mirrors the
        // struct/enum dispatch above but routes through the primitive
        // branch of `ImplTarget` in the IR.
        if crate::semantic::is_primitive_name(lookup) {
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

    /// Whether the named struct has a closure-typed field with the
    /// given name. Lets `f.onPress()` resolve to "invoke the closure
    /// stored in `f.onPress`" when `onPress: () -> E` is a struct field
    /// rather than an impl method. The receiver's full type-string is
    /// stripped of `?` and any generic args before the lookup.
    fn struct_field_is_closure(&self, type_name: &str, field_name: &str, file: &File) -> bool {
        let base = type_name.trim_end_matches('?');
        let lookup = base.split_once('<').map_or(base, |(n, _)| n);
        let mut found = false;
        let scan = |defs: &[crate::ast::Definition]| {
            let mut hit = false;
            for def in defs {
                if let crate::ast::Definition::Struct(s) = def {
                    if s.name.name == lookup {
                        for f in &s.fields {
                            if f.name.name == field_name
                                && matches!(f.ty, crate::ast::Type::Closure { .. })
                            {
                                hit = true;
                            }
                        }
                    }
                }
            }
            hit
        };
        let in_file: Vec<crate::ast::Definition> = file
            .statements
            .iter()
            .filter_map(|s| {
                if let Statement::Definition(d) = s {
                    Some((**d).clone())
                } else {
                    None
                }
            })
            .collect();
        if scan(&in_file) {
            return true;
        }
        for (cached_file, _) in self.module_cache.values() {
            let cached_defs: Vec<crate::ast::Definition> = cached_file
                .statements
                .iter()
                .filter_map(|s| {
                    if let Statement::Definition(d) = s {
                        Some((**d).clone())
                    } else {
                        None
                    }
                })
                .collect();
            if scan(&cached_defs) {
                found = true;
                break;
            }
        }
        found
    }

    /// Walk a trait's own methods plus every composed (super-)trait
    /// looking for `method_name`. The visited-set guards against cyclic
    /// trait composition (which is rejected upstream but the walker
    /// stays defensive). Used by `method_exists_on_type` so a call on
    /// a trait-typed binding finds methods inherited from parents.
    fn trait_chain_has_method(
        &self,
        trait_name: &str,
        method_name: &str,
        visited: &mut HashSet<String>,
    ) -> bool {
        if !visited.insert(trait_name.to_string()) {
            return false;
        }
        let Some(info) = self.symbols.get_trait(trait_name) else {
            return false;
        };
        if info.methods.iter().any(|m| m.name.name == method_name) {
            return true;
        }
        for parent in &info.composed_traits {
            if self.trait_chain_has_method(parent, method_name, visited) {
                return true;
            }
        }
        false
    }
}
