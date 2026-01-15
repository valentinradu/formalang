//! Tests for module resolution using a mock resolver
//!
//! These tests exercise the module resolution paths in semantic analysis
//! without requiring actual filesystem access.

use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
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
                span: formalang::location::Span::default(),
            })
    }
}

// =============================================================================
// Basic Mock Resolver Tests
// =============================================================================

#[test]
fn test_mock_resolver_returns_module() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(vec!["utils".to_string()], "pub struct Helper { x: String }");

    let result = resolver.resolve(&["utils".to_string()], None);
    assert!(result.is_ok());
    let (source, path) = result.unwrap();
    assert!(source.contains("Helper"));
    assert!(path.to_string_lossy().contains("utils"));
}

#[test]
fn test_mock_resolver_not_found() {
    let resolver = MockModuleResolver::new();

    let result = resolver.resolve(&["nonexistent".to_string()], None);
    assert!(result.is_err());
    match result.unwrap_err() {
        ModuleError::NotFound { path, .. } => {
            assert_eq!(path, vec!["nonexistent".to_string()]);
        }
        _ => panic!("Expected NotFound error"),
    }
}

#[test]
fn test_mock_resolver_returns_configured_error() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["broken".to_string()],
        ModuleError::ReadError {
            path: PathBuf::from("broken.forma"),
            error: "Permission denied".to_string(),
            span: formalang::location::Span::default(),
        },
    );

    let result = resolver.resolve(&["broken".to_string()], None);
    assert!(result.is_err());
    match result.unwrap_err() {
        ModuleError::ReadError { error, .. } => {
            assert!(error.contains("Permission denied"));
        }
        _ => panic!("Expected ReadError"),
    }
}

#[test]
fn test_mock_resolver_multi_segment_path() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );

    let result = resolver.resolve(&["std".to_string(), "collections".to_string()], None);
    assert!(result.is_ok());
}

// =============================================================================
// ModuleError Tests
// =============================================================================

#[test]
fn test_module_error_not_found_fields() {
    let error = ModuleError::NotFound {
        path: vec!["foo".to_string(), "bar".to_string()],
        searched_paths: vec![PathBuf::from("/a/b.forma"), PathBuf::from("/a/b/c.forma")],
        span: formalang::location::Span::default(),
    };

    match error {
        ModuleError::NotFound {
            path,
            searched_paths,
            ..
        } => {
            assert_eq!(path.len(), 2);
            assert_eq!(searched_paths.len(), 2);
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_module_error_circular_import() {
    let error = ModuleError::CircularImport {
        cycle: vec!["a".to_string(), "b".to_string(), "a".to_string()],
        span: formalang::location::Span::default(),
    };

    match error {
        ModuleError::CircularImport { cycle, .. } => {
            assert_eq!(cycle.len(), 3);
            assert_eq!(cycle[0], "a");
            assert_eq!(cycle[2], "a");
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_module_error_read_error() {
    let error = ModuleError::ReadError {
        path: PathBuf::from("/some/path.forma"),
        error: "File not accessible".to_string(),
        span: formalang::location::Span::default(),
    };

    match error {
        ModuleError::ReadError { path, error, .. } => {
            assert!(path.to_string_lossy().contains("path.forma"));
            assert!(error.contains("accessible"));
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_module_error_private_item() {
    let error = ModuleError::PrivateItem {
        item: "InternalHelper".to_string(),
        module: "utils".to_string(),
        span: formalang::location::Span::default(),
    };

    match error {
        ModuleError::PrivateItem { item, module, .. } => {
            assert_eq!(item, "InternalHelper");
            assert_eq!(module, "utils");
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_module_error_item_not_found() {
    let error = ModuleError::ItemNotFound {
        item: "Missing".to_string(),
        module: "utils".to_string(),
        available: vec!["Helper".to_string(), "Utils".to_string()],
        span: formalang::location::Span::default(),
    };

    match error {
        ModuleError::ItemNotFound {
            item,
            module,
            available,
            ..
        } => {
            assert_eq!(item, "Missing");
            assert_eq!(module, "utils");
            assert_eq!(available.len(), 2);
        }
        _ => panic!("Wrong error type"),
    }
}

#[test]
fn test_module_error_equality() {
    let error1 = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
        span: formalang::location::Span::default(),
    };
    let error2 = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
        span: formalang::location::Span::default(),
    };

    assert_eq!(error1, error2);
}

#[test]
fn test_module_error_debug() {
    let error = ModuleError::NotFound {
        path: vec!["test".to_string()],
        searched_paths: vec![],
        span: formalang::location::Span::default(),
    };

    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("NotFound"));
    assert!(debug_str.contains("test"));
}

#[test]
fn test_module_error_clone() {
    let error = ModuleError::ReadError {
        path: PathBuf::from("test.forma"),
        error: "Test error".to_string(),
        span: formalang::location::Span::default(),
    };

    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// =============================================================================
// Semantic Analyzer Integration Tests with Mock Resolver
// =============================================================================

use formalang::lexer::Lexer;
use formalang::parser;
use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::semantic::SemanticAnalyzer;

fn analyze_with_mock(
    source: &str,
    resolver: MockModuleResolver,
) -> Result<(), Vec<formalang::error::CompilerError>> {
    let tokens = Lexer::tokenize_all(source);
    let file = parser::parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| formalang::error::CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    let mut analyzer = SemanticAnalyzer::new_with_file(resolver, PathBuf::from("main.forma"));
    analyzer.analyze(&file)
}

fn analyze_with_filesystem(
    source: &str,
    root_dir: PathBuf,
) -> Result<(), Vec<formalang::error::CompilerError>> {
    let tokens = Lexer::tokenize_all(source);
    let file = parser::parse_file_with_source(&tokens, source).map_err(|errors| {
        errors
            .into_iter()
            .map(|(msg, span)| formalang::error::CompilerError::ParseError { message: msg, span })
            .collect::<Vec<_>>()
    })?;
    let resolver = FileSystemResolver::new(root_dir);
    let mut analyzer = SemanticAnalyzer::new_with_file(resolver, PathBuf::from("main.forma"));
    analyzer.analyze(&file)
}

#[test]
fn test_semantic_use_module_not_found() {
    let resolver = MockModuleResolver::new();
    let source = r#"
use nonexistent::Helper
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::ModuleNotFound { .. })));
}

#[test]
fn test_semantic_use_module_read_error() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["broken".to_string()],
        ModuleError::ReadError {
            path: PathBuf::from("broken.forma"),
            error: "Permission denied".to_string(),
            span: formalang::location::Span::default(),
        },
    );
    let source = r#"
use broken::Helper
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::ModuleReadError { .. })));
}

#[test]
fn test_semantic_use_circular_import() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["circular".to_string()],
        ModuleError::CircularImport {
            cycle: vec![
                "main".to_string(),
                "circular".to_string(),
                "main".to_string(),
            ],
            span: formalang::location::Span::default(),
        },
    );
    let source = r#"
use circular::Helper
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::CircularImport { .. })));
}

