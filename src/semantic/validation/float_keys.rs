//! A dictionary key typed `F32` or `F64` that inference gives.
//!
//! `validate_dictionary_key` checks a key type that the source writes.
//! A key type that inference gives enters a program at one of two
//! places, and each is checked there, once:
//!
//! - the keys of a dictionary literal: `[x: 1]` with `x: F64`;
//! - a type argument of a call that stands as a key in the callee's
//!   signature: `K` in `one<K>(k: K) -> [K: I32]` with `k: 1.5`, and
//!   in `collect<K, V>(key: (T) -> K, value: (T) -> V) -> [K: V]`.
//!
//! A later use of the result is not checked again: its type came from
//! one of these places, or from a written type that has its own error.
//! So one mistake gets one error, and a call to a callee with no type
//! parameter in a key position costs nothing here.

use std::collections::HashMap;

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use super::invocation::overloads::ParamView;
use crate::ast::{Expr, File, GenericParam, Ident, PrimitiveType, Type};
use crate::error::CompilerError;
use crate::location::Span;
use crate::semantic::inference::{function_view, substitute_all};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Report a dictionary literal whose keys are `F32` or `F64`.
    pub(in crate::semantic::validation) fn check_literal_float_key(
        &mut self,
        entries: &[(Expr, Expr)],
        span: Span,
        file: &File,
    ) {
        // The keys agree, or the homogeneity check reports them.
        let Some((key, _)) = entries.first() else {
            return;
        };
        let key_ty = self.infer_type_sem(key, file);
        self.report_float_key(&key_ty, span);
    }

    /// Report a call of a generic function that puts `F32` or `F64` in
    /// a key position of the function's signature.
    pub(in crate::semantic::validation) fn check_function_float_key(
        &mut self,
        path: &[Ident],
        type_args: &[Type],
        args: &[(Option<Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let found = {
            let overloads = self.function_overloads(&name);
            let Some(info) = overloads.first() else {
                return;
            };
            let generics: Vec<&str> = info.generics.iter().map(|g| g.name.name.as_str()).collect();
            let keys = key_names(
                &generics,
                info.params
                    .iter()
                    .filter_map(|p| p.ty.as_ref())
                    .chain(info.return_type.as_ref()),
            );
            if keys.is_empty() {
                return;
            }
            let (view, fresh) = function_view(&info.generics, type_args);
            let views: Vec<_> = info.params.iter().map(ParamView::of_param_info).collect();
            let outer = |ty| substitute_all(&ty, &view);
            let bindings = self.bind_call_generics(&fresh, &views, &outer, args, file);
            first_float_key(&keys, &view, &bindings)
        };
        if let Some(ty) = found {
            self.report_float_key(&ty, span);
        }
    }

    /// Report a call of a method that puts `F32` or `F64` in a key
    /// position of the method's signature: through a type parameter of
    /// the method, or through one of the receiver's type.
    pub(in crate::semantic::validation) fn check_method_float_key(
        &mut self,
        receiver_sem: &SemType,
        (fn_def, impl_generics): (&crate::ast::FnDef, &[GenericParam]),
        args: &[(Option<Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let receiver_args = Self::receiver_type_arguments(receiver_sem);
        let (view, fresh) = self.method_view(
            &Self::method_receiver_name(receiver_sem),
            impl_generics,
            &receiver_args,
            &fn_def.generics,
        );
        let candidates: Vec<&str> = view.keys().map(String::as_str).collect();
        let keys = key_names(
            &candidates,
            fn_def
                .params
                .iter()
                .filter_map(|p| p.ty.as_ref())
                .chain(fn_def.return_type.as_ref()),
        );
        if keys.is_empty() {
            return;
        }
        let views: Vec<_> = fn_def.params.iter().map(ParamView::of_fn_param).collect();
        let outer = |ty| substitute_all(&ty, &view);
        let bindings = self.bind_call_generics(&fresh, &views, &outer, args, file);
        if let Some(ty) = first_float_key(&keys, &view, &bindings) {
            self.report_float_key(&ty, span);
        }
    }

    /// Push `FloatDictionaryKey` when `key_ty` is `F32` or `F64`.
    fn report_float_key(&mut self, key_ty: &SemType, span: Span) {
        let key_type = match key_ty {
            SemType::Primitive(PrimitiveType::F32) => "F32",
            SemType::Primitive(PrimitiveType::F64) => "F64",
            SemType::Primitive(_)
            | SemType::Named(_)
            | SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Generic { .. }
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Unknown
            | SemType::InferredEnum
            | SemType::Nil => return,
        };
        self.errors.push(CompilerError::FloatDictionaryKey {
            key_type: key_type.to_string(),
            span,
        });
    }
}

/// The names of `candidates` that stand as the key of a dictionary type
/// in `types`: `K` in `[K: V]`.
fn key_names<'t>(candidates: &[&str], types: impl Iterator<Item = &'t Type>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ty in types {
        super::type_names::for_each_key_name(ty, &mut |name| {
            if candidates.contains(&name) && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        });
    }
    out
}

/// The type of the first name in `keys` that the call makes `F32` or
/// `F64`.
fn first_float_key(
    keys: &[String],
    view: &HashMap<String, SemType>,
    bindings: &HashMap<String, SemType>,
) -> Option<SemType> {
    keys.iter()
        .filter_map(|name| view.get(name))
        .map(|ty| substitute_all(ty, bindings))
        .find(|ty| {
            matches!(
                ty,
                SemType::Primitive(PrimitiveType::F32 | PrimitiveType::F64)
            )
        })
}
