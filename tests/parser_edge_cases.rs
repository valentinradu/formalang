//! Parser edge case tests
//!
//! Tests for parser edge cases and AST node coverage

use formalang::compile;

// =============================================================================
// Parse Function Tests
// =============================================================================

#[test]
fn test_compile_simple() {
    let source = "struct A { }";
    let result = compile(source);
    assert!(result.is_ok(), "Compile simple: {:?}", result.err());
}

#[test]
fn test_compile_empty() {
    let source = "";
    let result = compile(source);
    assert!(result.is_ok(), "Compile empty: {:?}", result.err());
}

#[test]
fn test_compile_whitespace() {
    let source = "   \n\n   ";
    let result = compile(source);
    assert!(result.is_ok(), "Compile whitespace: {:?}", result.err());
}

#[test]
fn test_compile_comments() {
    let source = r#"
        // Single line comment
        struct A { }
        /* Block comment */
        struct B { }
    "#;
    let result = compile(source);
    assert!(result.is_ok(), "Compile comments: {:?}", result.err());
}

// =============================================================================
// Expression Tests
// =============================================================================

#[test]
fn test_nil_literal() {
    let source = r#"
        struct A { x: String? }
        impl A { nil }
    "#;
    assert!(
        compile(source).is_ok(),
        "Nil literal: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_array_literal_empty() {
    let source = r#"
        struct A { items: [String] }
        impl A { [] }
    "#;
    assert!(
        compile(source).is_ok(),
        "Empty array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_array_literal_single() {
    let source = r#"
        struct A { items: [String] }
        impl A { ["one"] }
    "#;
    assert!(
        compile(source).is_ok(),
        "Single item array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_array_literal_many() {
    let source = r#"
        struct A { items: [Number] }
        impl A { [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }
    "#;
    assert!(
        compile(source).is_ok(),
        "Many items array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_negative_number() {
    let source = r#"
        struct A { x: Number }
        impl A { -42 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Negative number: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_decimal_number() {
    let source = r#"
        struct A { x: Number }
        impl A { 3.14159 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Decimal number: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_negative_decimal() {
    let source = r#"
        struct A { x: Number }
        impl A { -0.5 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Negative decimal: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_string_with_escapes() {
    let source = r#"
        struct A { x: String }
        impl A { "hello\nworld\t!" }
    "#;
    assert!(
        compile(source).is_ok(),
        "String with escapes: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_string_with_quotes() {
    let source = r#"
        struct A { x: String }
        impl A { "say \"hello\"" }
    "#;
    assert!(
        compile(source).is_ok(),
        "String with quotes: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Operator Precedence Tests
// =============================================================================

#[test]
fn test_arithmetic_precedence() {
    let source = r#"
        struct A { x: Number }
        impl A { 1 + 2 * 3 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Arithmetic precedence: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_comparison_chain() {
    let source = r#"
        struct A { x: Boolean }
        impl A { 1 < 2 && 2 < 3 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Comparison chain: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_logical_precedence() {
    let source = r#"
        struct A { x: Boolean }
        impl A { true || false && true }
    "#;
    assert!(
        compile(source).is_ok(),
        "Logical precedence: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_parenthesized_expression() {
    let source = r#"
        struct A { x: Number }
        impl A { (1 + 2) * 3 }
    "#;
    assert!(
        compile(source).is_ok(),
        "Parenthesized: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_nested_parentheses() {
    let source = r#"
        struct A { x: Number }
        impl A { ((1 + 2) * (3 + 4)) }
    "#;
    assert!(
        compile(source).is_ok(),
        "Nested parentheses: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Complex Control Flow Tests
// =============================================================================

#[test]
fn test_if_without_else() {
    let source = r#"
        struct A { x: String? }
        impl A { if true { "yes" } }
    "#;
    assert!(
        compile(source).is_ok(),
        "If without else: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_simple_else() {
    let source = r#"
        struct A { x: String }
        impl A {
            if false { "a" } else { "b" }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Simple else: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_for_with_if() {
    let source = r#"
        struct A { x: [String] }
        impl A {
            for x in ["a", "b", "c"] {
                if true { x } else { "default" }
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "For with if: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_let_chain() {
    let source = r#"
        struct A { x: Number }
        impl A {
            let a = 1
            let b = 2
            let c = 3
            a
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Let chain: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Field Access Tests
// =============================================================================

#[test]
fn test_field_access_simple() {
    let source = r#"
        struct Inner { value: String }
        struct Outer { inner: Inner }
        impl Outer { inner }
    "#;
    assert!(
        compile(source).is_ok(),
        "Field access simple: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Enum and Match Tests
// =============================================================================

#[test]
fn test_enum_single_variant() {
    let source = "enum Unit { unit }";
    assert!(
        compile(source).is_ok(),
        "Enum single variant: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_enum_many_variants() {
    let source = r#"
        enum Colors {
            red,
            orange,
            yellow,
            green,
            blue,
            indigo,
            violet
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Enum many variants: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_match_exhaustive() {
    let source = r#"
        enum AB { a, b }
        struct Handler { x: AB }
        impl Handler {
            match AB.a {
                .a: "first",
                .b: "second"
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Match exhaustive: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_empty_nested_modules() {
    let source = r#"
        module a {
            module b {
                module c { }
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Empty nested modules: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_sibling_modules() {
    let source = r#"
        module a { struct A { } }
        module b { struct B { } }
        module c { struct C { } }
    "#;
    assert!(
        compile(source).is_ok(),
        "Sibling modules: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Trait Tests
// =============================================================================

#[test]
fn test_trait_single_field() {
    let source = "trait Single { field: String }";
    assert!(
        compile(source).is_ok(),
        "Trait single field: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_trait_many_fields() {
    let source = r#"
        trait Many {
            a: String,
            b: Number,
            c: Boolean,
            d: [String],
            e: String?
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Trait many fields: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_trait_with_generics() {
    let source = "trait Container<T> { item: T }";
    assert!(
        compile(source).is_ok(),
        "Trait with generics: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_trait_inheritance() {
    let source = r#"
        trait Base { base: String }
        trait Derived: Base { derived: Number }
        struct Impl: Derived { base: String, derived: Number }
    "#;
    assert!(
        compile(source).is_ok(),
        "Trait inheritance: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Struct Tests
// =============================================================================

#[test]
fn test_struct_single_field() {
    let source = "struct Single { field: String }";
    assert!(
        compile(source).is_ok(),
        "Struct single field: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_many_fields() {
    let source = r#"
        struct Many {
            a: String,
            b: Number,
            c: Boolean,
            d: [String],
            e: String?,
            f: [String: Number]
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct many fields: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_with_modifiers() {
    let source = r#"
        struct Full {
            mut count: Number,
            @mount content: String,
            optional: String?,
            default: Number = 0
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct with modifiers: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_with_generics() {
    let source = "struct Box<T> { value: T }";
    assert!(
        compile(source).is_ok(),
        "Struct with generics: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_multiple_generic_params() {
    let source = r#"
        struct Map<K, V> {
            keys: [K],
            values: [V]
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct multiple generic params: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Impl Tests
// =============================================================================

#[test]
fn test_impl_empty_struct() {
    let source = r#"
        struct Empty { }
        impl Empty { "empty" }
    "#;
    assert!(
        compile(source).is_ok(),
        "Impl empty struct: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_impl_with_expression() {
    let source = r#"
        struct Config {
            name: String,
            value: Number
        }
        impl Config {
            "default config"
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Impl with expression: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Type Tests
// =============================================================================

#[test]
fn test_deeply_nested_array() {
    let source = "struct A { data: [[[[String]]]] }";
    assert!(
        compile(source).is_ok(),
        "Deeply nested array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_complex_dictionary() {
    let source = "struct A { map: [String: [Number]] }";
    assert!(
        compile(source).is_ok(),
        "Complex dictionary: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_optional_dictionary() {
    let source = "struct A { map: [String: Number]? }";
    assert!(
        compile(source).is_ok(),
        "Optional dictionary: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_closure_chain() {
    let source = "struct A { fn: String -> Number -> Boolean }";
    assert!(
        compile(source).is_ok(),
        "Closure chain: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Let Statement Tests
// =============================================================================

#[test]
fn test_let_string() {
    let source = "let name = \"value\"";
    assert!(
        compile(source).is_ok(),
        "Let string: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_let_number() {
    let source = "let count = 42";
    assert!(
        compile(source).is_ok(),
        "Let number: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_let_boolean() {
    let source = "let flag = true";
    assert!(
        compile(source).is_ok(),
        "Let boolean: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_let_array() {
    let source = "let items = [1, 2, 3]";
    assert!(
        compile(source).is_ok(),
        "Let array: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_pub_let() {
    let source = "pub let PUBLIC = \"public\"";
    assert!(
        compile(source).is_ok(),
        "Pub let: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_let_expression() {
    let source = "let counter = 0";
    assert!(
        compile(source).is_ok(),
        "Let expression: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Provides/Consumes Tests
// =============================================================================

#[test]
fn test_provides_simple() {
    let source = r#"
        struct Theme { color: String }
        struct Provider { theme: Theme }
        impl Provider {
            provides Theme { "blue" }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Provides simple: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_full_file() {
    let source = r#"
        // Traits
        trait Identifiable { id: Number }

        // Enums
        enum Status { active, inactive }

        // Structs
        struct User: Identifiable {
            id: Number,
            name: String,
            status: Status
        }

        // Modules
        module helpers {
            struct Config {
                debug: Boolean = false
            }
        }

        // Let bindings
        let VERSION = "1.0.0"

        // Impls
        impl User {
            name
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Full file: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_view_hierarchy() {
    let source = r#"
        struct Container {
            @mount header: String,
            @mount content: String,
            @mount footer: String?
        }

        struct Card {
            title: String,
            @mount body: String
        }

        struct Button {
            label: String,
            @mount onClick: String
        }

        impl Container { "Container" }
        impl Card { title }
        impl Button { label }
    "#;
    assert!(
        compile(source).is_ok(),
        "View hierarchy: {:?}",
        compile(source).err()
    );
}
