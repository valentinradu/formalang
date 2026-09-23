//! Type arguments of a call: the types that a callee's type parameters
//! take from the call's arguments, and the type of a closure argument
//! with the parameter types that its position gives it.
//!
//! A free function and a method share the rule, so it lives here once.

use std::collections::HashMap;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use super::calls::unify_sem;
use crate::ast::{Expr, File};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// The type of a closure literal.
    ///
    /// A parameter with no type takes the type at its position in
    /// `expected`, the parameter types of the closure that the position
    /// expects. With no `expected`, or a different number of
    /// parameters, such a parameter is `SemType::Unknown`.
    pub(in crate::semantic) fn infer_closure_sem(
        &self,
        params: &[crate::ast::ClosureParam],
        return_type: Option<&crate::ast::Type>,
        body: &Expr,
        expected: Option<&[(crate::ast::ParamConvention, SemType)]>,
        file: &File,
    ) -> SemType {
        let expected = expected.filter(|slots| slots.len() == params.len());
        let param_tys: Vec<(crate::ast::ParamConvention, SemType)> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = p.ty.as_ref().map_or_else(
                    || {
                        expected
                            .and_then(|slots| slots.get(i))
                            .map_or(SemType::Unknown, |(_, ty)| ty.clone())
                    },
                    SemType::from_ast,
                );
                (p.convention, ty)
            })
            .collect();
        // Push closure params into the inference-scope stack so
        // references inside the body resolve to their types instead
        // of "Unknown".
        let frame: HashMap<String, SemType> = params
            .iter()
            .zip(&param_tys)
            .filter(|(_, (_, ty))| !matches!(ty, SemType::Unknown))
            .map(|(p, (_, ty))| (p.name.name.clone(), ty.clone()))
            .collect();
        self.inference_scope_stack.borrow_mut().push(frame);
        let inferred_body_type = self.infer_type_sem(body, file);
        self.inference_scope_stack.borrow_mut().pop();
        // prefer the explicit return type when present;
        // fall back to body inference otherwise.
        let return_ty = return_type.map_or(inferred_body_type, SemType::from_ast);
        SemType::closure(param_tys, return_ty)
    }

    /// Bind the type parameters `generics` of a callee from the
    /// arguments of a call.
    ///
    /// Each argument's type is matched against the declared type of
    /// the parameter it fills, after `outer` rewrites that type (a
    /// method replaces the type parameters of its receiver there). A
    /// closure argument comes last: the parameters with no type take
    /// the declared parameter types, with the bindings found so far in
    /// them. So `f: (T) -> U` on a `Seq<I32>` types `(v) -> v * 2` as
    /// `(I32) -> I32`, and `U` binds to `I32`. Without this the closure
    /// parameter was unknown, `U` bound to an unknown type, and the
    /// caller checked nothing.
    ///
    /// Only a name of `generics` binds. A name of the caller that
    /// `outer` brings into a declared type is a type, not a slot. Pass
    /// fresh names (see [`fresh_type_param`]), so that a name of the
    /// caller can never be one of them.
    pub(in crate::semantic) fn bind_call_generics(
        &self,
        generics: &[String],
        params: &[crate::semantic::validation::invocation::overloads::ParamView<'_>],
        outer: &dyn Fn(SemType) -> SemType,
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> HashMap<String, SemType> {
        let mut bindings: HashMap<String, SemType> = HashMap::new();
        if generics.is_empty() {
            return bindings;
        }
        let bind = |pattern: &SemType, found: &SemType, bindings: &mut HashMap<String, SemType>| {
            let mut all = HashMap::new();
            unify_sem(pattern, found, &mut all);
            for (name, ty) in all {
                // A binding that still holds one of the callee's own
                // names gives no type: a closure parameter typed from
                // the declared `(U) -> I32` binds `U` to `U`.
                if generics.contains(&name) && !mentions_any(&ty, generics) {
                    bindings.entry(name).or_insert(ty);
                }
            }
        };
        let declared = |i: usize, label: Option<&crate::ast::Ident>| {
            outer(Self::expected_argument(params, i, label))
        };
        let is_closure = |e: &Expr| matches!(e, Expr::ClosureExpr { .. });
        for (i, (label, arg)) in args.iter().enumerate() {
            if !is_closure(arg) {
                let arg_sem = self.infer_type_sem(arg, file);
                bind(&declared(i, label.as_ref()), &arg_sem, &mut bindings);
            }
        }
        for (i, (label, arg)) in args.iter().enumerate() {
            let Expr::ClosureExpr {
                params: closure_params,
                return_type,
                body,
                ..
            } = arg
            else {
                continue;
            };
            let expected = substitute_all(&declared(i, label.as_ref()), &bindings);
            let slots = if let SemType::Closure { params: slots, .. } = &expected {
                Some(slots.as_slice())
            } else {
                None
            };
            let arg_sem =
                self.infer_closure_sem(closure_params, return_type.as_ref(), body, slots, file);
            bind(&expected, &arg_sem, &mut bindings);
        }
        bindings
    }

    /// How a method's declared types read at a call: the rewrite, and
    /// the names of the method's own type parameters in it.
    ///
    /// The receiver's type parameters take its type arguments. The
    /// impl names them, and `impl Box { ... }` may leave them out; the
    /// type's own declaration names them then. The method's own type
    /// parameters take fresh names, which no source name can be. A
    /// receiver argument may name a type parameter of the caller, and
    /// the caller's `U` must never meet the method's `U`: in
    /// `fn wrap<U>(self, other: U)`, `Box(value: other).map(...)`
    /// bound `map`'s `U` to the caller's `U`.
    pub(in crate::semantic) fn method_view(
        &self,
        type_name: &str,
        impl_generics: &[crate::ast::GenericParam],
        receiver_args: &[SemType],
        method_generics: &[crate::ast::GenericParam],
    ) -> (HashMap<String, SemType>, Vec<String>) {
        let names: Vec<String> = if impl_generics.is_empty() {
            self.symbols
                .structs
                .get(type_name)
                .map(|s| s.generics.clone())
                .or_else(|| {
                    self.symbols
                        .enums
                        .get(type_name)
                        .map(|e| e.generics.clone())
                })
                .unwrap_or_default()
                .iter()
                .map(|g| g.name.name.clone())
                .collect()
        } else {
            impl_generics.iter().map(|g| g.name.name.clone()).collect()
        };
        let mut view: HashMap<String, SemType> = names
            .into_iter()
            .zip(receiver_args.iter().cloned())
            .collect();
        let (own, fresh) = function_view(method_generics, &[]);
        view.extend(own);
        (view, fresh)
    }

    /// Bind the fresh names of a method's own type parameters from the
    /// arguments of a call. A name that no argument binds is unknown.
    pub(in crate::semantic) fn bind_method_generics(
        &self,
        view: &HashMap<String, SemType>,
        fresh: &[String],
        params: &[crate::semantic::validation::invocation::overloads::ParamView<'_>],
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> HashMap<String, SemType> {
        let outer = |ty: SemType| substitute_all(&ty, view);
        let mut bindings = self.bind_call_generics(fresh, params, &outer, args, file);
        for name in fresh {
            bindings.entry(name.clone()).or_insert(SemType::Unknown);
        }
        bindings
    }
}

/// How a generic function's declared types read at a call: the
/// rewrite, and the fresh names in it that the arguments must bind.
///
/// A type argument written at the call (`first<I32>(...)`) replaces its
/// parameter. Every other parameter takes a fresh name, so a type
/// parameter of the caller with the same name never meets it: in
/// `fn flip<A, B>(p: B, q: A)`, `swap(a: p, b: q)` binds `swap`'s `A`
/// to the caller's `B`.
pub(in crate::semantic) fn function_view(
    generics: &[crate::ast::GenericParam],
    type_args: &[crate::ast::Type],
) -> (HashMap<String, SemType>, Vec<String>) {
    let mut view = HashMap::new();
    let mut fresh = Vec::new();
    for (i, generic) in generics.iter().enumerate() {
        let name = &generic.name.name;
        if let Some(written) = type_args.get(i) {
            view.insert(name.clone(), SemType::from_ast(written));
        } else {
            let renamed = fresh_type_param(name);
            view.insert(name.clone(), SemType::Named(renamed.clone()));
            fresh.push(renamed);
        }
    }
    (view, fresh)
}

/// The fresh name of a callee's type parameter at a call. No name in
/// the source holds a `'`, so no type of the caller can be one.
pub(in crate::semantic) fn fresh_type_param(name: &str) -> String {
    format!("{name}'")
}

/// Whether `ty` holds a fresh name that no argument of the call bound:
/// its type is not known at the call.
pub(in crate::semantic) fn holds_an_unbound_type_param(ty: &SemType) -> bool {
    let found = std::cell::Cell::new(false);
    ty.map_named(&|n| {
        if n.ends_with('\'') {
            found.set(true);
        }
        SemType::Unknown
    });
    found.get()
}

/// Whether `ty` names one of `names`.
fn mentions_any(ty: &SemType, names: &[String]) -> bool {
    let found = std::cell::Cell::new(false);
    ty.map_named(&|n| {
        if names.iter().any(|name| name == n) {
            found.set(true);
        }
        SemType::Unknown
    });
    found.get()
}

/// Replace each name that `map` holds, all in one step. A type that
/// replaces a name is never rewritten again, so `A -> B, B -> A` swaps
/// the two. Replacing one name after the other made both `A`.
pub(in crate::semantic) fn substitute_all(ty: &SemType, map: &HashMap<String, SemType>) -> SemType {
    ty.map_named(&|n| {
        map.get(n)
            .cloned()
            .unwrap_or_else(|| SemType::Named(n.to_string()))
    })
}