#[test]
fn test_semantic_use_private_item() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["utils".to_string()],
        ModuleError::PrivateItem {
            item: "InternalHelper".to_string(),
            module: "utils".to_string(),
            span: formalang::location::Span::default(),
        },
    );
    let source = r#"
use utils::InternalHelper
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::PrivateImport { .. })));
}

#[test]
fn test_semantic_use_item_not_found() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_error(
        vec!["utils".to_string()],
        ModuleError::ItemNotFound {
            item: "Missing".to_string(),
            module: "utils".to_string(),
            available: vec!["Helper".to_string(), "Utils".to_string()],
            span: formalang::location::Span::default(),
        },
    );
    let source = r#"
use utils::Missing
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(
        e,
        formalang::error::CompilerError::ImportItemNotFound { .. }
    )));
}

#[test]
fn test_semantic_use_module_success_single_item() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );
    let source = r#"
use utils::Helper
struct Main {
    helper: Helper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_module_success_multiple_items() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: Number
}
"#,
    );
    let source = r#"
use utils::{Helper, Utils}
struct Main {
    helper: Helper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_module_parse_error() {
    let mut resolver = MockModuleResolver::new();
    // Add module with invalid syntax
    resolver.add_module(
        vec!["broken".to_string()],
        "pub struct Helper { name String }", // missing colon
    );
    let source = r#"
use broken::Helper
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::ParseError { .. })));
}

#[test]
fn test_semantic_use_nested_module_path() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        "pub struct List { items: [String] }",
    );
    let source = r#"
use std::collections::List
struct Main {
    items: List
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_module_with_trait() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["traits".to_string()],
        r#"
pub trait Named {
    name: String
}
"#,
    );
    let source = r#"
use traits::Named
trait LocalNamed: Named {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_module_with_enum() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["types".to_string()],
        "pub enum Status { Active, Inactive }",
    );
    let source = r#"
use types::Status
struct Item {
    status: Status
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_multiple_use_statements() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        "pub struct Helper { name: String }",
    );
    resolver.add_module(
        vec!["types".to_string()],
        "pub struct Value { amount: Number }",
    );
    let source = r#"
use utils::Helper
use types::Value
struct Main {
    helper: Helper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_same_module_twice() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: Number
}
"#,
    );
    let source = r#"
use utils::Helper
use utils::Utils
struct Main {
    helper: Helper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

// =============================================================================
// Glob Import Tests
// =============================================================================

#[test]
fn test_semantic_use_glob_import_all_public() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct Helper {
    name: String
}
pub struct Utils {
    value: Number
}
pub enum Status {
    Active,
    Inactive
}
pub trait Named {
    name: String
}
"#,
    );
    let source = r#"
