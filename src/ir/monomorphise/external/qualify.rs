//! Phase 1f: qualify bare-name paths in `FunctionCall` and `Reference`
//! expressions. After Phases 1b–1e a cloned function originally written
//! as `greet()` (with a top-of-file `use helper::greet`) is now named
//! `helper::greet` in the entry module, but its callsites still carry
//! the bare path `["greet"]`. This pass rewrites those single-segment
//! paths to their qualified form using a per-item context derived from
//! each cloned (or entry-native) item's source-module import set.

use std::collections::HashMap;

use crate::ir::{IrExpr, IrModule};

use super::super::walkers::walk_expr_children_mut;
use super::naming::{imported_path_of, qualified_name};

/// Phase 1f entry point. Walks every function/method/let body in
/// `module` with a per-item bare-name → qualified-path map.
///
/// - **Cloned imported items** use their source module's own
///   definitions + that module's `IrImport` list. So a body cloned
///   from `helper.fv` (with `use other::compute`) referencing
///   `compute` resolves to `["other", "compute"]`.
/// - **Entry-native items** use the entry module's `IrImport` list.
///   Bare references to imports like `use helper::greet` resolve to
///   `["helper", "greet"]`. The entry's own definitions are left
///   bare — `ResolveReferencesPass`'s `module_prefix` fallback
///   handles them.
pub(in crate::ir::monomorphise) fn qualify_imported_paths(
    module: &mut IrModule,
    imported_modules: &HashMap<Vec<String>, IrModule>,
) {
    if imported_modules.is_empty() {
        return;
    }
    // Snapshot the entry module's imports for entry-native item bodies.
    let entry_imports = module.imports.clone();

    // Pre-compute the source module for each item (functions, impl
    // methods, lets) so the body walk doesn't need to look up while
    // borrowing module mutably.
    let func_sources: Vec<Option<Vec<String>>> = module
        .functions
        .iter()
        .map(|f| imported_path_of(&f.name, imported_modules))
        .collect();
    let impl_method_sources: Vec<Vec<Option<Vec<String>>>> = module
        .impls
        .iter()
        .map(|imp| {
            let impl_source = match imp.target {
                crate::ir::ImplTarget::Struct(id) => module
                    .get_struct(id)
                    .and_then(|s| imported_path_of(&s.name, imported_modules)),
                crate::ir::ImplTarget::Enum(id) => module
                    .get_enum(id)
                    .and_then(|e| imported_path_of(&e.name, imported_modules)),
                crate::ir::ImplTarget::Primitive(_) => None,
            };
            imp.functions
                .iter()
                .map(|m| {
                    impl_source
                        .clone()
                        .or_else(|| imported_path_of(&m.name, imported_modules))
                })
                .collect()
        })
        .collect();
    let let_sources: Vec<Option<Vec<String>>> = module
        .lets
        .iter()
        .map(|l| imported_path_of(&l.name, imported_modules))
        .collect();

    // Walk each body with its precomputed source.
    for (func, source) in module.functions.iter_mut().zip(func_sources.iter()) {
        if let Some(body) = &mut func.body {
            let context =
                build_qualification_context(source.as_deref(), &entry_imports, imported_modules);
            if !context.is_empty() {
                qualify_paths_in_expr(body, &context);
            }
        }
    }
    for (impl_block, method_sources) in module.impls.iter_mut().zip(impl_method_sources.iter()) {
        for (method, source) in impl_block.functions.iter_mut().zip(method_sources.iter()) {
            if let Some(body) = &mut method.body {
                let context = build_qualification_context(
                    source.as_deref(),
                    &entry_imports,
                    imported_modules,
                );
                if !context.is_empty() {
                    qualify_paths_in_expr(body, &context);
                }
            }
        }
    }
    for (let_binding, source) in module.lets.iter_mut().zip(let_sources.iter()) {
        let context =
            build_qualification_context(source.as_deref(), &entry_imports, imported_modules);
        if !context.is_empty() {
            qualify_paths_in_expr(&mut let_binding.value, &context);
        }
    }
}

/// Build the bare-name → qualified-path map for a single item's body.
///
/// `source` is the imported module path the cloned item came from, or
/// `None` for entry-native items. `entry_imports` is the entry
/// module's `IrImport` list (used when source is None).
fn build_qualification_context(
    source: Option<&[String]>,
    entry_imports: &[crate::ir::IrImport],
    imported_modules: &HashMap<Vec<String>, IrModule>,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    if let Some(helper_path) = source {
        // Cloned item — use the source module's own items + imports.
        let Some(helper) = imported_modules.get(helper_path) else {
            return map;
        };
        // Helper's own items: cloned to entry under qualified name.
        // `helper.fv: pub fn helper_b()` cloned as `"helper::helper_b"`.
        let prefix = qualified_name(helper_path, "");
        for f in &helper.functions {
            map.entry(f.name.clone())
                .or_insert_with(|| format!("{prefix}{}", f.name));
        }
        for s in &helper.structs {
            map.entry(s.name.clone())
                .or_insert_with(|| format!("{prefix}{}", s.name));
        }
        for e in &helper.enums {
            map.entry(e.name.clone())
                .or_insert_with(|| format!("{prefix}{}", e.name));
        }
        for t in &helper.traits {
            map.entry(t.name.clone())
                .or_insert_with(|| format!("{prefix}{}", t.name));
        }
        for l in &helper.lets {
            map.entry(l.name.clone())
                .or_insert_with(|| format!("{prefix}{}", l.name));
        }
        // Helper's imports: each item bare-name maps to its declaring
        // module's qualified form.
        register_imports_into(&helper.imports, &mut map);
    } else {
        // Entry-native item — use entry's IrImport list. Entry's own
        // definitions are left bare (ResolveReferencesPass handles
        // them via module_prefix or by_name lookup).
        register_imports_into(entry_imports, &mut map);
    }

    map
}

fn register_imports_into(imports: &[crate::ir::IrImport], map: &mut HashMap<String, String>) {
    for imp in imports {
        let prefix = qualified_name(&imp.module_path, "");
        for item in &imp.items {
            map.entry(item.name.clone())
                .or_insert_with(|| format!("{prefix}{}", item.name));
        }
    }
}

fn qualify_paths_in_expr(expr: &mut IrExpr, bare_to_qualified: &HashMap<String, String>) {
    qualify_self_path(expr, bare_to_qualified);
    walk_expr_children_mut(expr, &mut |child| {
        qualify_paths_in_expr(child, bare_to_qualified);
    });
}

/// Rewrite a single-segment path on this node to its qualified form, if
/// applicable. Only `FunctionCall` (with no resolved `function_id`) and
/// `Reference` (still `Unresolved`) carry paths that should be qualified
/// here; every other variant is intentionally a no-op.
fn qualify_self_path(expr: &mut IrExpr, bare_to_qualified: &HashMap<String, String>) {
    let path: &mut Vec<String> = match expr {
        IrExpr::FunctionCall {
            path,
            function_id: None,
            ..
        }
        | IrExpr::Reference {
            path,
            target: crate::ir::ReferenceTarget::Unresolved,
            ..
        } => path,
        IrExpr::FunctionCall { .. }
        | IrExpr::Reference { .. }
        | IrExpr::Literal { .. }
        | IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::BinaryOp { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. }
        | IrExpr::Block { .. } => return,
    };
    if let [single] = path.as_slice() {
        if let Some(qualified) = bare_to_qualified.get(single) {
            *path = qualified.split("::").map(String::from).collect();
        }
    }
}
