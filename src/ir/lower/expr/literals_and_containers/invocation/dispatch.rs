use crate::ast::Expr;
use crate::ir::lower::IrLowerer;
use crate::ir::{IrExpr, ResolvedType};

impl IrLowerer<'_> {
    pub(in crate::ir::lower::expr) fn lower_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
    ) -> IrExpr {
        let name = path
            .iter()
            .map(|id| id.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let type_args_resolved: Vec<ResolvedType> =
            type_args.iter().map(|t| self.lower_type(t)).collect();

        if let Some(id) = self.module.struct_id(&name) {
            self.lower_struct_invocation(id, type_args_resolved, args)
        } else if let Some(external_ty) = self.try_external_type(&name, type_args_resolved.clone())
        {
            self.lower_external_invocation(external_ty, type_args_resolved, args)
        } else if let Some(call) = self.try_lower_closure_invocation(path, args) {
            call
        } else {
            self.lower_function_invocation(path, &type_args_resolved, args)
        }
    }
}
