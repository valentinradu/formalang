use crate::ast::Expr;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    /// Detect when an `Invocation` whose path is a single segment
    /// resolves to a closure-typed local binding rather than a top-
    /// level function, and lower it as [`IrExpr::CallClosure`]
    /// targeting that binding.
    ///
    /// Returns `None` when the path doesn't refer to a closure-typed
    /// local; the caller falls through to the regular
    /// [`IrExpr::FunctionCall`] path.
    pub(super) fn try_lower_closure_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> Option<IrExpr> {
        let [ident] = path else {
            return None;
        };
        let name = &ident.name;
        let local_ty = self.lookup_local_binding(name)?.clone();
        let ResolvedType::Closure {
            param_tys,
            return_ty,
        } = &local_ty
        else {
            return None;
        };
        let return_ty = (**return_ty).clone();
        let expected_param_tys: Vec<(String, ResolvedType)> = param_tys
            .iter()
            .enumerate()
            .map(|(i, (_, ty))| (format!("__closure_arg_{i}"), ty.clone()))
            .collect();
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .enumerate()
            .map(|(i, (arg_name, expr))| {
                let expected = Self::expected_arg_ty(&expected_param_tys, i, arg_name.as_ref());
                let lowered = self.lower_with_expected_value(expr, expected.as_ref());
                (arg_name.as_ref().map(|n| n.name.clone()), lowered)
            })
            .collect();
        Some(IrExpr::CallClosure {
            closure: Box::new(IrExpr::LetRef {
                name: name.clone(),
                binding_id: crate::ir::BindingId(0),
                ty: local_ty,
                span: self.current_ir_span(),
            }),
            args: lowered_args,
            ty: return_ty,
            span: self.current_ir_span(),
        })
    }
}
