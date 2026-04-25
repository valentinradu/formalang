//! Tests for impl Trait for Type conformance (#6)
//!
//! Trait conformance is declared exclusively via `impl Trait for Type` blocks.

use formalang::CompilerError;

// =============================================================================
// Happy path: impl Trait for Type
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_impl_trait_for_struct_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Named {
    name: String
}
struct User {
    name: String,
    age: Number
}
impl Named for User {
    fn greet(self) -> String {
        self.name
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_inherent_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Counter {
    value: Number
}
impl Counter {
    fn increment(self) -> Number {
        self.value + 1
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_impl_blocks_same_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Named { name: String }
trait Aged { age: Number }

struct Person {
    name: String,
    age: Number
}

impl Named for Person {
    fn display_name(self) -> String {
        self.name
    }
}

impl Aged for Person {
    fn is_adult(self) -> Boolean {
        self.age >= 18
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_trait_for_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
trait Describable {
    fn describe(self) -> String
}
enum Status { Active, Inactive }
impl Describable for Status {
    fn describe(self) -> String {
        "status"
    }
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error: trait fields missing in struct (still checked via impl)
// =============================================================================

#[test]
fn test_impl_trait_missing_field_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
trait Named {
    name: String
}
struct Ghost {}
impl Named for Ghost {
    fn greet(self) -> String {
        "hi"
    }
}
"#;
    let errors = compile(source)
        .err()
        .ok_or("expected error for missing trait field")?;
    let has_field_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::MissingTraitField { field, .. } if field == "name"));
    if !has_field_error {
        return Err(format!("expected MissingTraitField error, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_impl_trait_field_type_mismatch_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
trait Named {
    name: String
}
struct Broken {
    name: Number
}
impl Named for Broken {
    fn greet(self) -> String {
        "hi"
    }
}
"#;
    let errors = compile(source)
        .err()
        .ok_or("expected error for field type mismatch")?;
    let has_mismatch = errors.iter().any(
        |e| matches!(e, CompilerError::TraitFieldTypeMismatch { field, .. } if field == "name"),
    );
    if !has_mismatch {
        return Err(format!("expected TraitFieldTypeMismatch error, got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// type_satisfies_trait_constraint works via impl blocks
// =============================================================================

#[test]
fn test_trait_constraint_satisfied_via_impl() -> Result<(), Box<dyn std::error::Error>> {
    // Tier-1 item E2: trait values are banned in argument positions;
    // the test now exercises the same `impl Printable for Doc` path
    // through a generic-bounded parameter, which is the canonical way
    // to take a trait-constrained value in FormaLang.
    //
    // Field-through-bound access (`item.label` with `item: T,
    // T: Printable`) is a separate inference limitation tracked
    // independently — the body here is a constant so the test stays
    // focused on impl/constraint resolution.
    let source = r#"
trait Printable {
    label: String
}
struct Doc {
    label: String
}
impl Printable for Doc {}

fn print_it<T: Printable>(item: T) -> String {
    "ok"
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}
