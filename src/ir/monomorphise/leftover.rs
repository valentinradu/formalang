//! Phase 4: post-pass sanity check. Walks the IR looking for any
//! `Generic`, `TypeParam`, generic trait, or unresolved `Virtual`
//! dispatch left behind by the rewrite/compaction pipeline. The first
//! finding is surfaced as an `InternalError`; anything later is dropped
//! to keep the diagnostic short.

use crate::ir::{GenericBase, IrExpr, IrModule, ResolvedType};

use super::expr_walk::walk_expr;
use super::rewrite::receiver_to_base;
use super::walkers::walk_module_types;

#[derive(Default)]
pub(super) struct LeftoverScanner {
    first: Option<String>,
}

impl LeftoverScanner {
    fn note(&mut self, detail: String) {
        if self.first.is_none() {
            self.first = Some(detail);
        }
    }

    pub(super) fn first_error(self) -> Option<String> {
        self.first
            .map(|s| format!("monomorphise: leftover after pass — {s}"))
    }

    pub(super) fn scan(&mut self, module: &IrModule) {
        // Phase F: generic traits are now compacted alongside generic
        // structs and enums; a survivor here means the rewrite/remap
        // chain dropped a reference somewhere.
        for t in &module.traits {
            if !t.generic_params.is_empty() {
                self.note(format!(
                    "generic trait `{}` survived compaction (rewrite_trait_refs missed a reference)",
                    t.name
                ));
            }
        }

        let mut check = |ty: &ResolvedType| {
            if let Some(sample) = first_leftover(ty) {
                self.note(sample);
            }
        };
        walk_module_types(module, &mut check);

        // Tier-1 item E2: any `DispatchKind::Virtual` whose receiver
        // type is concrete (Struct/Enum) means Phase 2d failed to find
        // an impl that should exist. Surface the gap rather than
        // silently leaving the call unresolved. Calls on TypeParam
        // receivers (uninstantiated generic bodies) are tolerated.
        scan_dispatch_leftovers(module, self);
    }
}

/// Walk every method-call site in the module; report any `Virtual`
/// dispatch on a concrete (`Struct`/`Enum`) receiver. Used by the
/// monomorphise leftover scanner.
fn scan_dispatch_leftovers(module: &IrModule, scanner: &mut LeftoverScanner) {
    let mut check = |expr: &IrExpr| {
        if let IrExpr::MethodCall {
            receiver,
            method,
            dispatch: crate::ir::DispatchKind::Virtual { trait_id, .. },
            ..
        } = expr
        {
            if let Some(base) = receiver_to_base(receiver.ty()) {
                let kind = match base {
                    GenericBase::Struct(id) => format!("struct id {}", id.0),
                    GenericBase::Enum(id) => format!("enum id {}", id.0),
                    // Trait base shouldn't appear as a method-call
                    // receiver post item E2 — surface it in the
                    // diagnostic anyway so unexpected leftovers are
                    // visible.
                    GenericBase::Trait(id) => format!("trait id {}", id.0),
                };
                scanner.note(format!(
                    "unresolved Virtual dispatch — method `{method}` on concrete receiver \
                     ({kind}) for trait id {} (devirtualisation should have rewritten this)",
                    trait_id.0
                ));
            }
        }
    };
    for f in &module.functions {
        if let Some(body) = &f.body {
            walk_expr(body, &mut check);
        }
    }
    for imp in &module.impls {
        for f in &imp.functions {
            if let Some(body) = &f.body {
                walk_expr(body, &mut check);
            }
        }
    }
    for l in &module.lets {
        walk_expr(&l.value, &mut check);
    }
}

fn first_leftover(ty: &ResolvedType) -> Option<String> {
    // Lowering never emits `TypeParam` as a placeholder, so a survivor here
    // is a real monomorphisation gap — report it.
    match ty {
        ResolvedType::TypeParam(name) => Some(format!("unresolved TypeParam(`{name}`)")),
        ResolvedType::Generic { base, args } => {
            let (kind, id) = match base {
                GenericBase::Struct(s) => ("struct", s.0),
                GenericBase::Enum(e) => ("enum", e.0),
                GenericBase::Trait(t) => ("trait", t.0),
            };
            Some(format!(
                "unresolved Generic(base={kind}_id={id}, {} args)",
                args.len()
            ))
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            first_leftover(inner)
        }
        ResolvedType::Tuple(fields) => fields.iter().find_map(|(_, t)| first_leftover(t)),
        ResolvedType::Dictionary { key_ty, value_ty } => {
            first_leftover(key_ty).or_else(|| first_leftover(value_ty))
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => param_tys
            .iter()
            .find_map(|(_, t)| first_leftover(t))
            .or_else(|| first_leftover(return_ty)),
        ResolvedType::External { type_args, .. } => type_args.iter().find_map(first_leftover),
        // `Error` shouldn't reach monomorphisation under normal compilation
        // (upstream `CompilerError`s would have aborted before passes run);
        // surface it explicitly when an externally-loaded IR contains one.
        ResolvedType::Error => Some("ResolvedType::Error placeholder".to_string()),
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_) => None,
    }
}
