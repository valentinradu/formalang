//! Parser edge case tests
//!
//! Tests for parser edge cases and AST node coverage

use formalang::{compile, parse_only};

// =============================================================================
// Parse Function Tests
// =============================================================================

#[test]
fn test_compile_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Compile simple: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_compile_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Compile empty: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_compile_whitespace() -> Result<(), Box<dyn std::error::Error>> {
    let source = "   \n\n   ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Compile whitespace: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_compile_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        // Single line comment
        struct A { }
        /* Block comment */
        struct B { }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Compile comments: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Expression Tests
// =============================================================================

#[test]
fn test_nil_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String? = nil }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nil literal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_literal_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [String] = [] }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Empty array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_literal_single() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { items: [String] = ["one"] }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Single item array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_array_literal_many() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [Number] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Many items array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_negative_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = -42 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Negative number: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_decimal_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 3.14159 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Decimal number: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_negative_decimal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = -0.5 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Negative decimal: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_string_with_escapes() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "hello\nworld\t!" }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("String with escapes: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_string_with_quotes() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "say \"hello\"" }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("String with quotes: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Operator Precedence Tests
// =============================================================================

#[test]
fn test_arithmetic_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 1 + 2 * 3 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Arithmetic precedence: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_comparison_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = 1 < 2 && 2 < 3 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Comparison chain: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_logical_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = true || false && true }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Logical precedence: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_parenthesized_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = (1 + 2) * 3 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Parenthesized: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_nested_parentheses() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = ((1 + 2) * (3 + 4)) }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested parentheses: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Control Flow Tests
// =============================================================================

