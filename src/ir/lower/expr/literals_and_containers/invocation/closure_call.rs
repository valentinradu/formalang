use crate::ast::Expr;
use crate::ir::lower::expr::operators::ArgSlot;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    /// Detect when an `Invocation` whose path is a single segment
    /// resolves to a closure-typed binding rather than a top-level
    /// function, and lower it as [`IrExpr::CallClosure`] targeting that
    /// binding. The binding is a local, or a module-level `let` that no
    /// local shadows.
    ///
    /// Returns `None` when the path doesn't refer to a closure-typed
    /// binding; the caller falls through to the regular
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
        let local_ty = match self.lookup_local_binding(name) {
            Some(ty) => ty.clone(),
            None => self.module_let_type(name)?,
        };
        let ResolvedType::Closure {
            param_tys,
            return_ty,
        } = &local_ty
        else {
            return None;
        };
        let return_ty = (**return_ty).clone();
        let expected_param_tys: Vec<ArgSlot> = param_tys
            .iter()
            .enumerate()
            .map(|(i, (_, ty))| ArgSlot {
                name: format!("__closure_arg_{i}"),
                label: None,
                ty: Some(ty.clone()),
            })
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

    /// Lower a call of the value of an expression, `make()(4)`, to
    /// [`IrExpr::CallClosure`] on that value.
    pub(in crate::ir::lower::expr) fn lower_value_call(
        &mut self,
        callee: &Expr,
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let closure = self.lower_expr(callee);
        let (param_tys, return_ty) = if let ResolvedType::Closure {
            param_tys,
            return_ty,
        } = closure.ty()
        {
            (param_tys.clone(), (**return_ty).clone())
        } else {
            let other = closure.ty().clone();
            let detail = format!("a called value lowered to the non-closure type {other:?}");
            (
                Vec::new(),
                self.internal_error_type_if_concrete(&other, detail),
            )
        };
        let lowered_args: Vec<(Option<String>, IrExpr)> = args
            .iter()
            .enumerate()
            .map(|(i, (arg_name, expr))| {
                let expected = param_tys.get(i).map(|(_, ty)| ty.clone());
                let lowered = self.lower_with_expected_value(expr, expected.as_ref());
                (arg_name.as_ref().map(|n| n.name.clone()), lowered)
            })
            .collect();
        IrExpr::CallClosure {
            closure: Box::new(closure),
            args: lowered_args,
            ty: return_ty,
            span: self.current_ir_span(),
        }
    }
}
