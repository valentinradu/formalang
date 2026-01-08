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
        struct A { x: String? = nil }
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
        struct A { items: [String] = [] }
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
        struct A { items: [String] = ["one"] }
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
        struct A { items: [Number] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }
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
        struct A { x: Number = -42 }
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
        struct A { x: Number = 3.14159 }
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
        struct A { x: Number = -0.5 }
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
        struct A { x: String = "hello\nworld\t!" }
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
        struct A { x: String = "say \"hello\"" }
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
        struct A { x: Number = 1 + 2 * 3 }
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
        struct A { x: Boolean = 1 < 2 && 2 < 3 }
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
        struct A { x: Boolean = true || false && true }
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
        struct A { x: Number = (1 + 2) * 3 }
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
        struct A { x: Number = ((1 + 2) * (3 + 4)) }
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
        struct A { x: String? = if true { "yes" } }
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
        struct A { x: String = if false { "a" } else { "b" } }
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
        struct A {
            x: [String] = for item in ["a", "b", "c"] {
                if true { item } else { "default" }
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
        struct A {
            x: Number = (let a = 1
            let b = 2
            let c = 3
            a)
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
    // Field access to another field uses self, which is only valid in impl functions
    let source = r#"
        struct Inner { value: String }
        struct Outer { inner: Inner }
        impl Outer {
            fn display() -> Inner { self.inner }
        }
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
            fn result() -> String {
                match self.x {
                    .a: "first",
                    .b: "second"
                }
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
        mod a {
            mod b {
                mod c { }
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
        mod a { struct A { } }
        mod b { struct B { } }
        mod c { struct C { } }
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
fn test_struct_with_default() {
    let source = r#"
        struct Empty { x: String = "empty" }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct with default: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_struct_with_default_expression() {
    let source = r#"
        struct Config {
            name: String = "default config",
            value: Number
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Struct with default expression: {:?}",
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
    let source = "struct A { callback: String -> Number -> Boolean }";
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
    assert!(
        compile(source).is_ok(),
        "Full file: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_view_hierarchy() {
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
    assert!(
        compile(source).is_ok(),
        "View hierarchy: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Function in Impl Block Tests
// =============================================================================

#[test]
fn test_fn_in_impl_simple() {
    let source = r#"
        struct Rect {
            width: Number,
            height: Number
        }

        impl Rect {
            fn area(self) -> Number {
                self.width
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function in impl: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_in_impl_with_params() {
    let source = r#"
        struct Point {
            x: Number,
            y: Number
        }

        impl Point {
            fn add(self, other: Point) -> Point {
                Point(x: self.x, y: self.y)
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function with params: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_in_impl_no_return_type() {
    let source = r#"
        struct Counter {
            count: Number
        }

        impl Counter {
            fn increment(self) {
                self.count
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function without return type: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_in_impl_with_struct_defaults() {
    // Impl blocks now only contain functions; defaults go in struct
    let source = r#"
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
    "#;
    assert!(
        compile(source).is_ok(),
        "Functions with struct defaults: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_in_impl_multiple_functions() {
    // No commas between functions in impl blocks
    let source = r#"
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
    "#;
    assert!(
        compile(source).is_ok(),
        "Multiple functions: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Function Call and Method Call Tests
// =============================================================================

#[test]
fn test_function_call_single_arg() {
    // Function calls require module::function syntax with named args
    let source = r#"
        struct A { x: Number = math::sin(angle: 1.0) }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function call single arg: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_function_call_multiple_args() {
    // Function calls require module::function syntax with named args
    let source = r#"
        struct A { x: Number = math::max(a: 1.0, b: 2.0) }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function call multiple args: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_function_call_qualified_path() {
    // Function calls require module::function syntax with named args
    let source = r#"
        struct A { x: Number = builtin::math::sin(angle: 1.0) }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function call qualified path: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_function_call_nested() {
    // Function calls require module::function syntax with named args
    let source = r#"
        struct A { x: Number = math::sin(angle: math::cos(angle: 1.0)) }
    "#;
    assert!(
        compile(source).is_ok(),
        "Nested function calls: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_method_call_single() {
    // self is only valid in impl functions
    let source = r#"
        struct A { x: Number }
        impl A {
            fn get_abs() -> Number { self.x.abs() }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Method call: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_method_call_with_args() {
    // self is only valid in impl functions
    let source = r#"
        struct A { x: Number }
        impl A {
            fn get_clamped() -> Number { self.x.clamp(0, 100) }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Method call with args: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_method_call_chained() {
    // self is only valid in impl functions
    let source = r#"
        struct A { x: Number }
        impl A {
            fn get_floored() -> Number { self.x.abs().floor() }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Chained method calls: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_function_and_method_mixed() {
    // self is only valid in impl functions
    let source = r#"
        struct A { x: Number }
        impl A {
            fn get_max() -> Number { max(self.x.abs(), 0) }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Mixed function and method calls: {:?}",
        compile(source).err()
    );
}

// =============================================================================
// Function Parameter Default Tests
// =============================================================================

#[test]
fn test_fn_param_with_default() {
    let source = r#"
        struct Counter {
            value: Number
        }
        impl Counter {
            fn add(self, amount: Number = 1) -> Number {
                self.value
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function parameter with default: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_param_multiple_defaults() {
    let source = r#"
        struct Config {
            value: Number
        }
        impl Config {
            fn configure(self, timeout: Number = 30, retries: Number = 3) -> Number {
                self.value
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Multiple function parameters with defaults: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_param_mixed_with_and_without_defaults() {
    let source = r#"
        struct Calculator {
            value: Number
        }
        impl Calculator {
            fn compute(self, x: Number, scale: Number = 1.0, offset: Number = 0.0) -> Number {
                self.value
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function with mixed default/non-default params: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_param_default_with_expression() {
    let source = r#"
        struct Math {
            base: Number
        }
        impl Math {
            fn calculate(self, factor: Number = 2 * 3 + 1) -> Number {
                self.base
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function parameter with expression default: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_param_default_with_string() {
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
    assert!(
        compile(source).is_ok(),
        "Function parameter with string default: {:?}",
        compile(source).err()
    );
}

#[test]
fn test_fn_param_default_with_boolean() {
    let source = r#"
        struct Toggle {
            state: Boolean
        }
        impl Toggle {
            fn set(self, enabled: Boolean = true) -> Boolean {
                self.state
            }
        }
    "#;
    assert!(
        compile(source).is_ok(),
        "Function parameter with boolean default: {:?}",
        compile(source).err()
    );
}
