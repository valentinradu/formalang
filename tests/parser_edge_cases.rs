//! Parser edge case tests
//!
//! Tests for parser edge cases and AST node coverage

use formalang::parse_only;

// =============================================================================
// Parse Function Tests
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_compile_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }";
    compile(source).map_err(|e| format!("Compile simple: {e:?}"))?;
    Ok(())
}

#[test]
fn test_compile_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    compile(source).map_err(|e| format!("Compile empty: {e:?}"))?;
    Ok(())
}

#[test]
fn test_compile_whitespace() -> Result<(), Box<dyn std::error::Error>> {
    let source = "   \n\n   ";
    compile(source).map_err(|e| format!("Compile whitespace: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Compile comments: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Nil literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_literal_empty() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [String] = [] }
    ";
    compile(source).map_err(|e| format!("Empty array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_literal_single() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { items: [String] = ["one"] }
    "#;
    compile(source).map_err(|e| format!("Single item array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_literal_many() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { items: [Number] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }
    ";
    compile(source).map_err(|e| format!("Many items array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_negative_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = -42 }
    ";
    compile(source).map_err(|e| format!("Negative number: {e:?}"))?;
    Ok(())
}

#[test]
fn test_decimal_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 3.14159 }
    ";
    compile(source).map_err(|e| format!("Decimal number: {e:?}"))?;
    Ok(())
}

#[test]
fn test_negative_decimal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = -0.5 }
    ";
    compile(source).map_err(|e| format!("Negative decimal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_string_with_escapes() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "hello\nworld\t!" }
    "#;
    compile(source).map_err(|e| format!("String with escapes: {e:?}"))?;
    Ok(())
}

#[test]
fn test_string_with_quotes() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = "say \"hello\"" }
    "#;
    compile(source).map_err(|e| format!("String with quotes: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Arithmetic precedence: {e:?}"))?;
    Ok(())
}

#[test]
fn test_comparison_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = 1 < 2 && 2 < 3 }
    ";
    compile(source).map_err(|e| format!("Comparison chain: {e:?}"))?;
    Ok(())
}

#[test]
fn test_logical_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = true || false && true }
    ";
    compile(source).map_err(|e| format!("Logical precedence: {e:?}"))?;
    Ok(())
}

