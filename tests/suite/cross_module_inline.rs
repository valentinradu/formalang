//! Integration tests for cross-module Direction A inlining (CM-A — CM-I).
//!
//! These tests exercise `compile_to_ir_with_resolver` end-to-end and
//! assert that the post-pipeline `IrModule` is fully self-contained:
//! no `External` references survive, cross-module function calls
//! resolve, imported impls are present, the module tree is spliced.

use formalang::ir::ResolvedType;
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
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
            .get(path)
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
    let ResolvedType::Struct(id) = &h_field.ty else {
        return Err(format!("h.ty = {:?}; expected Struct({cloned:?})", h_field.ty).into());
    };
    if *id != cloned {
        return Err(format!("h.ty = Struct({id:?}); expected Struct({cloned:?})").into());
    }
    Ok(())
}

fn has_external(ty: &ResolvedType) -> bool {
    // Built-in compound types ride through `ResolvedType::Generic`; the
    // `Generic` arm walks every payload so a bare `External` inside any
    // of them is detected.
    match ty {
        ResolvedType::External { .. } => true,
        ResolvedType::Tuple(fields) => fields.iter().any(|(_, t)| has_external(t)),
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => param_tys.iter().any(|(_, t)| has_external(t)) || has_external(return_ty),
        ResolvedType::Generic { args, .. } => args.iter().any(has_external),
        ResolvedType::Primitive(_)
        | ResolvedType::Struct(_)
        | ResolvedType::Trait(_)
        | ResolvedType::Enum(_)
        | ResolvedType::TypeParam(_)
        | ResolvedType::Error => false,
    }
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
    for s in &module.structs {
        for f in &s.fields {
            if has_external(&f.ty) {
                return Err(format!("struct {} field {} carries External", s.name, f.name).into());
            }
        }
    }
    Ok(())
}

/// CM-I: imported module's `IrModuleNode` tree is spliced under its
/// path in entry's module tree, populated with translated ids.
#[test]
fn module_tree_includes_imported_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add(vec!["util".to_string()], "pub struct Helper { v: I32 }\n");
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

/// Cross-module `file_table`: every cloned item's `IrSpan.file` resolves
/// through the entry's `file_path` to the imported source path.
/// Without Phase 2b, the cloned struct's span would still reference the
/// imported module's id-space (which the entry's `file_table` doesn't
/// know about), and `entry.file_path(span.file)` would dangle.
#[test]
fn cloned_item_spans_resolve_to_imported_source() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockResolver::new();
    resolver.add(
        vec!["geom".to_string()],
        "pub struct Point { x: I32, y: I32 }\n",
    );
    let main = "use geom::Point\nstruct Main { p: Point }\n";

    let module = formalang::compile_to_ir_with_resolver(main, resolver)
        .map_err(|errors| format!("compile failed: {errors:?}"))?;

    let cloned_id = module
        .struct_id("geom::Point")
        .ok_or("geom::Point struct missing")?;
    let cloned = module
        .structs
        .iter()
        .find(|s| s.name == "geom::Point")
        .ok_or("cloned struct unexpectedly absent given valid id")?;
    let _ = cloned_id;

    // The cloned struct's span must NOT be synthetic — it came from
    // a real source file (the imported geom module).
    if cloned.span.file.is_synthetic() {
        return Err(
            "cloned geom::Point span was left synthetic; file_table integration didn't fire".into(),
        );
    }
    // Resolving through the entry module's file_table must succeed
    // and point at the imported source path.
    let path = module
        .file_path(cloned.span.file)
        .ok_or("entry file_table didn't resolve the cloned struct's FileId")?;
    if !path.to_string_lossy().contains("geom") {
        return Err(format!(
            "expected the resolved path to mention geom, got {}",
            path.display()
        )
        .into());
    }
    Ok(())
}
