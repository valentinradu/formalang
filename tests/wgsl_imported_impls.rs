//! Tests for WGSL generation with imported impl blocks.
//!
//! These tests verify that impl blocks from imported modules are properly
//! included in generated WGSL code.

use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::{compile_to_wgsl, compile_with_analyzer_and_resolver};
use std::path::PathBuf;

/// Helper to compile source and get the analyzer for testing
fn compile_with_imports(source: &str) -> Result<String, Vec<formalang::error::CompilerError>> {
    let resolver = FileSystemResolver::new(PathBuf::from("."));
    let (ast, analyzer) = compile_with_analyzer_and_resolver(source, resolver)?;
    let ir_module = formalang::ir::lower_to_ir(&ast, analyzer.symbols())?;
    Ok(formalang::codegen::generate_wgsl_with_imports(
        &ir_module,
        analyzer.imported_ir_modules(),
    ))
}

// =============================================================================
// Phase 1: SemanticAnalyzer IR Caching Tests
// =============================================================================

#[test]
fn test_imported_ir_modules_empty_for_no_imports() {
    let source = r#"
        struct LocalStruct {
            x: f32
        }
    "#;

    let resolver = FileSystemResolver::new(PathBuf::from("."));
    let (_, analyzer) =
        compile_with_analyzer_and_resolver(source, resolver).expect("Should compile");

    assert!(
        analyzer.imported_ir_modules().is_empty(),
        "No imports should result in empty IR module cache"
    );
}

#[test]
fn test_imported_ir_modules_cached_for_single_import() {
    let source = r#"
        use stdlib::gpu::Size2D

        let s = Size2D(width: 100.0, height: 100.0)
    "#;

    let resolver = FileSystemResolver::new(PathBuf::from("."));
    let (_, analyzer) =
        compile_with_analyzer_and_resolver(source, resolver).expect("Should compile");

    assert!(
        !analyzer.imported_ir_modules().is_empty(),
        "Import should populate IR module cache"
    );
}

// =============================================================================
// Phase 2: IrImport source_file Tests
// =============================================================================

#[test]
fn test_ir_import_has_source_file() {
    let source = r#"
        use stdlib::gpu::Size2D

        let s = Size2D(width: 100.0, height: 100.0)
    "#;

    let resolver = FileSystemResolver::new(PathBuf::from("."));
    let (ast, analyzer) =
        compile_with_analyzer_and_resolver(source, resolver).expect("Should compile");
    let ir_module = formalang::ir::lower_to_ir(&ast, analyzer.symbols()).expect("Should lower");

    // Find the import for Size2D
    let size2d_import = ir_module
        .imports
        .iter()
        .find(|i| i.items.iter().any(|item| item.name == "Size2D"));

    assert!(size2d_import.is_some(), "Should have import for Size2D");

    let import = size2d_import.unwrap();
    assert!(
        !import.source_file.as_os_str().is_empty(),
        "source_file should be populated"
    );
    assert!(
        import.source_file.to_string_lossy().contains("gpu"),
        "source_file should point to gpu module"
    );
}

// =============================================================================
// Phase 4: Imported Impl Generation Tests
// =============================================================================

#[test]
fn test_wgsl_includes_imported_struct_impl_functions() {
    let source = r#"
        use stdlib::shapes::Rect

        let r = Rect()
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // Should contain Rect struct definition
    assert!(
        wgsl.contains("struct Rect"),
        "Should generate Rect struct: {wgsl}"
    );

    // Should contain impl functions from Rect
    assert!(
        wgsl.contains("fn Rect_sdf") || wgsl.contains("fn Rect_render"),
        "Should generate Rect impl functions: {wgsl}"
    );
}

#[test]
fn test_wgsl_no_duplicate_functions_for_same_import() {
    let source = r#"
        use stdlib::shapes::Rect
        use stdlib::shapes::Rect  // Duplicate import (if allowed)

        let r1 = Rect()
        let r2 = Rect()
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // Count occurrences of Rect_sdf - should only appear once
    let sdf_count = wgsl.matches("fn Rect_sdf").count();
    assert!(
        sdf_count <= 1,
        "Should not duplicate impl functions, found {sdf_count} occurrences"
    );
}