#[test]
fn test_parenthesized_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = (1 + 2) * 3 }
    ";
    compile(source).map_err(|e| format!("Parenthesized: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_parentheses() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = ((1 + 2) * (3 + 4)) }
    ";
    compile(source).map_err(|e| format!("Nested parentheses: {e:?}"))?;
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
    compile(source).map_err(|e| format!("If without else: {e:?}"))?;
    Ok(())
}

#[test]
fn test_simple_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String = if false { "a" } else { "b" } }
    "#;
    compile(source).map_err(|e| format!("Simple else: {e:?}"))?;
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
    compile(source).map_err(|e| format!("For with if: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: Number = (let a = 1
            in let b = 2
            in let c = 3
            in a)
        }
    ";
    compile(source).map_err(|e| format!("Let chain: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_expr_requires_in_separator() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #21: a `let` expression without the `in` keyword between
    // value and body should fail to parse — the grammar is no longer
    // ambiguous and shouldn't fall back to greedy parsing.
    let source = r"
        struct A {
            x: Number = (let a = 1
            a)
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("expected `let` without `in` to fail to parse, but it compiled".into());
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
    compile(source).map_err(|e| format!("Field access simple: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Field Access vs Enum Instantiation Tests
// =============================================================================

/// Regression test: field access on function parameters should NOT be parsed as enum instantiation.
/// Before the fix, `point.x` was being parsed as `EnumInstantiation { enum_name: "point", variant: "x" }`
/// instead of field access, causing codegen to output `Unknown_x` instead of `point.x`.
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
    parse_only(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    parse_only(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum and Match Tests
// =============================================================================

#[test]
fn test_enum_single_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Unit { unit }";
    compile(source).map_err(|e| format!("Enum single variant: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Enum many variants: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Match exhaustive: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Empty nested modules: {e:?}"))?;
    Ok(())
}

#[test]
fn test_sibling_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod a { struct A { } }
        mod b { struct B { } }
        mod c { struct C { } }
    ";
    compile(source).map_err(|e| format!("Sibling modules: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Trait Tests
// =============================================================================

#[test]
fn test_trait_single_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Single { field: String }";
    compile(source).map_err(|e| format!("Trait single field: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Trait many fields: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Container<T> { item: T }";
    compile(source).map_err(|e| format!("Trait with generics: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base { base: String }
        trait Derived { derived: Number }
        struct Impl { base: String, derived: Number }
    ";
    compile(source).map_err(|e| format!("Trait inheritance: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct Tests
// =============================================================================

#[test]
fn test_struct_single_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Single { field: String }";
    compile(source).map_err(|e| format!("Struct single field: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Struct many fields: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_modifiers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Full {
            mut count: Number,
            content: String,
            optional: String?,
            default: Number = 0
        }
    ";
    compile(source).map_err(|e| format!("Struct with modifiers: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Box<T> { value: T }";
    compile(source).map_err(|e| format!("Struct with generics: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Struct multiple generic params: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Struct with default: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Struct with default expression: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Type Tests
// =============================================================================

#[test]
fn test_deeply_nested_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { data: [[[[String]]]] }";
    compile(source).map_err(|e| format!("Deeply nested array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_complex_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: [Number]] }";
    compile(source).map_err(|e| format!("Complex dictionary: {e:?}"))?;
    Ok(())
}

#[test]
fn test_optional_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { map: [String: Number]? }";
    compile(source).map_err(|e| format!("Optional dictionary: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { callback: String -> Number -> Boolean }";
    compile(source).map_err(|e| format!("Closure chain: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let Statement Tests
// =============================================================================

#[test]
fn test_let_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let name = \"value\"";
    compile(source).map_err(|e| format!("Let string: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let count = 42";
    compile(source).map_err(|e| format!("Let number: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let flag = true";
    compile(source).map_err(|e| format!("Let boolean: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let items = [1, 2, 3]";
    compile(source).map_err(|e| format!("Let array: {e:?}"))?;
    Ok(())
}

#[test]
fn test_pub_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub let PUBLIC = \"public\"";
    compile(source).map_err(|e| format!("Pub let: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let counter = 0";
    compile(source).map_err(|e| format!("Let expression: {e:?}"))?;
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
        struct User {
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
    compile(source).map_err(|e| format!("Full file: {e:?}"))?;
    Ok(())
}

#[test]
fn test_view_hierarchy() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            header: String,
            content: String,
            footer: String?
        }

        struct Card {
            title: String,
            body: String
        }

        struct Button {
            label: String,
            onClick: String
        }

        impl Card {
            fn getBody() -> String { self.title }
        }

        impl Button {
            fn getOnClick() -> String { self.label }
        }
    ";
    compile(source).map_err(|e| format!("View hierarchy: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Function in impl: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Function with params: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Function without return type: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Functions with struct defaults: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Multiple functions: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Function Call and Method Call Tests
// =============================================================================

#[test]
fn test_function_call_single_arg() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(angle: Number) -> Number { angle }
        struct A { x: Number = compute(angle: 1.0) }
    ";
    compile(source).map_err(|e| format!("Function call single arg: {e:?}"))?;
    Ok(())
}

#[test]
fn test_function_call_multiple_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn clamp(val: Number, lo: Number) -> Number { val }
        struct A { x: Number = clamp(val: 1.0, lo: 2.0) }
    ";
    compile(source).map_err(|e| format!("Function call multiple args: {e:?}"))?;
    Ok(())
}

#[test]
fn test_function_call_qualified_path() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod math {
            pub fn compute(angle: Number) -> Number { angle }
        }
        fn call_compute() -> Number { math::compute(angle: 1.0) }
    ";
    compile(source).map_err(|e| format!("Function call qualified path: {e:?}"))?;
    Ok(())
}

#[test]
fn test_function_call_nested() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn double(x: Number) -> Number { x }
        struct A { x: Number = double(x: double(x: 1.0)) }
    ";
    compile(source).map_err(|e| format!("Nested function calls: {e:?}"))?;
    Ok(())
}

#[test]
fn test_method_call_single() -> Result<(), Box<dyn std::error::Error>> {
    // Method calls on structs with extern impl are allowed
    let source = r"
        struct Canvas { width: Number, height: Number }
        extern impl Canvas {
            fn area(self) -> Number
        }
        extern fn get_canvas() -> Canvas
        struct A { x: Canvas }
        impl A {
            fn get_area() -> Number { self.x.area() }
        }
    ";
    compile(source).map_err(|e| format!("Method call: {e:?}"))?;
    Ok(())
}

#[test]
fn test_method_call_with_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Canvas { width: Number, height: Number }
        extern impl Canvas {
            fn scale(self, factor: Number) -> Canvas
        }
        extern fn get_canvas() -> Canvas
        struct A { x: Canvas }
        impl A {
            fn get_scaled() -> Canvas { self.x.scale(factor: 2) }
        }
    ";
    compile(source).map_err(|e| format!("Method call with args: {e:?}"))?;
    Ok(())
}

#[test]
fn test_method_call_chained() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Canvas { width: Number, height: Number }
        extern impl Canvas {
            fn flip(self) -> Canvas
            fn area(self) -> Number
        }
        extern fn get_canvas() -> Canvas
        struct A { x: Canvas }
        impl A {
            fn get_area() -> Number { self.x.flip().area() }
        }
    ";
    compile(source).map_err(|e| format!("Chained method calls: {e:?}"))?;
    Ok(())
}

#[test]
fn test_function_and_method_mixed() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn process(val: Number) -> Number { val }
        struct A { x: Number }
        impl A {
            fn compute() -> Number { process(val: self.x) }
        }
    ";
    compile(source).map_err(|e| format!("Mixed function and method calls: {e:?}"))?;
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
    compile(source).map_err(|e| format!("Function parameter with default: {e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
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
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error recovery: two independent malformed statements both surface
// separate parse errors (the parser skips to the next statement-start
// token on failure rather than bailing).
// =============================================================================

#[test]
fn test_parse_reports_errors_on_malformed_input() -> Result<(), Box<dyn std::error::Error>> {
    // Two independent top-level statements start with tokens that can't
    // begin a statement. The recovery strategy skips the bad tokens,
    // parses the good statement between them, then errors on the
    // second garbage run — so both surface.
    let source = r"
@@@
struct Good { y: Number }
###
";
    let errors = parse_only(source).err().ok_or("expected parse errors")?;
    if errors.len() < 2 {
        return Err(format!(
            "expected at least 2 parse errors after recovery, got {}: {errors:?}",
            errors.len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_block_expr_recovery_within_fn_body() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #40: a malformed let inside an inner block expression must not
    // hide the parse error in a sibling fn body. Both should surface.
    let source = r"
pub fn first() -> Number {
    let _result: Number = {
        let x: Number = + +
        1
    }
    2
}

pub fn second() -> Number {
    let y: Number = + +
    2
}
";
    let errors = parse_only(source).err().ok_or("expected parse errors")?;
    let parse_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, formalang::CompilerError::ParseError { .. }))
        .collect();
    if parse_errors.len() < 2 {
        return Err(format!(
            "expected at least 2 parse errors after block recovery, got {}: {errors:?}",
            parse_errors.len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_fn_body_recovery_surfaces_multiple_errors() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #40: a bad expression inside one function body must not abort
    // diagnostics for subsequent function bodies. Each broken `let` value
    // should surface its own ParseError.
    let source = r"
pub fn first() -> Number {
    let x: Number = + +
    1
}

pub fn second() -> Number {
    let y: Number = + +
    2
}
";
    let errors = parse_only(source).err().ok_or("expected parse errors")?;
    let parse_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, formalang::CompilerError::ParseError { .. }))
        .collect();
    if parse_errors.len() < 2 {
        return Err(format!(
            "expected at least 2 parse errors after fn-body recovery, got {}: {errors:?}",
            parse_errors.len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_struct_field_doc_comments_threaded_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B2: leading `///` doc comments on struct fields must reach
    // the IR through `IrField.doc`. Previously they were silently dropped
    // by the parser.
    let source = r"
struct User {
    /// The user's display name.
    name: String,
    /// Account age in days. Multi-line
    /// docs join with newlines.
    age: Number
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let user = module.structs.first().ok_or("expected User struct")?;
    let name_field = user
        .fields
        .iter()
        .find(|f| f.name == "name")
        .ok_or("name missing")?;
    if name_field.doc.as_deref() != Some("The user's display name.") {
        return Err(format!("name doc mismatch: {:?}", name_field.doc).into());
    }
    let age_field = user
        .fields
        .iter()
        .find(|f| f.name == "age")
        .ok_or("age missing")?;
    if age_field.doc.as_deref() != Some("Account age in days. Multi-line\ndocs join with newlines.")
    {
        return Err(format!("age doc mismatch: {:?}", age_field.doc).into());
    }
    Ok(())
}

#[test]
fn test_trait_field_doc_comments_threaded_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B2: trait field doc comments must also survive to the IR.
    let source = r"
trait Shape {
    /// Total surface area, in square units.
    area: Number
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let shape = module.traits.first().ok_or("expected Shape trait")?;
    let area = shape
        .fields
        .iter()
        .find(|f| f.name == "area")
        .ok_or("area missing")?;
    if area.doc.as_deref() != Some("Total surface area, in square units.") {
        return Err(format!("trait-field doc mismatch: {:?}", area.doc).into());
    }
    Ok(())
}

#[test]
fn test_enum_variant_field_doc_comments_threaded_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B2: enum-variant field doc comments must also reach the IR.
    let source = r"
enum Event {
    Click(/// X coordinate of the click.
        x: Number, /// Y coordinate of the click.
        y: Number)
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let event = module.enums.first().ok_or("expected Event enum")?;
    let click = event
        .variants
        .iter()
        .find(|v| v.name == "Click")
        .ok_or("Click variant missing")?;
    let x_field = click
        .fields
        .iter()
        .find(|f| f.name == "x")
        .ok_or("x missing")?;
    if x_field.doc.as_deref() != Some("X coordinate of the click.") {
        return Err(format!("enum-variant field doc mismatch: {:?}", x_field.doc).into());
    }
    Ok(())
}
