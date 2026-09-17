//! Phases 1b/1c/1d: copy every imported function, impl block, and `pub`
//! `let` into the entry module under a qualified name. Each clone has
//! its signature/body types externalised so the next pass of Phase 1a
//! re-discovers any newly-referenced `External` types and clones them.

use std::collections::HashMap;

use crate::error::CompilerError;
use crate::ir::{ImplTarget, IrModule};

use super::super::walkers::{walk_expr_types_mut, walk_function_types_mut};
use super::externalise::externalise_imported_refs;
use super::naming::qualified_name;
use super::specialise::specialise_external;

/// Phase 1d: inline every imported pub `let` into the current module
/// under a qualified name. The clone has its `ty` and the `value`
/// expression's embedded `ResolvedTypes` externalised so the next
/// worklist iteration of
/// [`super::specialise::specialise_external_instantiations`] picks up
/// any types they reference.
///
/// `IrLet` carries explicit visibility; non-public lets are skipped
/// (cross-module access to them is rejected at semantic time anyway).
///
/// Body **id-references** in the `value` expression stay in the imported
/// id-space. A follow-up commit walks all cloned bodies and remaps.
pub(in crate::ir::monomorphise) fn inline_imported_lets(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    for (module_path, imported) in super::sorted_imports(imported_modules) {
        for let_binding in &imported.lets {
            if !let_binding.visibility.is_public() {
                continue;
            }
            let qualified = qualified_name(module_path, &let_binding.name);
            if module.has_let(&qualified) {
                continue;
            }

            let mut clone = let_binding.clone();
            clone.name.clone_from(&qualified);
            externalise_imported_refs(&mut clone.ty, imported, module_path);
            walk_expr_types_mut(&mut clone.value, &mut |ty| {
                externalise_imported_refs(ty, imported, module_path);
            });
            module.add_let(clone);
        }
    }
}