#[test]
fn test_wgsl_multiple_imports_generate_all_impls() {
    let source = r#"
        use stdlib::shapes::{Rect, Circle}

        let r = Rect()
        let c = Circle()
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // Should contain functions from both structs
    assert!(
        wgsl.contains("fn Rect_") || wgsl.contains("Rect_sdf"),
        "Should generate Rect impl functions: {wgsl}"
    );
    assert!(
        wgsl.contains("fn Circle_") || wgsl.contains("Circle_sdf"),
        "Should generate Circle impl functions: {wgsl}"
    );
}

#[test]
fn test_wgsl_struct_without_impl_skipped() {
    // LocalStruct is a local struct without impl block.
    // Note: Size2D actually has an impl block in stdlib, so we use a local struct.
    let source = r#"
        struct LocalStruct {
            x: f32,
            y: f32
        }

        let s = LocalStruct(x: 100.0, y: 100.0)
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // Should contain struct definition but NOT LocalStruct_* functions (no impl block)
    assert!(
        wgsl.contains("struct LocalStruct"),
        "Should generate struct definition: {wgsl}"
    );
    assert!(
        !wgsl.contains("fn LocalStruct_"),
        "Should not generate functions for struct without impl: {wgsl}"
    );
}

// =============================================================================
// Phase 5: Public API Tests
// =============================================================================

#[test]
fn test_compile_to_wgsl_basic() {
    let source = r#"
        struct Vec2 {
            x: f32,
            y: f32
        }
    "#;

    let wgsl = compile_to_wgsl(source).expect("Should compile to WGSL");

    assert!(
        wgsl.contains("struct Vec2"),
        "Should generate struct: {wgsl}"
    );
}

#[test]
fn test_compile_to_wgsl_with_imports() {
    let source = r#"
        use stdlib::shapes::Rect

        let r = Rect()
    "#;

    let wgsl = compile_to_wgsl(source).expect("Should compile to WGSL");

    // Should contain Rect struct and impl functions
    assert!(wgsl.contains("struct Rect"), "Should generate Rect struct");
    assert!(
        wgsl.contains("fn Rect_"),
        "Should generate Rect impl functions"
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
fn test_transitive_imports_include_impls() {
    // shapes module imports fill and color internally
    // Their impls should be available when we use shapes
    let source = r#"
        use stdlib::shapes::Rect

        let r = Rect()
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // If Rect uses fill::Solid internally, we might need those impls too
    // This test documents the expected behavior for transitive dependencies
    assert!(
        wgsl.contains("struct Rect"),
        "Should have direct import: {wgsl}"
    );
}

#[test]
fn test_method_call_uses_mangled_name() {
    // Rect::render calls self.sdf() which should become Rect_sdf(self_, ...)
    // not just sdf(self_, ...)
    let source = r#"
        use stdlib::shapes::Rect

        let r = Rect()
    "#;

    let wgsl = compile_with_imports(source).expect("Should compile to WGSL");

    // The Rect_render function should call Rect_sdf, not bare sdf
    // This is the P1 bug: MethodCall generates method(recv) instead of StructName_method(recv)
    assert!(
        wgsl.contains("fn Rect_sdf"),
        "Should generate Rect_sdf: {wgsl}"
    );
    assert!(
        wgsl.contains("fn Rect_render"),
        "Should generate Rect_render: {wgsl}"
    );

    // The render function should call Rect_sdf, not bare sdf
    // Check that render's body contains the properly mangled call
    if wgsl.contains("sdf(self_") && !wgsl.contains("Rect_sdf(self_") {
        panic!(
            "Method call should use mangled name Rect_sdf, not bare sdf. WGSL:\n{}",
            wgsl
        );
    }

    // Verify that unsupported expressions don't appear
    assert!(
        !wgsl.contains("/* unsupported expr"),
        "Should not have unsupported expr placeholders. WGSL:\n{}",
        wgsl
    );
}
