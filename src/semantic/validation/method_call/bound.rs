//! A method call on a value whose type is a bounded type parameter.
//!
//! The bound gives the method: `b.get()` with `b: T` and
//! `T: Container<I32>` calls `get` of `Container`. The call has the same
//! checks as any other method call: the labels, the count and the type
//! of each argument, with the trait's type parameters read as the
//! bound gives them.

use super::super::super::bound_methods::BoundMethod;
use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use super::super::invocation::overloads::ParamView;
use crate::ast::{Expr, File, Ident};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Check a call of the bound method `bound`.
    pub(super) fn validate_bound_method_call(
        &mut self,
        bound: &BoundMethod,
        args: &[(Option<Ident>, Expr)],
        span: Span,
        file: &File,
    ) {
        let method = &bound.sig.name.name;
        let views: Vec<_> = bound
            .sig
            .params
            .iter()
            .map(ParamView::of_fn_param)
            .collect();
        let callee = format!("Method '{method}'");
        if !self.validate_call_shape(&callee, method, &views, args, span) {
            return;
        }
        let non_self: Vec<_> = bound
            .sig
            .params
            .iter()
            .filter(|p| p.name.name != "self")
            .collect();
        for (position, (label, arg)) in args.iter().enumerate() {
            let param = label.as_ref().map_or_else(
                || non_self.get(position).copied(),
                |label| {
                    non_self.iter().copied().find(|p| {
                        p.name.name == label.name
                            || p.external_label
                                .as_ref()
                                .is_some_and(|l| l.name == label.name)
                    })
                },
            );
            let Some(declared_ty) = param.and_then(|p| p.ty.as_ref()) else {
                continue;
            };
            let declared = bound.resolve(declared_ty);
            let inferred = self.infer_type_sem(arg, file);
            if declared.is_indeterminate() || inferred.is_indeterminate() {
                continue;
            }
            if !self.value_satisfies_declared(&declared.display(), &inferred) {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared.display(),
                    found: inferred.display(),
                    span: arg.span(),
                });
            }
        }
    }
}
