//! Tests for destructuring patterns in let bindings
//!
//! Tests array, struct, and enum destructuring as per documentation.

use formalang::compile;

// =============================================================================
// Array Destructuring Tests
// =============================================================================

#[test]
fn test_array_destructuring_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Basic positional destructuring: let [a, b] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [a, b] = items
    "#;
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_array_destructuring_with_rest() -> Result<(), Box<dyn std::error::Error>> {
    // Rest pattern: let [x, ...rest] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [x, ...rest] = items
    "#;
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_array_destructuring_skip_first() -> Result<(), Box<dyn std::error::Error>> {
    // Skip first element: let [_, second, ...] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [_, second, ...] = items
    "#;
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_array_destructuring_first_and_last() -> Result<(), Box<dyn std::error::Error>> {
    // Get first and last: let [first, ..., last] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [first, ..., last] = items
    "#;
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_err() {
        return Err("assertion failed".into());
    }
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
    if compile(source).is_ok() {
        return Err("assertion failed".into());
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
    // Parser should reject duplicate field names
    if compile(source).is_ok() {
        return Err("assertion failed".into());
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
    if compile(source).is_ok() {
        return Err("assertion failed".into());
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
    if compile(source).is_ok() {
        return Err("assertion failed".into());
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
    if compile(source).is_ok() {
        return Err("assertion failed".into());
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
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}
