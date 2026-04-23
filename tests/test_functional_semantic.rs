//! Functional tests for semantic analysis via the public API.
//!
//! Covers Bug 1: circular import not detected when using `compile_with_resolver`.

use formalang::compile_with_resolver;
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use std::collections::HashMap;
use std::path::PathBuf;

/// In-memory module resolver used in tests.
struct MemResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MemResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn add(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
    }
}

impl ModuleResolver for MemResolver {
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

// =============================================================================
// Bug 1: Circular import not detected via compile_with_resolver
// =============================================================================

#[test]
fn test_circular_import_public_api_a_imports_b_imports_a() -> Result<(), Box<dyn std::error::Error>>
{
    // Module A re-exports from B, module B re-exports from A — a direct cycle
    // detectable through the import graph.
    //
    // a.forma:  pub use b::Bar
    // b.forma:  pub use a::Foo
    // root:     use a::Foo
    //
    // When the root file is compiled via compile_with_resolver, the analyzer
    // should detect the cycle and return a CircularImport error.
    let mut resolver = MemResolver::new();

    resolver.add(
        vec!["a".to_string()],
        r"
pub struct Foo { value: Number }
pub use b::Bar
",
    );

    resolver.add(
        vec!["b".to_string()],
        r"
pub struct Bar { name: String }
pub use a::Foo
",
    );

    let source = "use a::Foo\nstruct Root { foo: Foo }";

    let result = compile_with_resolver(source, resolver);

    if result.is_ok() {
        return Err("Expected a circular import error but compilation succeeded".into());
    }

    let errors = result.err().ok_or("expected Err but got Ok")?;
    if !errors
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::CircularImport { .. }))
    {
        return Err(format!("Expected at least one CircularImport error, got: {errors:?}").into());
    }
    Ok(())
}