use utils::*
struct Main {
    helper: Helper,
    utils: Utils,
    status: Status
}
trait LocalNamed: Named {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_import_nested_path() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["std".to_string(), "collections".to_string()],
        r#"
pub struct List {
    items: [String]
}
pub struct Map {
    keys: [String]
}
"#,
    );
    let source = r#"
use std::collections::*
struct Main {
    list: List,
    map: Map
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_import_only_public() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct PublicHelper {
    name: String
}
struct PrivateHelper {
    secret: String
}
"#,
    );
    // Glob import should only import PublicHelper, not PrivateHelper
    let source = r#"
use utils::*
struct Main {
    helper: PublicHelper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_import_private_not_accessible() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct PublicHelper {
    name: String
}
struct PrivateHelper {
    secret: String
}
"#,
    );
    // Trying to use PrivateHelper after glob import should fail
    let source = r#"
use utils::*
struct Main {
    helper: PrivateHelper
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err(), "Expected error for private type usage");
}

#[test]
fn test_semantic_use_glob_with_other_imports() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["utils".to_string()],
        r#"
pub struct Helper {
    name: String
}
"#,
    );
    resolver.add_module(
        vec!["types".to_string()],
        r#"
pub struct Value {
    amount: Number
}
pub struct Item {
    id: String
}
"#,
    );
    // Mix glob import with specific imports
    let source = r#"
use utils::Helper
use types::*
struct Main {
    helper: Helper,
    value: Value,
    item: Item
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_module_not_found() {
    let resolver = MockModuleResolver::new();
    let source = r#"
use nonexistent::*
struct Main {}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, formalang::error::CompilerError::ModuleNotFound { .. })));
}

#[test]
fn test_semantic_use_glob_empty_module() {
    let mut resolver = MockModuleResolver::new();
    // Module with no public symbols
    resolver.add_module(
        vec!["empty".to_string()],
        r#"
struct PrivateOnly {
    secret: String
}
"#,
    );
    // Glob import from module with no public symbols should succeed (imports nothing)
    let source = r#"
use empty::*
struct Main {
    name: String
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_with_let_bindings() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["constants".to_string()],
        r#"
pub let MAX_SIZE: Number = 100
pub let DEFAULT_NAME: String = "unnamed"
"#,
    );
    let source = r#"
use constants::*
struct Config {
    size: Number
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

#[test]
fn test_semantic_use_glob_with_nested_module() {
    let mut resolver = MockModuleResolver::new();
    resolver.add_module(
        vec!["parent".to_string()],
        r#"
pub struct ParentStruct {
    name: String
}
pub mod child {
    pub struct ChildStruct {
        value: Number
    }
}
"#,
    );
    // Glob import should import ParentStruct and child module, but not child's contents directly
    let source = r#"
use parent::*
struct Main {
    parent: ParentStruct
}
"#;
    let result = analyze_with_mock(source, resolver);
    assert!(result.is_ok(), "Expected success, got: {:?}", result);
}

// =============================================================================
// Example Website Integration Test
// =============================================================================

#[test]
#[ignore = "stdlib uses WGSL types like vec2 that need parser support"]
fn test_stdlib_compiles_alone() {
    // Load stdlib from filesystem
    let stdlib_source =
        std::fs::read_to_string("stdlib.fv").expect("Failed to read docs/user/stdlib.fv");

    let root_dir = PathBuf::from(".");
    let result = analyze_with_filesystem(&stdlib_source, root_dir);
    assert!(result.is_ok(), "Stdlib should compile: {:?}", result.err());
}

#[test]
fn test_self_only() {
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

    let result = analyze_with_mock(source, resolver);
    assert!(
        result.is_ok(),
        "Self-only test should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_simple_self_with_stdlib() {
    // self references are only valid in impl functions
    let simple_source = r##"
use stdlib::*

pub struct Modal: View {
  isOpen: Boolean = false,
  title: String = "test",
  mount body: View
}

impl Modal {
  fn getTitle() -> String {
    self.title
  }
}
"##;

    let root_dir = PathBuf::from(".");
    let result = analyze_with_filesystem(simple_source, root_dir);
    assert!(
        result.is_ok(),
        "Simple self with stdlib should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_minimal_self_reference() {
    // self references are only valid in impl functions
    let minimal_source = r#"
use stdlib::*

pub struct Modal: View {
  isOpen: Boolean = false,
  mount body: View
}

impl Modal {
  fn isModalOpen() -> Boolean {
    self.isOpen
  }
}
"#;

    let root_dir = PathBuf::from(".");
    let result = analyze_with_filesystem(minimal_source, root_dir);
    assert!(
        result.is_ok(),
        "Minimal self reference should compile: {:?}",
        result.err()
    );
}

#[test]
fn test_path_literal_parsing() {
    let source = r#"
struct Test {
  p: Path = /icons/lightning.svg
}
"#;
    let result = analyze_with_mock(source, MockModuleResolver::new());
    assert!(
        result.is_ok(),
        "Path literal should parse: {:?}",
        result.err()
    );
}
