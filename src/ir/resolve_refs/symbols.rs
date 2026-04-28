//! Module-scope symbol table built once per pass invocation.
//!
//! Maps each module-level name (function / struct / enum / trait /
//! let) to its [`ReferenceTarget`] variant. Currently a flat top-level
//! lookup; nested-module resolution is a follow-up.

use std::collections::HashMap;

use crate::ir::{EnumId, FunctionId, IrModule, LetId, ReferenceTarget, StructId, TraitId};

/// Registered targets for cross-module imports. Each entry maps the
/// **unqualified** import-item name to an `External` target carrying
/// the resolved `module_path` so backends can re-link later.
pub(super) fn register_imports(by_name: &mut HashMap<String, ReferenceTarget>, module: &IrModule) {
    for import in &module.imports {
        for item in &import.items {
            // Don't shadow a same-named local definition; locals win.
            by_name
                .entry(item.name.clone())
                .or_insert_with(|| ReferenceTarget::External {
                    module_path: import.module_path.clone(),
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                });
        }
    }
}

pub(super) struct ModuleSymbols {
    pub(super) by_name: HashMap<String, ReferenceTarget>,
}

impl ModuleSymbols {
    pub(super) fn build(module: &IrModule) -> Self {
        let mut by_name = HashMap::new();
        for (i, f) in module.functions.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "function count is bounded by add_function's u32 ceiling upstream"
            )]
            by_name.insert(
                f.name.clone(),
                ReferenceTarget::Function(FunctionId(i as u32)),
            );
        }
        for (i, s) in module.structs.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "struct count bounded upstream"
            )]
            by_name.insert(s.name.clone(), ReferenceTarget::Struct(StructId(i as u32)));
        }
        for (i, e) in module.enums.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "enum count bounded upstream"
            )]
            by_name.insert(e.name.clone(), ReferenceTarget::Enum(EnumId(i as u32)));
        }
        for (i, t) in module.traits.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "trait count bounded upstream"
            )]
            by_name.insert(t.name.clone(), ReferenceTarget::Trait(TraitId(i as u32)));
        }
        for (i, l) in module.lets.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "let count bounded upstream"
            )]
            by_name.insert(l.name.clone(), ReferenceTarget::ModuleLet(LetId(i as u32)));
        }
        register_imports(&mut by_name, module);
        Self { by_name }
    }
}
