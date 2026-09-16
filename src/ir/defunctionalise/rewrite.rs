//! Replace closure values, indirect calls, and closure types.
//!
//! The module-level walk and the type rewrite live here; the
//! expression walk is in `rewrite_expr.rs`, which keeps each file
//! under the project's 500-line ceiling.

use crate::ir::{IrModule, ResolvedType};

use super::rewrite_expr::expr;
use super::synthesis::Plan;

/// Rewrite every expression and every type slot in the module.
pub(super) fn apply(module: &mut IrModule, plan: &Plan) {
    let lets = std::mem::take(&mut module.lets);
    module.lets = lets
        .into_iter()
        .map(|mut l| {
            l.value = expr(l.value, plan);
            ty(&mut l.ty, plan);
            l
        })
        .collect();

    let functions = std::mem::take(&mut module.functions);
    module.functions = functions.into_iter().map(|f| function(f, plan)).collect();

    let impls = std::mem::take(&mut module.impls);
    module.impls = impls
        .into_iter()
        .map(|mut i| {
            let methods = std::mem::take(&mut i.functions);
            i.functions = methods.into_iter().map(|f| function(f, plan)).collect();
            i
        })
        .collect();

    let structs = std::mem::take(&mut module.structs);
    module.structs = structs
        .into_iter()
        .map(|mut s| {
            let fields = std::mem::take(&mut s.fields);
            s.fields = fields
                .into_iter()
                .map(|mut field| {
                    field.default = field.default.map(|d| expr(d, plan));
                    ty(&mut field.ty, plan);
                    field
                })
                .collect();
            s
        })
        .collect();

    let enums = std::mem::take(&mut module.enums);
    module.enums = enums
        .into_iter()
        .map(|mut e| {
            for variant in &mut e.variants {
                let fields = std::mem::take(&mut variant.fields);
                variant.fields = fields
                    .into_iter()
                    .map(|mut field| {
                        field.default = field.default.map(|d| expr(d, plan));
                        ty(&mut field.ty, plan);
                        field
                    })
                    .collect();
            }
            e
        })
        .collect();
}

fn function(mut f: crate::ir::IrFunction, plan: &Plan) -> crate::ir::IrFunction {
    f.body = f.body.map(|b| expr(b, plan));
    for param in &mut f.params {
        if let Some(param_ty) = &mut param.ty {
            ty(param_ty, plan);
        }
        param.default = param.default.take().map(|d| expr(d, plan));
    }
    if let Some(return_ty) = &mut f.return_type {
        ty(return_ty, plan);
    }
    f
}

/// Replace a closure type with its tag enum, at any depth.
pub(super) fn ty(slot: &mut ResolvedType, plan: &Plan) {
    if let Some(shape) = plan.for_type(slot) {
        *slot = shape.enum_ty.clone();
        return;
    }
    match slot {
        ResolvedType::Generic { args, .. } => {
            for arg in args {
                ty(arg, plan);
            }
        }
        ResolvedType::Tuple(fields) => {
            for (_, field_ty) in fields {
                ty(field_ty, plan);
            }
        }
        ResolvedType::External { type_args, .. } => {
            for arg in type_args {
                ty(arg, plan);
            }
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            // A closure type with no value of it anywhere in the
            // module: nothing was built for it, so leave it be. No
            // value can reach it, so the code using it is dead and
            // `DeadCodeEliminationPass` removes it.
            for (_, param_ty) in param_tys {
                ty(param_ty, plan);
            }
            ty(return_ty, plan);
        }
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_)
        | ResolvedType::Error => {}
    }
}
