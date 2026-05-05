use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use super::super::qualified_types::{
    find_nested_module_definitions, impl_method_in_definitions, split_qualified_type,
};
use crate::ast::{Definition, File, Statement};
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check if a method exists on a given type
    ///
    /// Handles user-defined methods in impl blocks and trait methods available
    /// to types that implement the trait (directly or via a generic constraint).
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive lookup across local impls, trait impls, generic param constraints, cached modules, and qualified-name nested modules; splitting reduces locality without simplifying"
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

        // qualified-type lookup: when the receiver type is
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
    pub(super) fn struct_field_is_closure(
        &self,
        type_name: &str,
        field_name: &str,
        file: &File,
    ) -> bool {
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
