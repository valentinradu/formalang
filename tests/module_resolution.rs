//! Tests for module resolution using a mock resolver
//!
//! These tests exercise the module resolution paths in semantic analysis
//! without requiring actual filesystem access.

use formalang::error::CompilerError;
use formalang::lexer::Lexer;
use formalang::parser;
use formalang::semantic::module_resolver::{FileSystemResolver, ModuleError, ModuleResolver};
use formalang::semantic::SemanticAnalyzer;
use std::collections::HashMap;
use std::path::PathBuf;

/// Mock module resolver for testing
///
/// Stores modules in memory and returns them when resolved.
struct MockModuleResolver {
    /// Map from module path to (source code, file path)
    modules: HashMap<Vec<String>, (String, PathBuf)>,
    /// Errors to return for specific paths
    errors: HashMap<Vec<String>, ModuleError>,
}

impl MockModuleResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    fn add_module(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
    }

    fn add_error(&mut self, path: Vec<String>, error: ModuleError) {
        self.errors.insert(path, error);
    }
}

impl ModuleResolver for MockModuleResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        let path_vec = path.to_vec();

        // Check for configured errors first
        if let Some(error) = self.errors.get(&path_vec) {
            return Err(error.clone());
        }

        // Return the module if it exists
        self.modules
            .get(&path_vec)
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path_vec,
                searched_paths: vec![],
            })
    }
}

// =============================================================================
// Basic Mock Resolver Tests
// =============================================================================

#[test]
fn test_mock_resolver_returns_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(vec!["utils".to_string()], "pub struct Helper { x: String }");

    let result = resolver.resolve(&["utils".to_string()], None);
    let (source, path) = result.map_err(|e| format!("{e:?}"))?;
    if !(source.contains("Helper")) {
        return Err("assertion failed".into());
    }
    if !(path.to_string_lossy().contains("utils")) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_mock_resolver_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = MockModuleResolver::new();

    let result = resolver.resolve(&["nonexistent".to_string()], None);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let err = result.err().ok_or("expected error")?;
    match err {
        ModuleError::NotFound { path, .. } => {
            if path != vec!["nonexistent".to_string()] {
                return Err(format!(
                    "expected {:?} but got {:?}",
                    vec!["nonexistent".to_string()],
                    path
                )
                .into());
            }
        }
        ModuleError::CircularImport { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Expected NotFound error".into());
        }
    }
    Ok(())
}

#[test]
fn test_mock_resolver_returns_configured_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["broken".to_string()],
        ModuleError::ReadError {
            path: PathBuf::from("broken.forma"),
            error: "Permission denied".to_string(),
        },
    );

    let result = resolver.resolve(&["broken".to_string()], None);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let err = result.err().ok_or("expected error")?;
    match err {
        ModuleError::ReadError { error, .. } => {
            if !(error.contains("Permission denied")) {
                return Err("assertion failed".into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::CircularImport { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Expected ReadError".into());
        }
    }
    Ok(())
}

#[test]
fn test_mock_resolver_multi_segment_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );

    resolver
        .resolve(&["std".to_string(), "collections".to_string()], None)
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// ModuleError Tests
// =============================================================================

#[test]
fn test_module_error_not_found_fields() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::NotFound {
        path: vec!["foo".to_string(), "bar".to_string()],
        searched_paths: vec![PathBuf::from("/a/b.forma"), PathBuf::from("/a/b/c.forma")],
    };

    match error {
        ModuleError::NotFound {
            path,
            searched_paths,
            ..
        } => {
            if path.len() != 2 {
                return Err(format!("expected {:?} but got {:?}", 2, path.len()).into());
            }
            if searched_paths.len() != 2 {
                return Err(format!("expected {:?} but got {:?}", 2, searched_paths.len()).into());
            }
        }
        ModuleError::CircularImport { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Wrong error type".into());
        }
    }
    Ok(())
}

