//! Phase 4: post-pass sanity check. Walks the IR looking for any
//! `Generic`, `TypeParam`, generic trait, or unresolved `Virtual`
//! dispatch left behind by the rewrite/compaction pipeline. The first
//! finding is surfaced as an `InternalError`; anything later is dropped
//! to keep the diagnostic short.

use crate::ir::{GenericBase, IrExpr, IrModule, ResolvedType};

use super::expr_walk::walk_expr;
use super::rewrite::receiver_to_base;
use super::walkers::{walk_expr_types, walk_function_types};

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
        // chain dropped a reference somewhere. Prelude-shipped generic
        // built-ins (`Optional`, `Array`, `Dictionary`, `Range`) are
        // exempt — they're carriers, not specialisable templates.
        for t in &module.traits {
            if !t.generic_params.is_empty() {
                self.note(format!(
                    "generic trait `{}` survived compaction (rewrite_trait_refs missed a reference)",
                    t.name
                ));
            }
        }

        let is_prelude_builtin =
            |name: &str| matches!(name, "Array" | "Dictionary" | "Range" | "Optional");
        let mut check = |ty: &ResolvedType| {
            if let Some(sample) = first_leftover(ty, module) {
                self.note(sample);
            }
        };
        // Walk every type slot in the module *except* the bodies of the
        // prelude built-ins themselves and their extern impl blocks.
        // Their fields, variants, and method signatures legitimately
        // mention `TypeParam(T)` because they're surviving generic
        // carriers; everything else is real code that must be fully
        // specialised.
        for s in &module.structs {
            if is_prelude_builtin(&s.name) {
                continue;
            }
            for field in &s.fields {
                check(&field.ty);
                if let Some(d) = &field.default {
                    walk_expr_types(d, &mut check);
                }
            }
        }
        for e in &module.enums {
            if is_prelude_builtin(&e.name) {
                continue;
            }
            for v in &e.variants {
                for f in &v.fields {
                    check(&f.ty);
                    if let Some(d) = &f.default {
                        walk_expr_types(d, &mut check);
                    }
                }
            }
        }
        for t in &module.traits {
            for f in &t.fields {
                check(&f.ty);
                if let Some(d) = &f.default {
                    walk_expr_types(d, &mut check);
                }
            }
            for sig in &t.methods {
                for p in &sig.params {
                    if let Some(ty) = &p.ty {
                        check(ty);
                    }
                    if let Some(d) = &p.default {
                        walk_expr_types(d, &mut check);
                    }
                }
                if let Some(ty) = &sig.return_type {
                    check(ty);
                }
            }
        }
        for imp in &module.impls {
            let on_builtin = match imp.target {
                crate::ir::ImplTarget::Struct(id) => module
                    .get_struct(id)
                    .is_some_and(|s| is_prelude_builtin(&s.name)),
                crate::ir::ImplTarget::Enum(id) => module
                    .get_enum(id)
                    .is_some_and(|e| is_prelude_builtin(&e.name)),
                crate::ir::ImplTarget::Primitive(_) => false,
            };
            if on_builtin {
                continue;
            }
            for f in &imp.functions {
                walk_function_types(f, &mut check);
            }
        }
        for f in &module.functions {
            walk_function_types(f, &mut check);
        }
        for l in &module.lets {
            check(&l.ty);
            walk_expr_types(&l.value, &mut check);
        }

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

fn first_leftover(ty: &ResolvedType, module: &IrModule) -> Option<String> {
    // Lowering never emits `TypeParam` as a placeholder, so a survivor here
    // is a real monomorphisation gap — report it. Generic instantiations
    // of the prelude-shipped built-in carriers (`Optional`, `Array`,
    // `Dictionary`, `Range`) are the canonical post-pass shape and
    // never get specialised, so they're allowed.
    let prelude_ids = [
        module.prelude_array_id().map(GenericBase::Struct),
        module.prelude_dictionary_id().map(GenericBase::Struct),
        module.prelude_range_id().map(GenericBase::Struct),
        module.prelude_optional_id().map(GenericBase::Enum),
    ];
    let is_prelude_builtin =
        |base: &GenericBase| prelude_ids.iter().any(|p| p.as_ref() == Some(base));
    match ty {
        ResolvedType::TypeParam(name) => Some(format!("unresolved TypeParam(`{name}`)")),
        ResolvedType::Generic { base, args } => {
            if is_prelude_builtin(base) {
                return args.iter().find_map(|a| first_leftover(a, module));
            }
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
        ResolvedType::Tuple(fields) => fields.iter().find_map(|(_, t)| first_leftover(t, module)),
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => param_tys
            .iter()
            .find_map(|(_, t)| first_leftover(t, module))
            .or_else(|| first_leftover(return_ty, module)),
        ResolvedType::External { type_args, .. } => {
            type_args.iter().find_map(|a| first_leftover(a, module))
        }
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
