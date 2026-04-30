//! ID remapping after dead-code elimination.
//!
//! After unused [`StructId`] / [`TraitId`] / [`EnumId`] definitions are
//! removed from an [`IrModule`], every surviving reference must be rewritten
//! to the new contiguous id space. The functions in this module build the
//! [`IdRemap`] table and walk the module rewriting expression, statement,
//! type, and impl-target ids.

use std::collections::HashSet;

use crate::ir::{EnumId, IrExpr, IrModule, StructId, TraitId};

use super::filtering::{retain_trait_id, retain_trait_ref};

/// Mapping from old-to-new IDs after a DCE pass. `None` at an index means
/// the old definition was removed.
#[derive(Debug, Default)]
pub(super) struct IdRemap {
    pub(super) structs: Vec<Option<StructId>>,
    pub(super) traits: Vec<Option<TraitId>>,
    pub(super) enums: Vec<Option<EnumId>>,
}

impl IdRemap {
    pub(super) fn struct_of(&self, old: StructId) -> Option<StructId> {
        self.structs.get(old.0 as usize).copied().flatten()
    }

    pub(super) fn trait_of(&self, old: TraitId) -> Option<TraitId> {
        self.traits.get(old.0 as usize).copied().flatten()
    }

    pub(super) fn enum_of(&self, old: EnumId) -> Option<EnumId> {
        self.enums.get(old.0 as usize).copied().flatten()
    }
}

/// Remove unused struct/trait/enum definitions and every reference to them
/// across the whole IR module. Also drops impl blocks whose target is
/// removed, and rebuilds name-to-ID indices.
pub(super) fn remove_unused_definitions(
    module: &mut IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) {
    let remap = build_remap(module, used_structs, used_traits, used_enums);

    // Filter definition vectors in-place, preserving the relative order of
    // survivors so later-added IDs remain higher than earlier ones. Walk the
    // remap Option slice in lockstep with the definition vector.
    {
        let mut iter = remap.structs.iter();
        module
            .structs
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.traits.iter();
        module
            .traits
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.enums.iter();
        module
            .enums
            .retain(|_| iter.next().copied().flatten().is_some());
    }

    // Drop impls that target a removed struct or enum.
    module.impls.retain(|impl_block| {
        use crate::ir::ImplTarget;
        match impl_block.target {
            ImplTarget::Struct(id) => remap.struct_of(id).is_some(),
            ImplTarget::Enum(id) => remap.enum_of(id).is_some(),
        }
    });

    // Rewrite every remaining ID.
    remap_module(module, &remap);

    module.rebuild_indices();
}

fn build_remap(
    module: &IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) -> IdRemap {
    // Since every old id is itself < u32::MAX (add_* enforces this),
    // truncation here is safe. try_from flagged by strict lints; use it.
    fn remap_slice<Id: Copy + Eq + std::hash::Hash>(
        count: usize,
        used: &HashSet<Id>,
        make: impl Fn(u32) -> Id,
    ) -> Vec<Option<Id>> {
        let mut out = Vec::with_capacity(count);
        let mut next: u32 = 0;
        for i in 0..count {
            let Ok(old_idx) = u32::try_from(i) else {
                out.push(None);
                continue;
            };
            let old = make(old_idx);
            if used.contains(&old) {
                out.push(Some(make(next)));
                // If we've exhausted the u32 id space, drop remaining
                // items rather than wrap and alias ids.
                let Some(n) = next.checked_add(1) else {
                    for _ in i.saturating_add(1)..count {
                        out.push(None);
                    }
                    break;
                };
                next = n;
            } else {
                out.push(None);
            }
        }
        out
    }

    IdRemap {
        structs: remap_slice(module.structs.len(), used_structs, StructId),
        traits: remap_slice(module.traits.len(), used_traits, TraitId),
        enums: remap_slice(module.enums.len(), used_enums, EnumId),
    }
}