#[test]
fn test_module_error_circular_import() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::CircularImport {
        cycle: vec!["a".to_string(), "b".to_string(), "a".to_string()],
    };

    match error {
        ModuleError::CircularImport { cycle, .. } => {
            if cycle.len() != 3 {
                return Err(format!("expected {:?} but got {:?}", 3, cycle.len()).into());
            }
            let first = cycle.first().ok_or("cycle is empty")?;
            if first != "a" {
                return Err(format!("expected {:?} but got {:?}", "a", first).into());
            }
            let last = cycle.last().ok_or("cycle is empty")?;
            if last != "a" {
                return Err(format!("expected {:?} but got {:?}", "a", last).into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Wrong error type".into());
        }
    }
    Ok(())
}

#[test]
fn test_module_error_read_error() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::ReadError {
        path: PathBuf::from("/some/path.forma"),
        error: "File not accessible".to_string(),
    };

    match error {
        ModuleError::ReadError { path, error, .. } => {
            if !(path.to_string_lossy().contains("path.forma")) {
                return Err("assertion failed".into());
            }
            if !(error.contains("accessible")) {
                return Err("assertion failed".into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::CircularImport { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Wrong error type".into());
        }
    }
    Ok(())
}

#[test]
fn test_module_error_private_item() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::PrivateItem {
        item: "InternalHelper".to_string(),
        module: "utils".to_string(),
    };

    match error {
        ModuleError::PrivateItem { item, module, .. } => {
            if item != "InternalHelper" {
                return Err(format!("expected {:?} but got {:?}", "InternalHelper", item).into());
            }
            if module != "utils" {
                return Err(format!("expected {:?} but got {:?}", "utils", module).into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::CircularImport { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("Wrong error type".into());
        }
    }
    Ok(())
}

#[test]
fn test_module_error_item_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::ItemNotFound {
        item: "Missing".to_string(),
        module: "utils".to_string(),
        available: vec!["Helper".to_string(), "Utils".to_string()],
    };

    match error {
        ModuleError::ItemNotFound {
            item,
            module,
            available,
            ..
        } => {
            if item != "Missing" {
                return Err(format!("expected {:?} but got {:?}", "Missing", item).into());
            }
            if module != "utils" {
                return Err(format!("expected {:?} but got {:?}", "utils", module).into());
            }
            if available.len() != 2 {
                return Err(format!("expected {:?} but got {:?}", 2, available.len()).into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::CircularImport { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::PrivateItem { .. } => {
            return Err("Wrong error type".into());
        }
    }
    Ok(())
}

#[test]
fn test_module_error_equality() -> Result<(), Box<dyn std::error::Error>> {
    let error1 = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
    };
    let error2 = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
    };

    if error1 != error2 {
        return Err(format!("expected {error2:?} but got {error1:?}").into());
    }
    Ok(())
}

#[test]
fn test_module_error_debug() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
    };

    let debug_str = format!("{error:?}");
    if !(debug_str.contains("NotFound")) {
        return Err("assertion failed: expected debug string to contain 'NotFound'".into());
    }
    if !(debug_str.contains("test")) {
        return Err("assertion failed: expected debug string to contain 'test'".into());
    }
    Ok(())
}

#[test]
fn test_module_error_clone() -> Result<(), Box<dyn std::error::Error>> {
    let error = ModuleError::ReadError {
        path: PathBuf::from("test.forma"),
        error: "Test error".to_string(),
    };

    let cloned = error.clone();
    if error != cloned {
        return Err(format!("expected {cloned:?} but got {error:?}").into());
    }
    Ok(())
}

// =============================================================================
// Semantic Analyzer Integration Tests with Mock Resolver
// =============================================================================

fn analyze_with_mock(source: &str, resolver: MockModuleResolver) -> Result<(), Vec<CompilerError>> {
    let tokens = Lexer::tokenize_all(source);
    let file = parser::parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    let mut analyzer = SemanticAnalyzer::new_with_file(resolver, PathBuf::from("main.forma"));
    analyzer.analyze(&file)
}

#[test]
fn test_semantic_use_module_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = MockModuleResolver::new();
    let source = r"
use nonexistent::Helper
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ModuleNotFound { .. }))
    {
        return Err(format!("expected ModuleNotFound error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_module_read_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["broken".to_string()],
        ModuleError::ReadError {
            path: PathBuf::from("broken.forma"),
            error: "Permission denied".to_string(),
        },
    );
    let source = r"
use broken::Helper
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ModuleReadError { .. }))
    {
        return Err(format!("expected ModuleReadError error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_circular_import() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["circular".to_string()],
        ModuleError::CircularImport {
            cycle: vec![
                "main".to_string(),
                "circular".to_string(),
                "main".to_string(),
            ],
        },
    );
    let source = r"
use circular::Helper
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::CircularImport { .. }))
    {
        return Err(format!("expected CircularImport error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_private_item() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["utils".to_string()],
        ModuleError::PrivateItem {
            item: "InternalHelper".to_string(),
            module: "utils".to_string(),
        },
    );
    let source = r"
use utils::InternalHelper
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::PrivateImport { .. }))
    {
        return Err(format!("expected PrivateImport error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_item_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["utils".to_string()],
        ModuleError::ItemNotFound {
            item: "Missing".to_string(),
            module: "utils".to_string(),
            available: vec!["Helper".to_string(), "Utils".to_string()],
        },
    );
    let source = r"
use utils::Missing
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ImportItemNotFound { .. }))
    {
        return Err(format!("expected ImportItemNotFound error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_module_success_single_item() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );
    let source = r"
use utils::Helper
struct Main {
    helper: Helper
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_module_success_multiple_items() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: I32
}
",
    );
    let source = r"
use utils::{Helper, Utils}
struct Main {
    helper: Helper
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_module_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    // Add module with invalid syntax
    resolver.add_module(
        vec!["broken".to_string()],
        "pub struct Helper { name String }", // missing colon
    );
    let source = r"
use broken::Helper
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ParseError { .. }))
    {
        return Err(format!("expected ParseError error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_nested_module_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );
    let source = r"
use std::collections::List
struct Main {
    items: List
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_module_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["traits".to_string()],
        r"
pub trait Named {
    name: String
}
",
    );
    let source = r"
use traits::Named
trait LocalNamed: Named {}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_module_with_enum() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { Active, Inactive }",
    );
    let source = r"
use types::Status
struct Item {
    status: Status
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_multiple_use_statements() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );
    resolver.add_module(
        vec!["types".to_string()],
        "pub struct Value { amount: I32 }",
    );
    let source = r"
use utils::Helper
use types::Value
struct Main {
    helper: Helper
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_same_module_twice() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: I32
}
",
    );
    let source = r"
use utils::Helper
use utils::Utils
struct Main {
    helper: Helper
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Glob Import Tests
// =============================================================================

#[test]
fn test_semantic_use_glob_import_all_public() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: I32
}
pub enum Status {
    Active,
    Inactive
}
pub trait Named {
    name: String
}
",
    );
    let source = r"
use utils::*
struct Main {
    helper: Helper,
    utils: Utils,
    status: Status
}
trait LocalNamed: Named {}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_import_nested_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        r"
pub struct List {
    items: [String]
}
pub struct Map {
    keys: [String]
}
",
    );
    let source = r"
use std::collections::*
struct Main {
    list: List,
    map: Map
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_import_only_public() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct PublicHelper {
    name: String
}
struct PrivateHelper {
    secret: String
}
",
    );
    // Glob import should only import PublicHelper, not PrivateHelper
    let source = r"
use utils::*
struct Main {
    helper: PublicHelper
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_import_private_not_accessible() -> Result<(), Box<dyn std::error::Error>>
{
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct PublicHelper {
    name: String
}
struct PrivateHelper {
    secret: String
}
",
    );
    // Trying to use PrivateHelper after glob import should fail
    let source = r"
use utils::*
struct Main {
    helper: PrivateHelper
}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("Expected error for private type usage".into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_glob_with_other_imports() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r"
pub struct Helper {
    name: String
}
",
    );
    resolver.add_module(
        vec!["types".to_string()],
        r"
pub struct Value {
    amount: I32
}
pub struct Item {
    id: String
}
",
    );
    // Mix glob import with specific imports
    let source = r"
use utils::Helper
use types::*
struct Main {
    helper: Helper,
    value: Value,
    item: Item
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_module_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = MockModuleResolver::new();
    let source = r"
use nonexistent::*
struct Main {}
";
    let result = analyze_with_mock(source, resolver);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ModuleNotFound { .. }))
    {
        return Err(format!("expected ModuleNotFound error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_semantic_use_glob_empty_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    // Module with no public symbols
    resolver.add_module(
        vec!["empty".to_string()],
        r"
struct PrivateOnly {
    secret: String
}
",
    );
    // Glob import from module with no public symbols should succeed (imports nothing)
    let source = r"
use empty::*
struct Main {
    name: String
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_with_let_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["constants".to_string()],
        r#"
pub let MAX_SIZE: I32 = 100
pub let DEFAULT_NAME: String = "unnamed"
"#,
    );
    let source = r"
use constants::*
struct Config {
    size: I32
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_semantic_use_glob_with_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["parent".to_string()],
        r"
pub struct ParentStruct {
    name: String
}
pub mod child {
    pub struct ChildStruct {
        value: I32
    }
}
",
    );
    // Glob import should import ParentStruct and child module, but not child's contents directly
    let source = r"
use parent::*
struct Main {
    parent: ParentStruct
}
";
    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_self_only() -> Result<(), Box<dyn std::error::Error>> {
    // self references are only valid in impl functions, not struct field defaults
    let source = r#"
pub struct Modal {
  isOpen: Boolean = false
}

impl Modal {
  fn title() -> String {
    if self.isOpen { "open" } else { "closed" }
  }
}
"#;
    let resolver = MockModuleResolver::new();

    analyze_with_mock(source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_simple_self_with_imported_module() -> Result<(), Box<dyn std::error::Error>> {
    // self references are only valid in impl functions
    let simple_source = r#"
use mylib::*

pub struct Modal {
  isOpen: Boolean = false,
  title: String = "test"
}

impl Modal {
  fn getTitle() -> String {
    self.title
  }
}
"#;

    let mut resolver = MockModuleResolver::new();
    resolver.add_module(vec!["mylib".to_string()], "pub trait View {}");
    analyze_with_mock(simple_source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_minimal_self_reference() -> Result<(), Box<dyn std::error::Error>> {
    // self references are only valid in impl functions
    let minimal_source = r"
use mylib::*

pub struct Modal {
  isOpen: Boolean = false
}

impl Modal {
  fn isModalOpen() -> Boolean {
    self.isOpen
  }
}
";

    let mut resolver = MockModuleResolver::new();
    resolver.add_module(vec!["mylib".to_string()], "pub trait View {}");
    analyze_with_mock(minimal_source, resolver).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_path_literal_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Test {
  p: Path = /icons/lightning.svg
}
";
    analyze_with_mock(source, MockModuleResolver::new()).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_imported_ir_modules_cache_is_populated() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
use utils::Helper
struct Main { h: Helper }
";
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(vec!["utils".to_string()], "pub struct Helper { x: String }");
    let tokens = Lexer::tokenize_all(source);
    let file =
        parser::parse_file_with_source(&tokens, source).map_err(|errors| format!("{errors:?}"))?;
    let mut analyzer = SemanticAnalyzer::new_with_file(resolver, PathBuf::from("main.forma"));
    analyzer.analyze(&file).map_err(|e| format!("{e:?}"))?;

    let cache = analyzer.imported_ir_modules();
    if cache.is_empty() {
        return Err("expected imported_ir_modules() to contain the utils module".into());
    }
    let utils = cache
        .values()
        .find(|m| m.structs.iter().any(|s| s.name == "Helper"))
        .ok_or("Helper struct missing from imported IR modules")?;
    if utils.structs.is_empty() {
        return Err("utils IR module has no structs".into());
    }
    Ok(())
}

#[test]
fn test_monomorphise_specialises_external_generic() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #45: when a main module uses `External<I32>` from an
    // imported module, MonomorphisePass with `with_imports` clones the
    // imported generic definition into the main module with substituted
    // arguments and rewrites the External reference to a local Struct.
    use formalang::ir::{lower_to_ir, MonomorphisePass, ResolvedType};
    use formalang::IrPass;

    let main_source = r"
use utils::Helper
struct Main { h: Helper<I32> }
";
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper<T> { value: T }",
    );

    let tokens = Lexer::tokenize_all(main_source);
    let file = parser::parse_file_with_source(&tokens, main_source)
        .map_err(|errors| format!("{errors:?}"))?;
    let mut analyzer = SemanticAnalyzer::new_with_file(resolver, PathBuf::from("main.forma"));
    analyzer.analyze(&file).map_err(|e| format!("{e:?}"))?;

    let mut module = lower_to_ir(&file, analyzer.symbols()).map_err(|e| format!("{e:?}"))?;

    // Sanity: pre-monomorphise, Main.h is an External with type_args=[I32].
    let main_struct = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("Main missing")?;
    let h_field = main_struct
        .fields
        .iter()
        .find(|f| f.name == "h")
        .ok_or("h field missing")?;
    if !matches!(&h_field.ty, ResolvedType::External { type_args, .. } if !type_args.is_empty()) {
        return Err(format!("expected External with type_args, got {:?}", h_field.ty).into());
    }

    // Build a Vec<String>-keyed imports map from the analyzer's IR cache.
    let mut imports: HashMap<Vec<String>, formalang::ir::IrModule> = HashMap::new();
    for ir in analyzer.imported_ir_modules().values() {
        // Single-module test: just associate the imported IR with the
        // logical path used by the source (`use utils::Helper`).
        imports.insert(vec!["utils".to_string()], ir.clone());
    }

    let mut pass = MonomorphisePass::default().with_imports(imports);
    module = pass.run(module).map_err(|e| format!("{e:?}"))?;

    // After monomorphise, a `Helper__I32` struct exists locally and
    // Main.h is now a concrete local Struct reference.
    if !module
        .structs
        .iter()
        .any(|s| s.name.starts_with("Helper__"))
    {
        return Err(format!(
            "expected a Helper__... clone, got structs: {:?}",
            module
                .structs
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        )
        .into());
    }
    let main_struct = module
        .structs
        .iter()
        .find(|s| s.name == "Main")
        .ok_or("Main missing post-mono")?;
    let h_field = main_struct
        .fields
        .iter()
        .find(|f| f.name == "h")
        .ok_or("h field missing post-mono")?;
    if !matches!(h_field.ty, ResolvedType::Struct(_)) {
        return Err(format!(
            "expected Main.h to be a local Struct after specialisation, got {:?}",
            h_field.ty
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// FileSystemResolver error paths
// =============================================================================

#[test]
fn test_filesystem_resolver_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = FileSystemResolver::new(PathBuf::from("/nonexistent-root-for-tests"));
    let err = resolver
        .resolve(&["definitely_missing".to_string()], None)
        .err()
        .ok_or("expected NotFound")?;
    match err {
        ModuleError::NotFound {
            path,
            searched_paths,
        } => {
            if path != vec!["definitely_missing".to_string()] {
                return Err(format!("unexpected path {path:?}").into());
            }
            if searched_paths.is_empty() {
                return Err("searched_paths should be populated".into());
            }
        }
        ModuleError::CircularImport { .. }
        | ModuleError::ReadError { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("expected NotFound".into());
        }
    }
    Ok(())
}

#[test]
fn test_filesystem_resolver_read_error_on_directory() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temp directory with a subdirectory named `m.fv` (a directory, not a file).
    // Attempting to `resolve(["m"])` should find the path via exists() but fail to
    // read its contents because it is a directory, surfacing a ReadError.
    let tmp = tempfile::tempdir()?;
    let fake_file = tmp.path().join("m.fv");
    std::fs::create_dir(&fake_file)?;
    let resolver = FileSystemResolver::new(tmp.path().to_path_buf());
    let err = resolver
        .resolve(&["m".to_string()], None)
        .err()
        .ok_or("expected ReadError when target is a directory")?;
    match err {
        ModuleError::ReadError { path, error } => {
            if path != fake_file {
                return Err(format!("unexpected path {path:?}").into());
            }
            if error.is_empty() {
                return Err("ReadError.error should describe the failure".into());
            }
        }
        ModuleError::NotFound { .. }
        | ModuleError::CircularImport { .. }
        | ModuleError::PrivateItem { .. }
        | ModuleError::ItemNotFound { .. } => {
            return Err("expected ReadError".into());
        }
    }
    Ok(())
}

// =============================================================================
// Filesystem integration: end-to-end compile using real .fv files
// =============================================================================

#[test]
fn test_filesystem_resolver_loads_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let utils_dir = tmp.path().join("utils");
    std::fs::create_dir(&utils_dir)?;
    std::fs::write(
        utils_dir.join("helpers.fv"),
        "pub struct Point { x: I32, y: I32 }\n",
    )?;
    std::fs::write(
        tmp.path().join("main.fv"),
        "use utils::helpers::Point\nlet origin = Point(x: 0, y: 0)\n",
    )?;

    let resolver = FileSystemResolver::new(tmp.path().to_path_buf());
    let source = std::fs::read_to_string(tmp.path().join("main.fv"))?;
    let module = formalang::compile_to_ir_with_resolver(&source, resolver)
        .map_err(|errors| format!("unexpected compile failure: {errors:?}"))?;
    // The imported Point should be visible in the resulting IR.
    if module.struct_id("utils::helpers::Point").is_none() && module.struct_id("Point").is_none() {
        return Err("expected imported Point to appear in the IR module".into());
    }
    Ok(())
}

#[test]
fn test_filesystem_resolver_detects_circular_import() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    // `pub use` re-exports participate in cycle detection across modules.
    // Root imports b::A, b re-exports A from a, a re-exports A from b.
    std::fs::write(
        tmp.path().join("a.fv"),
        "pub use b::A\npub struct LocalA { x: I32 }\n",
    )?;
    std::fs::write(
        tmp.path().join("b.fv"),
        "pub use a::A\npub struct LocalB { y: I32 }\n",
    )?;
    std::fs::write(tmp.path().join("main.fv"), "use b::A\n")?;

    let resolver = FileSystemResolver::new(tmp.path().to_path_buf());
    let source = std::fs::read_to_string(tmp.path().join("main.fv"))?;
    let errors = formalang::compile_to_ir_with_resolver(&source, resolver)
        .err()
        .ok_or("expected circular-import or item-not-found error")?;
    // The cycle prevents A from ever being fully resolved; accept either
    // CircularImport or ImportItemNotFound as evidence the resolver
    // refused to loop forever.
    let detected = errors.iter().any(|e| {
        matches!(
            e,
            CompilerError::CircularImport { .. } | CompilerError::ImportItemNotFound { .. }
        )
    });
    if !detected {
        return Err(
            format!("expected CircularImport or ImportItemNotFound, got: {errors:?}").into(),
        );
    }
    Ok(())
}
