//! Tests for destructuring patterns in let bindings
//!
//! Tests array, struct, and enum destructuring as per documentation.

use formalang::CompilerError;

// =============================================================================
// Array Destructuring Tests
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_array_destructuring_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Basic positional destructuring: let [a, b] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [a, b] = items
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_array_destructuring_with_rest() -> Result<(), Box<dyn std::error::Error>> {
    // Rest pattern: let [x, ...rest] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [x, ...rest] = items
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_array_destructuring_skip_first() -> Result<(), Box<dyn std::error::Error>> {
    // Skip first element: let [_, second, ...] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [_, second, ...] = items
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_array_destructuring_first_and_last() -> Result<(), Box<dyn std::error::Error>> {
    // Get first and last: let [first, ..., last] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [first, ..., last] = items
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct Destructuring Tests
// =============================================================================

#[test]
fn test_struct_destructuring_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Basic struct destructuring: let {name, age} = user
    let source = r#"
        struct User { name: String, age: Number }
        pub let user = User(name: "Alice", age: 30)
        pub let {name, age} = user
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_destructuring_with_rename() -> Result<(), Box<dyn std::error::Error>> {
    // Rename during destructuring: let {name as username} = user
    let source = r#"
        struct User { name: String, age: Number }
        pub let user = User(name: "Alice", age: 30)
        pub let {name as username} = user
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_destructuring_partial() -> Result<(), Box<dyn std::error::Error>> {
    // Partial destructuring: let {name} = user (only extract some fields)
    let source = r#"
        struct User { name: String, age: Number }
        pub let user = User(name: "Alice", age: 30)
        pub let {name} = user
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum/Tuple Destructuring Tests
// =============================================================================

#[test]
fn test_enum_destructuring_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Enum destructuring: let (permissions, articles) = account
    let source = r#"
        enum AccountType { admin, user(permissions: [String], articles: [String]) }
        pub let account = AccountType.user(permissions: ["read", "write"], articles: ["article1", "article2"])
        pub let (permissions, articles) = account
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_destructuring_nested() -> Result<(), Box<dyn std::error::Error>> {
    // Nested destructuring with enums: let ([firstPerm, ...], articles) = account
    let source = r#"
        enum AccountType { admin, user(permissions: [String], articles: [String]) }
        pub let account = AccountType.user(permissions: ["read", "write"], articles: ["article1", "article2"])
        pub let ([firstPerm, ...], articles) = account
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error Cases - Duplicate Bindings
// =============================================================================

#[test]
fn test_error_duplicate_binding_in_array() -> Result<(), Box<dyn std::error::Error>> {
    // Can't have duplicate bindings in array destructuring
    let source = r#"
        pub let items = ["a", "b"]
        pub let [a, a] = items
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors.iter().any(
        |e| matches!(e, CompilerError::DuplicateDefinition { name, .. } if name.starts_with('a')),
    ) {
        return Err(format!("Expected DuplicateDefinition for 'a': {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_binding_in_struct() -> Result<(), Box<dyn std::error::Error>> {
    // Can't have duplicate bindings in struct destructuring
    let source = r"
        struct Point { x: Number, y: Number }
        pub let p = Point(x: 1, y: 2)
        pub let {x, x} = p
    ";
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors.iter().any(
        |e| matches!(e, CompilerError::DuplicateDefinition { name, .. } if name.starts_with('x')),
    ) {
        return Err(format!("Expected DuplicateDefinition for 'x': {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_binding_across_patterns() -> Result<(), Box<dyn std::error::Error>> {
    // Can't redefine an existing binding
    let source = r#"
        pub let items = ["a", "b"]
        pub let [first, second] = items
        pub let first = "other"
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors.iter().any(|e| matches!(e, CompilerError::DuplicateDefinition { name, .. } if name.starts_with("first"))) {
        return Err(format!("Expected DuplicateDefinition for 'first': {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Error Cases - Type Mismatch
// =============================================================================

#[test]
fn test_error_array_destructuring_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Can't destructure non-array as array
    let source = r#"
        pub let value = "not an array"
        pub let [a, b] = value
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ArrayDestructuringNotArray { .. }))
    {
        return Err(format!("Expected ArrayDestructuringNotArray: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_error_struct_destructuring_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Can't destructure non-struct as struct
    let source = r#"
        pub let value = "not a struct"
        pub let {name} = value
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::StructDestructuringNotStruct { .. }))
    {
        return Err(format!("Expected StructDestructuringNotStruct: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_error_struct_destructuring_missing_field() -> Result<(), Box<dyn std::error::Error>> {
    // Can't destructure non-existent field
    let source = r#"
        struct User { name: String }
        pub let user = User(name: "Alice")
        pub let {name, age} = user
    "#;
    let errors = compile(source).err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::UnknownField { field, .. } if field == "age"))
    {
        return Err(format!("Expected UnknownField for 'age': {errors:?}").into());
    }
    Ok(())
}
