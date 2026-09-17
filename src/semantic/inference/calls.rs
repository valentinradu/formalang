use std::collections::HashMap;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};

/// Apply a `param -> concrete` substitution map structurally to a
/// `SemType`, but only for names that appear in `param_names` (the
/// active generic-parameter set). Used by `specialise_generic_return`
/// so `fn f<T>(xs: [T]) -> T?` returns the substituted `I32?` instead
/// of `T?`.
fn substitute_named_in_sem(
    ty: &SemType,
    bindings: &HashMap<String, SemType>,
    param_names: &std::collections::HashSet<String>,
) -> SemType {
    let mut out = ty.clone();
    for (name, concrete) in bindings {
        if !param_names.contains(name) {
            continue;
        }
        out = out.substitute_named(name, concrete);
    }
    out
}

/// Walk `pattern` (a struct field's declared `SemType` mentioning
/// `Named(T)` placeholders for type parameters) alongside `concrete`
/// (the inferred argument type) and record each `T -> concrete` binding
/// into `out`. Used by `infer_struct_type_args` to recover concrete
/// type arguments from a call like `Box(value: 7)`.
///
/// `Named(name)` is the form `SemType::from_ast` emits for a bare AST
/// `Type::Ident(name)` when the name is not a primitive — so for the
/// purposes of type-arg inference we treat any `Named` whose name is
/// among the struct's generic parameters as a placeholder. Callers
/// filter the resulting bindings to the parameter set.
fn unify_sem(pattern: &SemType, concrete: &SemType, out: &mut HashMap<String, SemType>) {
    match (pattern, concrete) {
        (SemType::Named(name), other) => {
            out.entry(name.clone()).or_insert_with(|| other.clone());
        }
        (SemType::Array(p_inner), SemType::Array(c_inner))
        | (SemType::Optional(p_inner), SemType::Optional(c_inner)) => {
            unify_sem(p_inner, c_inner, out);
        }
        (SemType::Tuple(p_fields), SemType::Tuple(c_fields)) => {
            for ((_, pt), (_, ct)) in p_fields.iter().zip(c_fields.iter()) {
                unify_sem(pt, ct, out);
            }
        }
        (
            SemType::Dictionary {
                key: p_key,
                value: p_val,
            },
            SemType::Dictionary {
                key: c_key,
                value: c_val,
            },
        ) => {
            unify_sem(p_key, c_key, out);
            unify_sem(p_val, c_val, out);
        }
        (
            SemType::Generic {
                base: p_base,
                args: p_args,
            },
            SemType::Generic {
                base: c_base,
                args: c_args,
            },
        ) if p_base == c_base => {
            for (pa, ca) in p_args.iter().zip(c_args.iter()) {
                unify_sem(pa, ca, out);
            }
        }
        (
            SemType::Closure {
                params: p_params,
                return_ty: p_ret,
            },
            SemType::Closure {
                params: c_params,
                return_ty: c_ret,
            },
        ) => {
            for (pt, ct) in p_params.iter().zip(c_params.iter()) {
                unify_sem(pt, ct, out);
            }
            unify_sem(p_ret, c_ret, out);
        }
        _ => {}
    }
}

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
            let resolved_ty =
                scope_lookup.or_else(|| self.local_let_bindings.get(&name).map(|(t, _)| t.clone()));
            if let Some(SemType::Closure { return_ty, .. }) = resolved_ty {
                return *return_ty;
            }
        }

        if self.symbols.is_struct_qualified(&name) {
            // Struct instantiation — return the struct type, with generic args
            // if present. When `type_args` is empty for a generic struct, try
            // inferring them from the call's named-argument expressions so
            // `Box(value: 7)` reaches downstream as `Box<I32>` and field
            // accesses substitute correctly. The lookup is qualified so
            // `geometry::Point(x: 1, y: 2)` reaches the inline-module
            // struct.
            if type_args.is_empty() {
                if let Some(inferred) = self.infer_struct_type_args(&name, args, file) {
                    SemType::Generic {
                        base: name,
                        args: inferred,
                    }
                } else {
                    SemType::Named(name)
                }
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
            // Other shapes (`[T]`, `(T) -> U`, etc.) fall through and
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
            if let Some(ret) = self.lookup_qualified_function_return(path, args, file) {
                return ret;
            }
            SemType::Unknown
        } else {
            SemType::Unknown
        }
    }

    /// Infer the type arguments for a struct constructor invoked
    /// without explicit `<...>`. For each generic parameter on the
    /// struct, looks for any field whose declared AST type unifies
    /// against the corresponding lowered argument's inferred [`SemType`]
    /// and records the binding. Returns the inferred argument list
    /// when every parameter is bound, `None` otherwise.
    pub(super) fn infer_struct_type_args(
        &self,
        struct_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> Option<Vec<SemType>> {
        let info = self.symbols.get_struct_qualified(struct_name)?;
        if info.generics.is_empty() {
            return None;
        }
        let param_names: Vec<String> = info.generics.iter().map(|p| p.name.name.clone()).collect();
        let fields: Vec<(String, crate::ast::Type)> = info
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty.clone()))
            .collect();
        let mut bindings: std::collections::HashMap<String, SemType> =
            std::collections::HashMap::new();
        for (name_opt, expr) in args {
            let Some(arg_name) = name_opt.as_ref() else {
                continue;
            };
            let Some((_, declared_ast)) = fields.iter().find(|(n, _)| n == &arg_name.name) else {
                continue;
            };
            let declared_sem = SemType::from_ast(declared_ast);
            let arg_sem = self.infer_type_sem(expr, file);
            unify_sem(&declared_sem, &arg_sem, &mut bindings);
        }
        let mut out = Vec::with_capacity(param_names.len());
        for name in &param_names {
            out.push(bindings.remove(name)?);
        }
        Some(out)
    }

    /// Infer the type arguments of a generic enum from the values in
    /// a variant's payload.
    ///
    /// The twin of [`Self::infer_struct_type_args`]. Without it a
    /// construction of `enum Maybe<T> { some(v: T), none }` reached
    /// the type checker as the bare name `Maybe`, and a bare name fits
    /// any instantiation: `let m: Maybe<I32> = Maybe.some(v: "text")`
    /// compiled, and the IR carried a `String` in a slot every reader
    /// of `Maybe<I32>` treats as an `I32`.
    ///
    /// Returns `None` unless every parameter reached a type, which is
    /// what `Maybe.none` does — a payload-free variant names no
    /// parameter, so the enum stays the bare name and the surrounding
    /// annotation decides.
    pub(super) fn infer_enum_type_args(
        &self,
        enum_name: &str,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
        file: &File,
    ) -> Option<Vec<SemType>> {
        let info = self.symbols.get_enum_qualified(enum_name)?;
        if info.generics.is_empty() {
            return None;
        }
        let param_names: Vec<String> = info.generics.iter().map(|p| p.name.name.clone()).collect();
        let fields: Vec<(String, crate::ast::Type)> = info
            .variant_fields
            .get(variant)?
            .iter()
            .map(|f| (f.name.clone(), f.ty.clone()))
            .collect();

        let mut bindings: std::collections::HashMap<String, SemType> =
            std::collections::HashMap::new();
        for (index, (arg_name, expr)) in data.iter().enumerate() {
            // A payload is written either by name or in order.
            let declared_ast = fields
                .iter()
                .find(|(n, _)| n == &arg_name.name)
                .or_else(|| fields.get(index))
                .map(|(_, ty)| ty)?;
            let declared_sem = SemType::from_ast(declared_ast);
            let arg_sem = self.infer_type_sem(expr, file);
            unify_sem(&declared_sem, &arg_sem, &mut bindings);
        }

        let mut out = Vec::with_capacity(param_names.len());
        for name in &param_names {
            out.push(bindings.remove(name)?);
        }
        Some(out)
    }

    /// Public wrapper used by the constructor validator to suppress the
    /// `MissingGenericArguments` diagnostic when type-arg inference can
    /// fully cover the struct's generic parameters from the call's
    /// named arguments.
    pub(in crate::semantic) fn can_infer_struct_type_args(
        &self,
        struct_name: &str,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> bool {
        self.infer_struct_type_args(struct_name, args, file)
            .is_some()
    }

    /// Specialise a generic function's return type by unifying every
    /// declared parameter type against the matching argument's inferred
    /// type, then substituting each generic parameter binding into the
    /// raw return.
    ///
    /// Handles compound returns: `-> T`, `-> T?`, `-> [T]`,
    /// `-> [K: V]`, `-> Option<T>`, `-> (a: T, b: U)`, `-> T -> U`, …
    /// — every shape `unify_sem` walks. Used by the call-site inference
    /// path so a `fn first<T>(xs: [T]) -> T?` call returns the
    /// instantiated `I32?` instead of leaking `T?` to the type checker.
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
        let param_names: std::collections::HashSet<String> = func_info
            .generics
            .iter()
            .map(|g| g.name.name.clone())
            .collect();
        let mut bindings: HashMap<String, SemType> = HashMap::new();
        for (i, param) in func_info.params.iter().enumerate() {
            let Some(declared_ast) = &param.ty else {
                continue;
            };
            let arg_expr = args
                .iter()
                .find_map(|(n, e)| {
                    n.as_ref()
                        .filter(|name| name.name == param.name.name)
                        .map(|_| e)
                })
                .or_else(|| args.get(i).map(|(_, e)| e));
            let Some(arg) = arg_expr else { continue };
            let declared_sem = SemType::from_ast(declared_ast);
            let arg_sem = self.infer_type_sem(arg, file);
            unify_sem(&declared_sem, &arg_sem, &mut bindings);
        }
        substitute_named_in_sem(&raw_ret, &bindings, &param_names)
    }

    /// Resolve a qualified function path (`a::b::compute`) by walking
    /// `self.symbols.modules` segment by segment, then through the
    /// imported-module cache. Returns the function's declared return
    /// type as a string when found.
    fn lookup_qualified_function_return(
        &self,
        path: &[crate::ast::Ident],
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> Option<SemType> {
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
                let raw = f
                    .return_type
                    .as_ref()
                    .map_or(SemType::Nil, SemType::from_ast);
                // A function declared inside a module specialises its
                // return type the same way a top-level one does. Doing
                // it only for the unqualified path meant
                // `m::identity(item: 7)` came back as the bare `T`,
                // and every use of the result was a type mismatch
                // against a parameter the call had already fixed.
                self.specialise_generic_return(f, raw, args, file)
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
