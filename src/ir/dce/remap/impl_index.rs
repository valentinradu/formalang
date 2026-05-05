use crate::ir::{IrExpr, IrModule};

use super::IdRemap;

pub(super) fn build_impl_index_remap(module: &IrModule, remap: &IdRemap) -> Vec<Option<usize>> {
    use crate::ir::ImplTarget;
    let mut out = Vec::with_capacity(module.impls.len());
    let mut next: usize = 0;
    for imp in &module.impls {
        let keep = match imp.target {
            ImplTarget::Struct(id) => remap.struct_of(id).is_some(),
            ImplTarget::Enum(id) => remap.enum_of(id).is_some(),
            ImplTarget::Primitive(_) => true,
        };
        if keep {
            out.push(Some(next));
            next = next.saturating_add(1);
        } else {
            out.push(None);
        }
    }
    out
}

pub(super) fn apply_impl_index_remap(module: &mut IrModule, remap: &[Option<usize>]) {
    let identity = remap
        .iter()
        .enumerate()
        .all(|(i, slot)| matches!(slot, Some(j) if *j == i));
    if identity {
        return;
    }
    for f in &mut module.functions {
        if let Some(body) = &mut f.body {
            rewrite_static_impl_ids(body, remap);
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            if let Some(body) = &mut f.body {
                rewrite_static_impl_ids(body, remap);
            }
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                rewrite_static_impl_ids(default, remap);
            }
        }
    }
    for l in &mut module.lets {
        rewrite_static_impl_ids(&mut l.value, remap);
    }
}

fn rewrite_static_impl_ids(expr: &mut IrExpr, remap: &[Option<usize>]) {
    use crate::ir::{DispatchKind, ImplId};
    walk_expr_static_impl(expr, remap);
    if let IrExpr::MethodCall {
        dispatch: DispatchKind::Static { impl_id },
        ..
    } = expr
    {
        if let Some(Some(new)) = remap.get(impl_id.0 as usize).copied() {
            *impl_id = ImplId(u32::try_from(new).unwrap_or(u32::MAX));
        }
    }
}

fn walk_expr_static_impl(expr: &mut IrExpr, remap: &[Option<usize>]) {
    use crate::ir::IrBlockStatement;
    match expr {
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            for (_, _, e) in fields {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            rewrite_static_impl_ids(left, remap);
            rewrite_static_impl_ids(right, remap);
        }
        IrExpr::UnaryOp { operand, .. } => rewrite_static_impl_ids(operand, remap),
        IrExpr::FieldAccess { object, .. } => rewrite_static_impl_ids(object, remap),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rewrite_static_impl_ids(condition, remap);
            rewrite_static_impl_ids(then_branch, remap);
            if let Some(e) = else_branch {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::For {
            collection, body, ..
        } => {
            rewrite_static_impl_ids(collection, remap);
            rewrite_static_impl_ids(body, remap);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            rewrite_static_impl_ids(scrutinee, remap);
            for arm in arms {
                rewrite_static_impl_ids(&mut arm.body, remap);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            rewrite_static_impl_ids(closure, remap);
            for (_, e) in args {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::MethodCall { receiver, args, .. } => {
            rewrite_static_impl_ids(receiver, remap);
            for (_, e) in args {
                rewrite_static_impl_ids(e, remap);
            }
        }
        IrExpr::Closure { body, .. } => rewrite_static_impl_ids(body, remap),
        IrExpr::ClosureRef { env_struct, .. } => rewrite_static_impl_ids(env_struct, remap),
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                rewrite_static_impl_ids(k, remap);
                rewrite_static_impl_ids(v, remap);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            rewrite_static_impl_ids(dict, remap);
            rewrite_static_impl_ids(key, remap);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements.iter_mut() {
                match stmt {
                    IrBlockStatement::Let { value, .. } => rewrite_static_impl_ids(value, remap),
                    IrBlockStatement::Assign { target, value, .. } => {
                        rewrite_static_impl_ids(target, remap);
                        rewrite_static_impl_ids(value, remap);
                    }
                    IrBlockStatement::Expr(e) => rewrite_static_impl_ids(e, remap),
                }
            }
            rewrite_static_impl_ids(result, remap);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}
