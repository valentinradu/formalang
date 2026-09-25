use crate::ir::IrModule;

use super::expr::remap_expr;
use super::ty::remap_type;
use super::IdRemap;
use crate::ir::dce::filtering::{retain_trait_id, retain_trait_ref};

pub(super) fn remap_module(module: &mut IrModule, remap: &IdRemap) {
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
            crate::ir::ImplTarget::Primitive(_) => {}
        }
        // The analysis keeps the trait of each impl that survives, so
        // the trait always has a new id here.
        if let Some(trait_ref) = &mut i.trait_ref {
            retain_trait_ref(trait_ref, remap);
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
