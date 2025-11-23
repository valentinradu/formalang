//! Tests for destructuring patterns in let bindings
//!
//! Tests array, struct, and enum destructuring as per documentation.

use formalang::compile;

// =============================================================================
// Array Destructuring Tests
// =============================================================================

#[test]
fn test_array_destructuring_simple() {
    // Basic positional destructuring: let [a, b] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [a, b] = items
    "#;
    assert!(compile(source).is_ok());
}

#[test]
fn test_array_destructuring_with_rest() {
    // Rest pattern: let [x, ...rest] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [x, ...rest] = items
    "#;
    assert!(compile(source).is_ok());
}

#[test]
fn test_array_destructuring_skip_first() {
    // Skip first element: let [_, second, ...] = items
    let source = r#"
        pub let items = ["first", "second", "third"]
        pub let [_, second, ...] = items
    "#;
    assert!(compile(source).is_ok());
}

#[test]
fn test_array_destructuring_first_and_last() {
    // Get first and last: let [first, ..., last] = items
    let source = r#"
        pub let items = ["first", "second", "third", "fourth"]
        pub let [first, ..., last] = items
    "#;
    assert!(compile(source).is_ok());
}

// =============================================================================
// Struct Destructuring Tests (require full semantic support - currently ignored)
// =============================================================================

#[test]
#[ignore = "requires semantic support for struct destructuring bindings"]
fn test_struct_destructuring_simple() {
    // Basic struct destructuring: let {name, age} = user
    let source = r#"
        struct User {
            name: String
            age: Number
        }
        pub let user = User(name: "Alice", age: 30)
        pub let {name, age} = user
    "#;
    assert!(compile(source).is_ok());
}

#[test]
#[ignore = "requires semantic support for struct destructuring bindings"]
fn test_struct_destructuring_with_rename() {
    // Rename during destructuring: let {name as username} = user
    let source = r#"
        struct User {
            name: String
            age: Number
        }
        pub let user = User(name: "Alice", age: 30)
        pub let {name as username} = user
    "#;
    assert!(compile(source).is_ok());
}

#[test]
#[ignore = "requires semantic support for struct destructuring bindings"]
fn test_struct_destructuring_partial() {
    // Partial destructuring: let {name} = user (only extract some fields)
    let source = r#"
        struct User {
            name: String
            age: Number
        }
        pub let user = User(name: "Alice", age: 30)
        pub let {name} = user
    "#;
    assert!(compile(source).is_ok());
}

// =============================================================================
// Enum Destructuring Tests (require full semantic support - currently ignored)
// =============================================================================

#[test]
#[ignore = "requires semantic support for enum destructuring bindings"]
fn test_enum_destructuring_simple() {
    // Enum destructuring: let (permissions, articles) = account
    let source = r#"
        enum AccountType {
            admin
            user(permissions: [String], articles: [String])
        }
        pub let account = AccountType.user(
            permissions: ["read", "write"],
            articles: ["article1", "article2"]
        )
        pub let (permissions, articles) = account
    "#;
    assert!(compile(source).is_ok());
}

#[test]
#[ignore = "requires semantic support for enum destructuring bindings"]
fn test_enum_destructuring_nested() {
    // Nested destructuring with enums: let ([firstPerm, ...], articles) = account
    let source = r#"
        enum AccountType {
            admin
            user(permissions: [String], articles: [String])
        }
        pub let account = AccountType.user(
            permissions: ["read", "write"],
            articles: ["article1", "article2"]
        )
        pub let ([firstPerm, ...], articles) = account
    "#;
    assert!(compile(source).is_ok());
}

// =============================================================================
// Error Cases (require semantic validation - currently ignored)
// =============================================================================

#[test]
#[ignore = "requires semantic validation of destructuring patterns"]
fn test_error_array_destructuring_type_mismatch() {
    // Can't destructure non-array as array
    let source = r#"
        pub let value = "not an array"
        pub let [a, b] = value
    "#;
    assert!(compile(source).is_err());
}

#[test]
#[ignore = "requires semantic validation of destructuring patterns"]
fn test_error_struct_destructuring_type_mismatch() {
    // Can't destructure non-struct as struct
    let source = r#"
        pub let value = "not a struct"
        pub let {name} = value
    "#;
    assert!(compile(source).is_err());
}

#[test]
#[ignore = "requires semantic validation of destructuring patterns"]
fn test_error_struct_destructuring_missing_field() {
    // Can't destructure non-existent field
    let source = r#"
        struct User {
            name: String
        }
        pub let user = User(name: "Alice")
        pub let {name, age} = user
    "#;
    assert!(compile(source).is_err());
}
