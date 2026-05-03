//! Phase 1e: walk every cloned item's body and translate id references
//! from the imported module's id-space into the entry module's id-space.
//!
//! - `FunctionCall.function_id: Some(N)` → translate via the relevant
//!   imported module's function map.
//! - `DispatchKind::Static.impl_id: ImplId(N)` → translate via the
//!   relevant imported module's impl map.
//!
//! `Reference.target` is left as-is (lowering emitted Unresolved;
//! `ResolveReferencesPass` rebinds via the path + module-prefix
//! resolution updated to use the qualified-name path).

use std::collections::HashMap;

use crate::ir::{DispatchKind, FunctionId, ImplId, ImplTarget, IrExpr, IrModule};

use super::super::walkers::walk_expr_children_mut;
use super::naming::{imported_path_of, qualified_name};

/// Per-imported-module id translation table for the body-id remap pass.
#[derive(Default)]
pub(super) struct ItemMaps {
    /// imported function index → local `FunctionId`
    pub functions: HashMap<u32, FunctionId>,
    /// imported impl index → local impl index
    pub impls: HashMap<u32, u32>,
}

/// Build per-imported-module id translation tables from the final
/// state of `module` after Phases 1a-1d. For each imported module
/// path, records (`imported_id` → `local_id`) for functions and impls so
/// the body-id remap pass can translate ids in cloned bodies.
///
/// Functions look up their local id by qualified name. Impl
/// translation is fed in directly from Phase 1c (no name to look up
/// against; impls are positional).
pub(super) fn build_item_maps(
    module: &IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_clone_remap: &HashMap<(Vec<String>, u32), u32>,
) -> HashMap<Vec<String>, ItemMaps> {
    let mut out: HashMap<Vec<String>, ItemMaps> = HashMap::new();
    for (path, imported) in imported_modules {
        let mut maps = ItemMaps::default();
        for (i, f) in imported.functions.iter().enumerate() {
            let qualified = qualified_name(path, &f.name);
            if let Some(local_id) = module.function_id(&qualified) {
                maps.functions
                    .insert(u32::try_from(i).unwrap_or(u32::MAX), local_id);
            }
        }
        for ((p, imported_idx), local_idx) in impl_clone_remap {
            if p == path {
                maps.impls.insert(*imported_idx, *local_idx);
            }
        }
        out.insert(path.clone(), maps);
    }
    out
}

/// Body id-remap entry point. Items not recognised as clones (no
/// qualified-name prefix matching any imported module path) are
/// skipped — they're entry-module native, their ids are valid as-is.
pub(in crate::ir::monomorphise) fn remap_imported_body_ids(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
    impl_clone_remap: &HashMap<(Vec<String>, u32), u32>,
) {
    if imported_modules.is_empty() {
        return;
    }
    let item_maps = build_item_maps(module, imported_modules, impl_clone_remap);

    // Functions
    for func in &mut module.functions {
        let Some(path) = imported_path_of(&func.name, imported_modules) else {
            continue;
        };
        let Some(maps) = item_maps.get(&path) else {
            continue;
        };
        if let Some(body) = &mut func.body {
            remap_expr_ids(body, maps);
        }
    }

    // Impl methods. Identify which impls are clones via their target's
    // qualified name.
    let impl_paths: Vec<Option<Vec<String>>> = module
        .impls
        .iter()
        .map(|impl_block| match impl_block.target {
            ImplTarget::Struct(id) => module
                .get_struct(id)
                .and_then(|s| imported_path_of(&s.name, imported_modules)),
            ImplTarget::Enum(id) => module
                .get_enum(id)
                .and_then(|e| imported_path_of(&e.name, imported_modules)),
            ImplTarget::Primitive(_) => None,
        })
        .collect();
    for (impl_block, path_opt) in module.impls.iter_mut().zip(impl_paths.iter()) {
        let Some(path) = path_opt else { continue };
        let Some(maps) = item_maps.get(path) else {
            continue;
        };
        for method in &mut impl_block.functions {
            if let Some(body) = &mut method.body {
                remap_expr_ids(body, maps);
            }
        }
    }

    // Lets
    for let_binding in &mut module.lets {
        let Some(path) = imported_path_of(&let_binding.name, imported_modules) else {
            continue;
        };
        let Some(maps) = item_maps.get(&path) else {
            continue;
        };
        remap_expr_ids(&mut let_binding.value, maps);
    }
}

/// Walk an expression tree and apply id remaps for `FunctionCall.function_id`
/// and `DispatchKind::Static.impl_id`.
fn remap_expr_ids(expr: &mut IrExpr, maps: &ItemMaps) {
    remap_self_ids(expr, maps);
    walk_expr_children_mut(expr, &mut |child| remap_expr_ids(child, maps));
}

/// Apply id remaps to this node only, leaving children to the caller.
/// Only `FunctionCall` and `MethodCall` (with `Static` dispatch) carry
/// ids that need remapping; every other variant is intentionally a no-op.
fn remap_self_ids(expr: &mut IrExpr, maps: &ItemMaps) {
    match expr {
        IrExpr::FunctionCall {
            function_id: Some(old),
            ..
        } => {
            if let Some(new) = maps.functions.get(&old.0) {
                *old = *new;
            }
        }
        IrExpr::MethodCall {
            dispatch: DispatchKind::Static { impl_id },
            ..
        } => {
            if let Some(new) = maps.impls.get(&impl_id.0) {
                *impl_id = ImplId(*new);
            }
        }
        IrExpr::FunctionCall { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Literal { .. }
        | IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::BinaryOp { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. }
        | IrExpr::Block { .. } => {}
    }
}