#[test]
fn test_if_without_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String? = if true { "yes" } }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("If without else: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_simple_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = if false { "a" } else { "b" } }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Simple else: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_for_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            x: [String] = for item in ["a", "b", "c"] {
                if true { item } else { "default" }
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("For with if: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: Number = (let a = 1
            let b = 2
            let c = 3
            a)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let chain: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Field Access Tests
// =============================================================================

#[test]
fn test_field_access_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Field access to another field uses self, which is only valid in impl functions
    let source = r"
        struct Inner { value: String }
        struct Outer { inner: Inner }
        impl Outer {
            fn display() -> Inner { self.inner }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Field access simple: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Field Access vs Enum Instantiation Tests
// =============================================================================

/// Regression test: field access on function parameters should NOT be parsed as enum instantiation.
/// Before the fix, `point.x` was being parsed as `EnumInstantiation { enum_name: "point", variant: "x" }`
/// instead of field access, causing WGSL codegen to output `Unknown_x` instead of `point.x`.
#[test]
fn test_field_access_on_parameter_parses() -> Result<(), Box<dyn std::error::Error>> {
    // This tests that lowercase.identifier parses correctly (not as enum instantiation)
    // We use parse_only to test just parsing, not semantic analysis
    let source = r"
        struct Point { x: Number, y: Number }
        impl Point {
            fn get_x(p: Point) -> Number { p.x }
        }
    ";
    let result = parse_only(source);
    if result.is_err() {
        return Err(format!("Field access on parameter should parse: {:?}", result.err()).into());
    }
    Ok(())
}

/// Enum instantiation requires uppercase type name
#[test]
fn test_enum_instantiation_requires_uppercase() -> Result<(), Box<dyn std::error::Error>> {
    // Status.active should parse as enum instantiation (uppercase S)
    let source = r"
        enum Status { active, inactive }
        let s: Status = Status.active
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Enum instantiation with uppercase type should compile: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

/// Field access chain on parameters parses correctly
#[test]
fn test_field_access_chain_on_parameter_parses() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { value: Number }
        struct Outer { inner: Inner }
        impl Outer {
            fn get_value(o: Outer) -> Number { o.inner.value }
        }
    ";
    let result = parse_only(source);
    if result.is_err() {
        return Err(
            format!(
                "Field access chain on parameter should parse: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

// =============================================================================
// Enum and Match Tests
// =============================================================================

#[test]
fn test_enum_single_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Unit { unit }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Enum single variant: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_enum_many_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Colors {
            red,
            orange,
            yellow,
            green,
            blue,
            indigo,
            violet
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Enum many variants: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_match_exhaustive() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum AB { a, b }
        struct Handler { x: AB }
        impl Handler {
            fn result() -> String {
                match self.x {
                    .a: "first",
                    .b: "second"
                }
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Match exhaustive: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_empty_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod a {
            mod b {
                mod c { }
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Empty nested modules: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_sibling_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod a { struct A { } }
        mod b { struct B { } }
        mod c { struct C { } }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Sibling modules: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Trait Tests
// =============================================================================

#[test]
fn test_trait_single_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Single { field: String }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Trait single field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_many_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Many {
            a: String,
            b: Number,
            c: Boolean,
            d: [String],
            e: String?
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Trait many fields: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Container<T> { item: T }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Trait with generics: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_trait_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base { base: String }
        trait Derived: Base { derived: Number }
        struct Impl: Derived { base: String, derived: Number }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Trait inheritance: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Struct Tests
// =============================================================================

#[test]
fn test_struct_single_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Single { field: String }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct single field: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_many_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Many {
            a: String,
            b: Number,
            c: Boolean,
            d: [String],
            e: String?,
            f: [String: Number]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct many fields: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_with_modifiers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Full {
            mut count: Number,
            @mount content: String,
            optional: String?,
            default: Number = 0
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct with modifiers: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Box<T> { value: T }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct with generics: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_multiple_generic_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Map<K, V> {
            keys: [K],
            values: [V]
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct multiple generic params: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Impl Tests
// =============================================================================

#[test]
fn test_struct_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Empty { x: String = "empty" }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct with default: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_struct_with_default_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            name: String = "default config",
            value: Number
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Struct with default expression: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Type Tests
// =============================================================================

#[test]
fn test_deeply_nested_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { data: [[[[String]]]] }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Deeply nested array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_complex_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: [Number]] }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Complex dictionary: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_optional_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: Number]? }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Optional dictionary: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> Number -> Boolean }";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure chain: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Let Statement Tests
// =============================================================================

#[test]
fn test_let_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let name = \"value\"";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let string: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let count = 42";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let number: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let flag = true";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let boolean: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let items = [1, 2, 3]";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let array: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_pub_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub let PUBLIC = \"public\"";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Pub let: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_let_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let counter = 0";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let expression: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_full_file() -> Result<(), Box<dyn std::error::Error>> {
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
        mod helpers {
            struct Config {
                debug: Boolean = false
            }
        }

        // Let bindings
        let VERSION = "1.0.0"

        // Structs with defaults
        struct Display {
            user: User = User(id: 1, name: "test", status: Status.active)
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Full file: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_view_hierarchy() -> Result<(), Box<dyn std::error::Error>> {
    // Mount field references to other fields use self, which is only valid in impl functions
    let source = r#"
        struct Container {
            @mount header: String = "Container",
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

        impl Card {
            fn getBody() -> String { self.title }
        }

        impl Button {
            fn getOnClick() -> String { self.label }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("View hierarchy: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Function in Impl Block Tests
// =============================================================================

#[test]
fn test_fn_in_impl_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Rect {
            width: Number,
            height: Number
        }

        impl Rect {
            fn area(self) -> Number {
                self.width
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function in impl: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_fn_in_impl_with_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: Number,
            y: Number
        }

        impl Point {
            fn add(self, other: Point) -> Point {
                Point(x: self.x, y: self.y)
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function with params: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_fn_in_impl_no_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            count: Number
        }

        impl Counter {
            fn increment(self) {
                self.count
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function without return type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_fn_in_impl_with_struct_defaults() -> Result<(), Box<dyn std::error::Error>> {
    // Impl blocks now only contain functions; defaults go in struct
    let source = r"
        struct Rect {
            width: Number = 100,
            height: Number = 50
        }

        impl Rect {
            fn area(self) -> Number {
                self.width
            }

            fn perimeter(self) -> Number {
                self.width
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Functions with struct defaults: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_fn_in_impl_multiple_functions() -> Result<(), Box<dyn std::error::Error>> {
    // No commas between functions in impl blocks
    let source = r"
        struct Vec2 {
            x: Number,
            y: Number
        }

        impl Vec2 {
            fn length(self) -> Number {
                self.x
            }

            fn normalize(self) -> Vec2 {
                Vec2(x: self.x, y: self.y)
            }

            fn dot(self, other: Vec2) -> Number {
                self.x
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Multiple functions: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Function Call and Method Call Tests
// =============================================================================

#[test]
fn test_function_call_single_arg() -> Result<(), Box<dyn std::error::Error>> {
    // Function calls require module::function syntax with named args
    let source = r"
        struct A { x: Number = math::sin(angle: 1.0) }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function call single arg: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_function_call_multiple_args() -> Result<(), Box<dyn std::error::Error>> {
    // Function calls require module::function syntax with named args
    let source = r"
        struct A { x: Number = math::max(a: 1.0, b: 2.0) }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function call multiple args: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_function_call_qualified_path() -> Result<(), Box<dyn std::error::Error>> {
    // Function calls require module::function syntax with named args
    let source = r"
        struct A { x: Number = builtin::math::sin(angle: 1.0) }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function call qualified path: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_function_call_nested() -> Result<(), Box<dyn std::error::Error>> {
    // Function calls require module::function syntax with named args
    let source = r"
        struct A { x: Number = math::sin(angle: math::cos(angle: 1.0)) }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Nested function calls: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_single() -> Result<(), Box<dyn std::error::Error>> {
    // self is only valid in impl functions
    let source = r"
        struct A { x: Number }
        impl A {
            fn get_abs() -> Number { self.x.abs() }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Method call: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_with_args() -> Result<(), Box<dyn std::error::Error>> {
    // self is only valid in impl functions
    let source = r"
        struct A { x: Number }
        impl A {
            fn get_clamped() -> Number { self.x.clamp(0, 100) }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Method call with args: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_chained() -> Result<(), Box<dyn std::error::Error>> {
    // self is only valid in impl functions
    let source = r"
        struct A { x: Number }
        impl A {
            fn get_floored() -> Number { self.x.abs().floor() }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Chained method calls: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_function_and_method_mixed() -> Result<(), Box<dyn std::error::Error>> {
    // self is only valid in impl functions
    let source = r"
        struct A { x: Number }
        impl A {
            fn get_max() -> Number { max(self.x.abs(), 0) }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Mixed function and method calls: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Function Parameter Default Tests
// =============================================================================

#[test]
fn test_fn_param_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            value: Number
        }
        impl Counter {
            fn add(self, amount: Number = 1) -> Number {
                self.value
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function parameter with default: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_fn_param_multiple_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            value: Number
        }
        impl Config {
            fn configure(self, timeout: Number = 30, retries: Number = 3) -> Number {
                self.value
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Multiple function parameters with defaults: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

#[test]
fn test_fn_param_mixed_with_and_without_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            value: Number
        }
        impl Calculator {
            fn compute(self, x: Number, scale: Number = 1.0, offset: Number = 0.0) -> Number {
                self.value
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function with mixed default/non-default params: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_fn_param_default_with_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math {
            base: Number
        }
        impl Math {
            fn calculate(self, factor: Number = 2 * 3 + 1) -> Number {
                self.base
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!(
                "Function parameter with expression default: {:?}",
                result.err()
            )
            .into(),
        );
    }
    Ok(())
}

#[test]
fn test_fn_param_default_with_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Greeter {
            name: String
        }
        impl Greeter {
            fn greet(self, message: String = "Hello") -> String {
                self.name
            }
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Function parameter with string default: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

#[test]
fn test_fn_param_default_with_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Toggle {
            state: Boolean
        }
        impl Toggle {
            fn set(self, enabled: Boolean = true) -> Boolean {
                self.state
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Function parameter with boolean default: {:?}", result.err()).into(),
        );
    }
    Ok(())
}