pub(super) fn remap_type(ty: &mut crate::ir::ResolvedType, remap: &IdRemap) {
    use crate::ir::ResolvedType;
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(new) = remap.struct_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Trait(id) => {
            if let Some(new) = remap.trait_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(new) = remap.enum_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Generic { base, args } => {
            match base {
                crate::ir::GenericBase::Struct(id) => {
                    if let Some(new) = remap.struct_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Enum(id) => {
                    if let Some(new) = remap.enum_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Trait(id) => {
                    if let Some(new) = remap.trait_of(*id) {
                        *id = new;
                    }
                }
            }
            for a in args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            remap_type(inner, remap);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                remap_type(t, remap);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            remap_type(key_ty, remap);
            remap_type(value_ty, remap);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                remap_type(t, remap);
            }
            remap_type(return_ty, remap);
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over every IrExpr variant; splitting would hide the structural walk"
)]
fn remap_expr(expr: &mut IrExpr, remap: &IdRemap) {
    remap_type(expr.ty_mut(), remap);
    match expr {
        IrExpr::StructInst {
            struct_id,
            type_args,
            fields,
            ..
        } => {
            if let Some(id) = struct_id {
                if let Some(new) = remap.struct_of(*id) {
                    *id = new;
                }
            }
            for t in type_args {
                remap_type(t, remap);
            }
            for (_, _, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::EnumInst {
            enum_id, fields, ..
        } => {
            if let Some(id) = enum_id {
                if let Some(new) = remap.enum_of(*id) {
                    *id = new;
                }
            }
            for (_, _, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            remap_expr(left, remap);
            remap_expr(right, remap);
        }
        IrExpr::UnaryOp { operand, .. } => remap_expr(operand, remap),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            remap_expr(condition, remap);
            remap_expr(then_branch, remap);
            if let Some(eb) = else_branch {
                remap_expr(eb, remap);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                remap_expr(e, remap);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::FieldAccess { object, .. } => remap_expr(object, remap),
        IrExpr::For {
            var_ty,
            collection,
            body,
            ..
        } => {
            remap_type(var_ty, remap);
            remap_expr(collection, remap);
            remap_expr(body, remap);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            remap_expr(scrutinee, remap);
            for arm in arms {
                for (_, _, t) in &mut arm.bindings {
                    remap_type(t, remap);
                }
                remap_expr(&mut arm.body, remap);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                remap_expr(e, remap);
            }
        }
        IrExpr::CallClosure { closure, args, .. } => {
            remap_expr(closure, remap);
            for (_, e) in args {
                remap_expr(e, remap);
            }
        }
        IrExpr::MethodCall {
            receiver,
            args,
            dispatch,
            ..
        } => {
            remap_expr(receiver, remap);
            for (_, e) in args {
                remap_expr(e, remap);
            }
            if let crate::ir::DispatchKind::Virtual { trait_id, .. } = dispatch {
                if let Some(new) = remap.trait_of(*trait_id) {
                    *trait_id = new;
                }
            }
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, _, _, t) in params {
                remap_type(t, remap);
            }
            for (_, _, _, t) in captures {
                remap_type(t, remap);
            }
            remap_expr(body, remap);
        }
        IrExpr::ClosureRef { env_struct, ty, .. } => {
            remap_type(ty, remap);
            remap_expr(env_struct, remap);
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                remap_expr(k, remap);
                remap_expr(v, remap);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            remap_expr(dict, remap);
            remap_expr(key, remap);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements.iter_mut() {
                remap_block_statement(stmt, remap);
            }
            remap_expr(result, remap);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

fn remap_block_statement(stmt: &mut crate::ir::IrBlockStatement, remap: &IdRemap) {
    use crate::ir::IrBlockStatement;
    match stmt {
        IrBlockStatement::Let { ty, value, .. } => {
            if let Some(t) = ty {
                remap_type(t, remap);
            }
            remap_expr(value, remap);
        }
        IrBlockStatement::Assign { target, value } => {
            remap_expr(target, remap);
            remap_expr(value, remap);
        }
        IrBlockStatement::Expr(e) => remap_expr(e, remap),
    }
}

fn remap_module(module: &mut IrModule, remap: &IdRemap) {
    for s in &mut module.structs {
        s.traits.retain_mut(|tr| retain_trait_ref(tr, remap));
        for f in &mut s.fields {
            remap_type(&mut f.ty, remap);
            if let Some(default) = &mut f.default {
                remap_expr(default, remap);
            }
        }
        for gp in &mut s.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for t in &mut module.traits {
        t.composed_traits
            .retain_mut(|id| retain_trait_id(id, remap));
        for f in &mut t.fields {
            remap_type(&mut f.ty, remap);
        }
        for m in &mut t.methods {
            for p in &mut m.params {
                if let Some(ty) = &mut p.ty {
                    remap_type(ty, remap);
                }
            }
            if let Some(ret) = &mut m.return_type {
                remap_type(ret, remap);
            }
        }
        for gp in &mut t.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for e in &mut module.enums {
        for v in &mut e.variants {
            for f in &mut v.fields {
                remap_type(&mut f.ty, remap);
            }
        }
        for gp in &mut e.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for i in &mut module.impls {
        match &mut i.target {
            crate::ir::ImplTarget::Struct(id) => {
                if let Some(new) = remap.struct_of(*id) {
                    *id = new;
                }
            }
            crate::ir::ImplTarget::Enum(id) => {
                if let Some(new) = remap.enum_of(*id) {
                    *id = new;
                }
            }
        }
        for f in &mut i.functions {
            remap_function(f, remap);
        }
    }
    for f in &mut module.functions {
        remap_function(f, remap);
    }
    for l in &mut module.lets {
        remap_type(&mut l.ty, remap);
        remap_expr(&mut l.value, remap);
    }
}

fn remap_function(f: &mut crate::ir::IrFunction, remap: &IdRemap) {
    for p in &mut f.params {
        if let Some(ty) = &mut p.ty {
            remap_type(ty, remap);
        }
        if let Some(default) = &mut p.default {
            remap_expr(default, remap);
        }
    }
    if let Some(ret) = &mut f.return_type {
        remap_type(ret, remap);
    }
    if let Some(body) = &mut f.body {
        remap_expr(body, remap);
    }
}
