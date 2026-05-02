//! Integration tests for cross-module Direction A inlining (CM-A — CM-I).
//!
//! These tests exercise `compile_to_ir_with_resolver` end-to-end and
//! assert that the post-pipeline `IrModule` is fully self-contained:
//! no `External` references survive, cross-module function calls
//! resolve, imported impls are present, the module tree is spliced.

use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use formalang::ir::ResolvedType;
use std::collections::HashMap;
use std::path::PathBuf;

struct MockResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MockResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }
    fn add(&mut self, path: Vec<String>, source: &str) {
        let file = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file));
    }
}

impl ModuleResolver for MockResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(&path.to_vec())
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: vec![],
            })
    }
}

/// CM-B: a non-generic struct used in a field type from another
/// module gets cloned into the entry under its qualified name and
/// the External reference is rewritten to the local clone.
#[test]
fn non_generic_struct_inlined() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add(
        vec!["helper".to_string()],
        "pub struct Helper { a: I32, b: I32 }",
    );
    let main = "use helper::Helper\nstruct Main { h: Helper }\n";

    let module = formalang::compile_to_ir_with_resolver(main, resolver)
        .map_err(|errors| format!("compile failed: {errors:?}"))?;

    // The cloned Helper exists under its qualified name.
    let cloned = module
        .struct_id("helper::Helper")
        .ok_or("cloned helper::Helper missing from entry module")?;

    // Main.h's type is the local clone, not External.
    let main_struct = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("Main struct missing")?;
    let h_field = main_struct
        .fields
        .iter()
        .find(|f| f.name == "h")
        .ok_or("h field missing")?;
    match &h_field.ty {
        ResolvedType::Struct(id) if *id == cloned => {}
        other => return Err(format!("h.ty = {other:?}; expected Struct({cloned:?})").into()),
    }
    Ok(())
}

/// CM-A exit criterion: zero `ResolvedType::External` references in
/// the post-pipeline IR for a non-generic cross-module program.
#[test]
fn no_external_references_after_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add(
        vec!["geom".to_string()],
        "pub struct Point { x: I32, y: I32 }\npub struct Line { a: Point, b: Point }\n",
    );
    let main = "use geom::Line\nstruct Main { line: Line }\n";

    let module = formalang::compile_to_ir_with_resolver(main, resolver)
        .map_err(|errors| format!("compile failed: {errors:?}"))?;

    fn has_external(ty: &ResolvedType) -> bool {
        match ty {
            ResolvedType::External { .. } => true,
            ResolvedType::Array(inner)
            | ResolvedType::Range(inner)
            | ResolvedType::Optional(inner) => has_external(inner),
            ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| has_external(t)),
            ResolvedType::Dictionary { key_ty, value_ty } => {
                has_external(key_ty) || has_external(value_ty)
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => param_tys.iter().any(|(_, t)| has_external(t)) || has_external(return_ty),
            ResolvedType::Generic { args, .. } => args.iter().any(has_external),
            _ => false,
        }
    }
    for s in &module.structs {
        for f in &s.fields {
            if has_external(&f.ty) {
                return Err(format!("struct {} field {} carries External", s.name, f.name).into());
            }
        }
    }
    Ok(())
}

/// CM-I: imported module's IrModuleNode tree is spliced under its
/// path in entry's module tree, populated with translated ids.
#[test]
fn module_tree_includes_imported_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add(
        vec!["util".to_string()],
        "pub struct Helper { v: I32 }\n",
    );
    let main = "use util::Helper\nstruct Main { h: Helper }\n";

    let module = formalang::compile_to_ir_with_resolver(main, resolver)
        .map_err(|errors| format!("compile failed: {errors:?}"))?;

    // `util` should appear as a top-level module node, with at least
    // one struct entry pointing at the cloned Helper.
    let util_node = module
        .modules
        .iter()
        .find(|n| n.name == "util")
        .ok_or("util node missing from entry module tree")?;
    let helper_id = module
        .struct_id("util::Helper")
        .ok_or("util::Helper struct missing")?;
    if !util_node.structs.contains(&helper_id) {
        return Err(format!(
            "util node didn't reference cloned Helper id {helper_id:?}; node: {util_node:?}"
        )
        .into());
    }
    Ok(())
}
