//! Phase 2b: register each imported module's source files in the entry
//! module's `file_table`, then remap every cloned item's `IrSpan.file`
//! to the entry-side id-space.
//!
//! Without this pass, cloned items still carry `IrSpan.file` values from
//! their imported `IrModule`'s file table — which is dropped after
//! monomorphisation, leaving spans pointing at indices that don't
//! resolve via `IrModule.file_path` on the entry. After this pass the
//! entry's `file_table` is the single source of truth: every span's
//! `FileId` either targets an entry-table slot or is `FileId::SYNTHETIC`.

use std::collections::HashMap;

use crate::ir::{ImplTarget, IrModule};

use super::super::walkers::{walk_expr_spans_mut, walk_function_spans_mut};
use super::naming::imported_path_of;

pub(in crate::ir::monomorphise) fn remap_imported_file_ids(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    if imported_modules.is_empty() {
        return;
    }
    let remaps = build_file_remap_table(module, imported_modules);
    remap_function_file_ids(module, &remaps, imported_modules);
    remap_struct_file_ids(module, &remaps, imported_modules);
    remap_enum_file_ids(module, &remaps, imported_modules);
    remap_let_file_ids(module, &remaps, imported_modules);
    remap_impl_file_ids(module, &remaps, imported_modules);
}

/// Per-module `FileId` remap: imported `FileId(N)` to entry `FileId(M)`.
/// Synthetic `FileId(0)` is preserved as-is. Registers every path the
/// imported module knows about so any span (including those from
/// transitive `use` chains the imported module had) round-trips correctly
/// through `module.file_path`.
fn build_file_remap_table(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>> {
    let mut remaps: HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>> =
        HashMap::with_capacity(imported_modules.len());
    for (path, imported) in imported_modules {
        let cap = imported.file_table.len().saturating_add(1);
        let mut per_module: HashMap<crate::ir::FileId, crate::ir::FileId> =
            HashMap::with_capacity(cap);
        per_module.insert(crate::ir::FileId::SYNTHETIC, crate::ir::FileId::SYNTHETIC);
        for (idx, file) in imported.file_table.iter().enumerate() {
            // Imported FileId is offset by 1 (id 0 reserved for synthetic).
            let imported_id = crate::ir::FileId(u32::try_from(idx).unwrap_or(0).saturating_add(1));
            let entry_id = module.register_file(file.clone());
            per_module.insert(imported_id, entry_id);
        }
        remaps.insert(path.clone(), per_module);
    }
    remaps
}

fn remap_function_file_ids(
    module: &mut IrModule,
    remaps: &HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    let indices: Vec<(usize, Vec<String>)> = module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(i, f)| imported_path_of(&f.name, imported_modules).map(|p| (i, p)))
        .collect();
    for (idx, source_path) in indices {
        let Some(remap) = remaps.get(&source_path) else {
            continue;
        };
        if let Some(func) = module.functions.get_mut(idx) {
            apply_file_remap(&mut func.span, remap);
            walk_function_spans_mut(func, &mut |s| apply_file_remap(s, remap));
        }
    }
}

fn remap_struct_file_ids(
    module: &mut IrModule,
    remaps: &HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    let indices: Vec<(usize, Vec<String>)> = module
        .structs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| imported_path_of(&s.name, imported_modules).map(|p| (i, p)))
        .collect();
    for (idx, source_path) in indices {
        let Some(remap) = remaps.get(&source_path) else {
            continue;
        };
        if let Some(s) = module.structs.get_mut(idx) {
            apply_file_remap(&mut s.span, remap);
            for field in &mut s.fields {
                apply_file_remap(&mut field.span, remap);
                if let Some(d) = &mut field.default {
                    walk_expr_spans_mut(d, &mut |sp| apply_file_remap(sp, remap));
                }
            }
        }
    }
}

fn remap_enum_file_ids(
    module: &mut IrModule,
    remaps: &HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    let indices: Vec<(usize, Vec<String>)> = module
        .enums
        .iter()
        .enumerate()
        .filter_map(|(i, e)| imported_path_of(&e.name, imported_modules).map(|p| (i, p)))
        .collect();
    for (idx, source_path) in indices {
        let Some(remap) = remaps.get(&source_path) else {
            continue;
        };
        if let Some(e) = module.enums.get_mut(idx) {
            apply_file_remap(&mut e.span, remap);
            for variant in &mut e.variants {
                apply_file_remap(&mut variant.span, remap);
                for field in &mut variant.fields {
                    apply_file_remap(&mut field.span, remap);
                    if let Some(d) = &mut field.default {
                        walk_expr_spans_mut(d, &mut |sp| {
                            apply_file_remap(sp, remap);
                        });
                    }
                }
            }
        }
    }
}

fn remap_let_file_ids(
    module: &mut IrModule,
    remaps: &HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    let indices: Vec<(usize, Vec<String>)> = module
        .lets
        .iter()
        .enumerate()
        .filter_map(|(i, l)| imported_path_of(&l.name, imported_modules).map(|p| (i, p)))
        .collect();
    for (idx, source_path) in indices {
        let Some(remap) = remaps.get(&source_path) else {
            continue;
        };
        if let Some(l) = module.lets.get_mut(idx) {
            apply_file_remap(&mut l.span, remap);
            walk_expr_spans_mut(&mut l.value, &mut |sp| {
                apply_file_remap(sp, remap);
            });
        }
    }
}

/// Impl blocks are identified by their target type's name.
fn remap_impl_file_ids(
    module: &mut IrModule,
    remaps: &HashMap<Vec<String>, HashMap<crate::ir::FileId, crate::ir::FileId>>,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    let sources: Vec<(usize, Option<Vec<String>>)> = module
        .impls
        .iter()
        .enumerate()
        .map(|(i, imp)| {
            let source = match imp.target {
                ImplTarget::Struct(id) => module
                    .get_struct(id)
                    .and_then(|s| imported_path_of(&s.name, imported_modules)),
                ImplTarget::Enum(id) => module
                    .get_enum(id)
                    .and_then(|e| imported_path_of(&e.name, imported_modules)),
                ImplTarget::Primitive(_) => None,
            };
            (i, source)
        })
        .collect();
    for (idx, source_opt) in sources {
        let Some(source_path) = source_opt else {
            continue;
        };
        let Some(remap) = remaps.get(&source_path) else {
            continue;
        };
        if let Some(imp) = module.impls.get_mut(idx) {
            apply_file_remap(&mut imp.span, remap);
            for method in &mut imp.functions {
                apply_file_remap(&mut method.span, remap);
                walk_function_spans_mut(method, &mut |sp| apply_file_remap(sp, remap));
            }
        }
    }
}

fn apply_file_remap(
    span: &mut crate::ir::IrSpan,
    remap: &HashMap<crate::ir::FileId, crate::ir::FileId>,
) {
    if let Some(new_file) = remap.get(&span.file).copied() {
        span.file = new_file;
    }
}