/// Phase 1c: inline every imported impl block whose target type is now
/// present in the local module (its struct or enum has already been
/// cloned by Phase 1a).
///
/// The clone has its `target` and `trait_ref.trait_id` translated from
/// the imported module's id-space to the local clone's id (looked up by
/// qualified name). Method signatures and bodies have `ResolvedType`
/// references externalised so subsequent worklist iterations of
/// [`super::specialise::specialise_external_instantiations`] pick up
/// any new types.
///
/// Method body **id-references** (`DispatchKind::Static.impl_id`,
/// `Reference::Function/Struct/...` ids in method bodies) stay in the
/// imported module's id-space and are stale — same limitation as
/// [`inline_imported_functions`]. A follow-up commit walks bodies and
/// remaps those ids.
///
/// Impls whose target struct/enum was *not* cloned are skipped (they
/// would have nowhere to attach in the local module).
///
/// Records the (`imported_module_path`, `imported_impl_idx`) →
/// `local_impl_idx` mapping into `impl_remap` so the body-id remapping
/// pass can rewrite `DispatchKind::Static.impl_id` references in
/// cloned bodies.
pub(in crate::ir::monomorphise) fn inline_imported_impls(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_remap: &mut HashMap<(Vec<String>, u32), u32>,
) -> Result<(), Vec<CompilerError>> {
    let mut errors: Vec<CompilerError> = Vec::new();
    for (module_path, imported) in super::sorted_imports(imported_modules) {
        for (imported_idx, impl_block) in imported.impls.iter().enumerate() {
            // Translate the impl's target id to the local clone's id.
            let new_target = match impl_block.target {
                ImplTarget::Struct(imported_id) => {
                    let Some(s) = imported.structs.get(imported_id.0 as usize) else {
                        continue;
                    };
                    // Phase 1a's clone carries the qualified name. A
                    // type the entry module mentions by name is
                    // registered at lowering time under its bare name
                    // instead, so look for both — otherwise the impl
                    // is skipped and the type arrives with none of its
                    // methods. Two definitions of one name is already
                    // a semantic error, so the bare name is
                    // unambiguous here.
                    let qualified = qualified_name(module_path, &s.name);
                    let Some(local_id) = module
                        .struct_id(&qualified)
                        .or_else(|| module.struct_id(&s.name))
                    else {
                        continue;
                    };
                    ImplTarget::Struct(local_id)
                }
                ImplTarget::Enum(imported_id) => {
                    let Some(e) = imported.enums.get(imported_id.0 as usize) else {
                        continue;
                    };
                    let qualified = qualified_name(module_path, &e.name);
                    let Some(local_id) = module
                        .enum_id(&qualified)
                        .or_else(|| module.enum_id(&e.name))
                    else {
                        continue;
                    };
                    ImplTarget::Enum(local_id)
                }
                ImplTarget::Primitive(_) => continue,
            };

            let mut clone = impl_block.clone();
            clone.target = new_target;

            // Translate trait_ref.trait_id into this module's id-space.
            //
            // The trait is usually not here yet: nothing in the entry
            // module names it as a type, so Phase 1a never collected
            // it. Leaving the imported id in place left the impl
            // pointing at whichever trait sits at that index locally —
            // or at nothing at all — and the conformance the impl
            // records was lost, so a generic bound on that trait could
            // no longer be satisfied by the imported struct.
            //
            // Clone the trait across instead, the same way a field's
            // type is cloned.
            if let Some(tref) = &mut clone.trait_ref {
                let source_name = imported
                    .traits
                    .get(tref.trait_id.0 as usize)
                    .map(|t| t.name.clone());
                if let Some(source_name) = source_name {
                    let qualified = qualified_name(module_path, &source_name);
                    if let Some(local_id) = module.trait_id(&qualified) {
                        tref.trait_id = local_id;
                    } else {
                        let args = tref.args.clone();
                        match specialise_external(
                            module,
                            imported,
                            module_path,
                            &source_name,
                            &args,
                        ) {
                            Ok((crate::ir::ResolvedType::Trait(local_id), _)) => {
                                tref.trait_id = local_id;
                            }
                            Ok(_) => {}
                            Err(e) => errors.push(e),
                        }
                    }
                }
            }

            // Externalise types in each method's signature + body. The
            // worklist re-runs Phase 1a and clones any types these
            // methods reference for the first time.
            for method in &mut clone.functions {
                walk_function_types_mut(method, &mut |ty| {
                    externalise_imported_refs(ty, imported, module_path);
                });
            }

            // Record the index mapping for body-id remap: the new
            // local impl is appended to module.impls; its index is the
            // current length minus 1 after the push.
            let local_idx = u32::try_from(module.impls.len()).unwrap_or(u32::MAX);
            module.impls.push(clone);
            impl_remap.insert(
                (
                    module_path.clone(),
                    u32::try_from(imported_idx).unwrap_or(u32::MAX),
                ),
                local_idx,
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Phase 1b: inline every imported function into the current module under
/// a qualified name.
///
/// Each clone has its signature and body types externalised via
/// [`externalise_imported_refs`] so subsequent worklist iterations of
/// [`super::specialise::specialise_external_instantiations`] pick up
/// any newly-introduced type references and clone them too. Body
/// id-references (`ReferenceTarget::Function/Struct/Enum/Trait/ModuleLet`,
/// `FunctionCall.function_id`, `DispatchKind::Static.impl_id`) stay
/// in their imported-module id-space — correct for leaf functions whose
/// bodies only reference `Param` / `Local` / primitive types, but will
/// produce stale references if the body touches other module-level
/// items. A follow-up commit walks and remaps those id references.
///
/// `FormaLang`'s IR doesn't carry function visibility today, so every
/// function in each imported module is cloned. Unused clones are removed
/// by `DeadCodeEliminationPass` in the codegen pipeline.
pub(in crate::ir::monomorphise) fn inline_imported_functions(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    for (module_path, imported) in super::sorted_imports(imported_modules) {
        for func in &imported.functions {
            let qualified = qualified_name(module_path, &func.name);

            // Skip if a clone with this name already exists. Defends
            // against double-processing when the same module appears
            // under multiple aliases or the pass is run twice.
            if module.function_id(&qualified).is_some() {
                continue;
            }

            let mut clone = func.clone();
            clone.name.clone_from(&qualified);

            // Externalise types in signature + body. The next worklist
            // iteration of specialise_external_instantiations re-collects
            // the produced External references and clones them.
            walk_function_types_mut(&mut clone, &mut |ty| {
                externalise_imported_refs(ty, imported, module_path);
            });

            if let Err(e) = module.add_function(qualified, clone) {
                errors.push(e);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
