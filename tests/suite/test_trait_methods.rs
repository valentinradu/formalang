//! Tests for trait method signatures (#2)
//!
//! Traits can declare method signatures (no default bodies).
//! Every `impl Trait for Type` must provide all declared methods with matching signatures.

use formalang::CompilerError;

// =============================================================================
// Happy path: trait with method signatures
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_trait_with_method_signatures() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Drawable {
    fn draw(self) -> Boolean
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_fields_and_methods() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Shape {
    color: String
    fn area(self) -> I32
    fn perimeter(self) -> I32
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_trait_provides_all_methods() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Drawable {
    fn draw(self) -> Boolean
    fn visible(self) -> Boolean
}
struct Circle {
    radius: I32
}
impl Drawable for Circle {
    fn draw(self) -> Boolean {
        true
    }
    fn visible(self) -> Boolean {
        self.radius > 0
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_method_with_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Resizable {
    fn resize(self, factor: I32) -> Boolean
}
struct Box {
    width: I32,
    height: I32
}
impl Resizable for Box {
    fn resize(self, factor: I32) -> Boolean {
        true
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_composed_trait_only_requires_directly_declared_methods(
) -> Result<(), Box<dyn std::error::Error>> {
    // Audit #16: positive test locking in the composed-trait stance.
    //
    // `trait Extended: Base` declares Extended composes Base, but
    // `impl Extended for T` only requires T to implement the methods
    // *directly* declared on Extended. Inherited methods from Base are
    // a separate concern: if the user wants Base's methods on T too,
    // they must add an explicit `impl Base for T` block.
    //
    // This is the design choice fixed by audit #16. The alternative
    // (auto-implying `impl Base for T` from `impl Extended for T`)
    // would require collecting required methods transitively in
    // `collect_all_trait_methods` and emitting an extra
    // MissingTraitMethod for inherited methods.
    let only_extended = r#"
        trait Base { fn id(self) -> I32 }
        trait Extended: Base { fn name(self) -> String }
        struct Item { value: I32 }
        impl Extended for Item {
            fn name(self) -> String { "item" }
        }
    "#;
    compile(only_extended).map_err(|e| {
        format!(
            "design intent: `impl Extended` without `impl Base` should compile cleanly, got: {e:?}"
        )
    })?;

    let both_impls = r#"
        trait Base { fn id(self) -> I32 }
        trait Extended: Base { fn name(self) -> String }
        struct Item { value: I32 }
        impl Base for Item {
            fn id(self) -> I32 { self.value }
        }
        impl Extended for Item {
            fn name(self) -> String { "item" }
        }
    "#;
    compile(both_impls).map_err(|e| {
        format!("expected both `impl Base` + `impl Extended` to compile, got: {e:?}")
    })?;
    Ok(())
}

#[test]
fn test_trait_inheritance_includes_methods() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
trait Base {
    fn id(self) -> I32
}
trait Extended: Base {
    fn name(self) -> String
}
struct Item {
    value: I32
}
impl Base for Item {
    fn id(self) -> I32 {
        self.value
    }
}
impl Extended for Item {
    fn name(self) -> String {
        "item"
    }
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error: impl Trait for Type missing a required method
// =============================================================================

#[test]
fn test_missing_trait_method_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Drawable {
    fn draw(self) -> Boolean
    fn visible(self) -> Boolean
}
struct Square {
    side: I32
}
impl Drawable for Square {
    fn draw(self) -> Boolean {
        true
    }
}
";
    let errors = compile(source)
        .err()
        .ok_or("expected error for missing trait method")?;
    let has_error = errors.iter().any(|e| {
        matches!(e, CompilerError::MissingTraitMethod { method, trait_name, .. }
            if method == "visible" && trait_name == "Drawable")
    });
    if !has_error {
        return Err(format!("expected MissingTraitMethod 'visible', got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_missing_all_trait_methods_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Drawable {
    fn draw(self) -> Boolean
}
struct Square {
    side: I32
}
impl Drawable for Square {}
";
    let errors = compile(source)
        .err()
        .ok_or("expected error for missing all trait methods")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::MissingTraitMethod { method, .. } if method == "draw"));
    if !has_error {
        return Err(format!("expected MissingTraitMethod 'draw', got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Error: impl Trait method signature mismatch
// =============================================================================

#[test]
fn test_trait_method_return_type_mismatch_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Drawable {
    fn draw(self) -> Boolean
}
struct Circle {
    radius: I32
}
impl Drawable for Circle {
    fn draw(self) -> I32 {
        42
    }
}
";
    let errors = compile(source)
        .err()
        .ok_or("expected error for method signature mismatch")?;
    let has_error = errors.iter().any(|e| {
        matches!(e, CompilerError::TraitMethodSignatureMismatch { method, trait_name, .. }
            if method == "draw" && trait_name == "Drawable")
    });
    if !has_error {
        return Err(
            format!("expected TraitMethodSignatureMismatch 'draw', got: {errors:?}").into(),
        );
    }
    Ok(())
}

#[test]
fn test_trait_method_param_count_mismatch_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Resizable {
    fn resize(self, factor: I32) -> Boolean
}
struct Rect {
    w: I32
}
impl Resizable for Rect {
    fn resize(self) -> Boolean {
        true
    }
}
";
    let errors = compile(source)
        .err()
        .ok_or("expected error for param count mismatch")?;
    let has_error = errors.iter().any(|e| {
        matches!(e, CompilerError::TraitMethodSignatureMismatch { method, .. } if method == "resize")
    });
    if !has_error {
        return Err(
            format!("expected TraitMethodSignatureMismatch 'resize', got: {errors:?}").into(),
        );
    }
    Ok(())
}

// =============================================================================
// Trait fields + methods: both checked
// =============================================================================

#[test]
fn test_trait_fields_and_methods_both_checked() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Named {
    name: String
    fn greet(self) -> String
}
struct Person {
    name: String
}
impl Named for Person {
    fn greet(self) -> String {
        self.name
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Trait with no methods (fields only) still works
// =============================================================================

#[test]
fn test_trait_with_no_methods_still_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Labeled {
    label: String
}
struct Tag {
    label: String
}
impl Labeled for Tag {}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}
