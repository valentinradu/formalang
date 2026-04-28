use std::collections::HashMap;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(super) fn infer_type_invocation(
        &self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> SemType {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        // Closure-typed binding called as a function (`cb()` where
        // `cb: (...) -> R`). Unpack the closure shape structurally
        // and yield its return type.
        if path.len() == 1 {
            let scope_lookup = {
                let stack = self.inference_scope_stack.borrow();
                stack
                    .iter()
                    .rev()
                    .find_map(|frame| frame.get(&name).cloned())
            };
            let ty_str =
                scope_lookup.or_else(|| self.local_let_bindings.get(&name).map(|(t, _)| t.clone()));
            if let Some(t) = ty_str {
                if let SemType::Closure { return_ty, .. } = SemType::from_legacy_string(&t) {
                    return *return_ty;
                }
            }
        }

        if self.symbols.is_struct(&name) {
            // Struct instantiation — return the struct type, with generic args if present
            if type_args.is_empty() {
                SemType::Named(name)
            } else {
                SemType::Generic {
                    base: name,
                    args: type_args.iter().map(SemType::from_ast).collect(),
                }
            }
        } else if let Some(func_info) = self.symbols.get_function(&name) {
            // User-defined standalone function — return its declared return type
            let raw = func_info
                .return_type
                .as_ref()
                .map_or(SemType::Nil, SemType::from_ast);
            // Tier-1 follow-up to item E2: if the function is generic
            // and the declared return type is itself a generic
            // parameter (`fn id<T>(x: T) -> T`), substitute the
            // inferred concrete type from a matching argument so the
            // call site sees `I32` instead of the placeholder `T`.
            // Other shapes (`[T]`, `T -> U`, etc.) fall through and
            // keep the original generic-param string — extending
            // this to compound shapes lives with the broader generic-
            // function inference work.
            self.specialise_generic_return(func_info, raw, args, file)
        } else if path.len() >= 2 {
            // resolve impl-block static method calls
            // (`Type::method(...)`), enum variant constructors
            // (`Enum::variant(...)`) that weren't rewritten to
            // EnumInstantiation at parse time, and module-qualified
            // function calls (`math::compute(...)`).
            let (Some(first), Some(last)) = (path.first(), path.last()) else {
                return SemType::Unknown;
            };
            let receiver = &first.name;
            let method_name = &last.name;
            if self.symbols.is_struct(receiver) {
                if let Some(ret) = self.infer_method_return_from_impls(receiver, method_name) {
                    return ret;
                }
            }
            if self.symbols.get_enum_variants(receiver).is_some() {
                return SemType::Named(receiver.clone());
            }
            // Module-qualified function: walk through module symbol tables.
            if let Some(ret) = self.lookup_qualified_function_return(path) {
                return ret;
            }
            SemType::Unknown
        } else {
            SemType::Unknown
        }
    }

    /// If the function is generic and its declared return type is a
    /// bare generic-parameter name, substitute the inferred type from
    /// the matching argument. Used by the call-site inference path so
    /// `let n: I32 = identity(1)` doesn't surface `T` to the
    /// type-mismatch checker.
    fn specialise_generic_return(
        &self,
        func_info: &super::super::symbol_table::FunctionInfo,
        raw_ret: SemType,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> SemType {
        if func_info.generics.is_empty() {
            return raw_ret;
        }
        // Only a bare generic parameter (`-> T`) qualifies for the
        // shortcut substitution. Compound shapes (`[T]`, `T -> U`,
        // ...) are handled by the broader generic-function inference
        // path elsewhere.
        let SemType::Named(ref param_name) = raw_ret else {
            return raw_ret;
        };
        if !func_info
            .generics
            .iter()
            .any(|g| g.name.name == *param_name)
        {
            return raw_ret;
        }
        // Find the first parameter whose declared type is exactly
        // this generic param name; the corresponding argument's
        // inferred type is the substitution.
        for (i, param) in func_info.params.iter().enumerate() {
            let Some(declared) = &param.ty else { continue };
            let crate::ast::Type::Ident(ident) = declared else {
                continue;
            };
            if ident.name != *param_name {
                continue;
            }
            let arg_expr = args
                .iter()
                .find_map(|(n, e)| {
                    n.as_ref()
                        .filter(|name| name.name == param.name.name)
                        .map(|_| e)
                })
                .or_else(|| args.get(i).map(|(_, e)| e));
            if let Some(arg) = arg_expr {
                return self.infer_type_sem(arg, file);
            }
        }
        raw_ret
    }

    /// Resolve a qualified function path (`a::b::compute`) by walking
    /// `self.symbols.modules` segment by segment, then through the
    /// imported-module cache. Returns the function's declared return
    /// type as a string when found.
    fn lookup_qualified_function_return(&self, path: &[crate::ast::Ident]) -> Option<SemType> {
        let last = path.last()?;
        let segments: Vec<&str> = path
            .iter()
            .take(path.len().saturating_sub(1))
            .map(|i| i.name.as_str())
            .collect();
        let look = |symbols: &super::super::symbol_table::SymbolTable| -> Option<SemType> {
            let mut current = symbols;
            for part in &segments {
                match current.modules.get(*part) {
                    Some(info) => current = &info.symbols,
                    None => return None,
                }
            }
            current.get_function(&last.name).map(|f| {
                f.return_type
                    .as_ref()
                    .map_or(SemType::Nil, SemType::from_ast)
            })
        };
        if let Some(ty) = look(&self.symbols) {
            return Some(ty);
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(ty) = look(symbols) {
                return Some(ty);
            }
        }
        None
    }

    /// Build a per-arm inference scope from a match pattern's bindings.
    /// `enum_name` is the (optionally optional-stripped) name of the
    /// scrutinee's type. For a `Variant { name, bindings }` pattern with
    /// `n` bindings, looks up the variant's field types on the named
    /// enum and zips them with the binding identifiers. Variants on
    /// imported enums fall back through the module cache. Returns an
    /// empty map for `Wildcard` and for variants that can't be resolved
    /// (the body then falls back to existing inference behaviour).
    pub(super) fn build_match_arm_scope(
        &self,
        enum_name: &str,
        pattern: &crate::ast::Pattern,
    ) -> HashMap<String, String> {
        use crate::ast::Pattern;
        let mut frame = HashMap::new();
        let Pattern::Variant { name, bindings } = pattern else {
            return frame;
        };
        let variant_field_tys = self
            .lookup_enum_variant_field_types(enum_name, &name.name)
            .unwrap_or_default();
        for (i, ident) in bindings.iter().enumerate() {
            if let Some(ty) = variant_field_tys.get(i) {
                frame.insert(ident.name.clone(), ty.clone());
            }
        }
        frame
    }

    /// Look up an enum variant's field types as type-strings, in the
    /// current symbol table first, then through any imported module
    /// cache. Returns `None` if the enum or variant isn't found.
    fn lookup_enum_variant_field_types(
        &self,
        enum_name: &str,
        variant_name: &str,
    ) -> Option<Vec<String>> {
        if let Some(info) = self.symbols.enums.get(enum_name) {
            if let Some(fields) = info.variant_fields.get(variant_name) {
                return Some(fields.iter().map(|f| Self::type_to_string(&f.ty)).collect());
            }
        }
        for (_, symbols) in self.module_cache.values() {
            if let Some(info) = symbols.enums.get(enum_name) {
                if let Some(fields) = info.variant_fields.get(variant_name) {
                    return Some(fields.iter().map(|f| Self::type_to_string(&f.ty)).collect());
                }
            }
        }
        None
    }

    /// Walk `self.symbols` for a method declared on an impl block whose
    /// target is `struct_name` and whose name is `method_name`; return the
    /// method's declared return type as a string if found. Used by
    /// `infer_type_invocation` for impl static calls.
    fn infer_method_return_from_impls(
        &self,
        struct_name: &str,
        method_name: &str,
    ) -> Option<SemType> {
        let trait_names = self.symbols.get_all_traits_for_struct(struct_name);
        for trait_name in trait_names {
            if let Some(trait_info) = self.symbols.get_trait(&trait_name) {
                for m in &trait_info.methods {
                    if m.name.name == method_name {
                        return Some(
                            m.return_type
                                .as_ref()
                                .map_or(SemType::Nil, SemType::from_ast),
                        );
                    }
                }
            }
        }
        None
    }
}
