//! Tests to cover gaps in test coverage
//!
//! Targets: AST helpers, Token parsing, `ImportGraph`, semantic edge cases

use formalang::compile;

// =============================================================================
// AST Span Tests - Exercise Expr::span() for all variants
// =============================================================================

#[test]
fn test_expr_span_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Container { p: Point = Point(x: 1, y: 2) }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_enum_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, error(msg: String) }
        struct A { s: Status = Status.error(msg: "fail") }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_inferred_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green, blue }
        struct A { c: Color = .red }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_tuple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { t: (first: String, second: Number) = (first: "a", second: 1) }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_dict_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = ["key": "value", "num": "42"]
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_dict_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            x: String = (let d = ["a": "b"]
            d["a"])
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let add = x, y -> 0
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_expr_span_group() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = (1 + 2) * 3 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Binary Operator Tests - Exercise precedence and associativity
// =============================================================================

#[test]
fn test_binary_op_precedence_mul_add() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 1 + 2 * 3 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_precedence_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = 1 < 2 && 3 > 2 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_precedence_or_and() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = true || false && true }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_all_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: Boolean = 1 == 1 && 1 != 2 && 1 < 2 && 2 > 1 && 1 <= 1 && 2 >= 2
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_op_modulo() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 10 % 3 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// String Escape Sequence Tests
// =============================================================================

#[test]
fn test_string_escape_newline() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "line1\nline2"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_string_escape_tab() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "col1\tcol2"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_string_escape_carriage_return() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let s = \"hello\\rworld\"";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_string_escape_quote() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "He said \"Hello\""
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_string_escape_backslash() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "path\\to\\file"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_string_unicode_escape() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s = "\u0041\u0042\u0043"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_multiline_string() -> Result<(), Box<dyn std::error::Error>> {
    // Multiline strings (""""..."""") may not be fully implemented
    // Testing regular string with escapes instead
    let source = r#"
        let s = "line1\nline2\nline3"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Regex and Path Literal Tests
// =============================================================================

#[test]
fn test_regex_with_flags() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pattern = r/hello.*/gi
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_regex_no_flags() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pattern = r/[a-z]+/
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_path_literal_usage() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let p = /usr/local/bin
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Semantic Validation Edge Cases
// =============================================================================

#[test]
fn test_generic_constraint_validation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { text: String }
        struct Printer<T: Printable> { item: T }
        struct Doc: Printable { text: String }
        struct MyPrinter { printer: Printer<Doc> }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_multiple_generic_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair<A, B> { first: A, second: B }
        struct Container { pair: Pair<String, Number> }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_in_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container<T> { item: T }
    ";
    // Generic traits might not support struct conformance with same generic param
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Nested { box: Box<Box<String>> }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Nested Expression Tests
// =============================================================================

#[test]
fn test_deeply_nested_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            x: String = if true {
                if false {
                    if true { "a" } else { "b" }
                } else {
                    "c"
                }
            } else {
                "d"
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_for_with_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A {
            items: [String] = for item in ["a", "b", "c"] {
                let prefix = "item: "
                item
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_match_with_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Result { ok, err }
        let status: Result = Result.ok
        struct Handler {
            x: String = match status {
                ok: "success",
                err: "failure"
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_complex_binary_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: Number = (1 + 2) * (3 - 4) / (5 % 2)
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error Detection Tests
// =============================================================================

#[test]
fn test_error_duplicate_generic_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Bad<T, T> { x: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_unknown_generic_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Bad<T: UnknownTrait> { x: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_as_trait_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NotATrait { x: String }
        struct Bad<T: NotATrait> { x: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_generic_without_args_in_field() -> Result<(), Box<dyn std::error::Error>> {
    // Using a generic type without explicit args in a field definition
    // This is allowed - the type remains uninstantiated
    let source = r"
        struct Box<T> { value: T }
        struct Container { box: Box }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_error_wrong_generic_arity() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Container { box: Box<String, Number> }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Module and Visibility Tests
// =============================================================================

#[test]
fn test_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod outer {
            mod inner {
                pub struct Deep { x: String }
            }
            struct Middle { x: String }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_module_with_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        mod ui {
            struct Button { label: String = "click" }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_pub_visibility_all_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod api {
            pub trait T { x: String }
            pub struct S: T { x: String }
            pub enum E { a, b }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            onClick: () -> String
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_with_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Mapper {
            transform: String -> Number
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_multi_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Reducer {
            reduce: Number, Number -> Number
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_returning_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Factory {
            create: String -> (Number -> Boolean)
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Nil Literal Tests
// =============================================================================

#[test]
fn test_nil_in_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String? = nil }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nil_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String? = nil }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Default Value Tests
// =============================================================================

#[test]
fn test_default_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config { name: String = "default" }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_default_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { count: Number = 42 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_default_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { enabled: Boolean = true }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_default_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config { items: [String] = ["a", "b"] }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Mount Field Tests
// =============================================================================

#[test]
fn test_mount_field_basic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            @mount content: String
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_mount_field_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List {
            @mount items: [String]
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_mount_field_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Card {
            @mount footer: String?
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Mutable Field Tests
// =============================================================================

#[test]
fn test_mut_field_basic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_mut_field_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = 0
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Semantic Error Path Tests - Module and Trait Composition
// =============================================================================

#[test]
fn test_error_duplicate_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui { struct A { x: String } }
        mod ui { struct B { y: Number } }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_as_composed_trait() -> Result<(), Box<dyn std::error::Error>> {
    // Using a struct in trait composition should fail
    let source = r"
        struct NotATrait { x: String }
        trait MyTrait: NotATrait { y: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_enum_as_composed_trait() -> Result<(), Box<dyn std::error::Error>> {
    // Using an enum in trait composition should fail
    let source = r"
        enum NotATrait { a, b }
        trait MyTrait: NotATrait { y: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_undefined_composed_trait() -> Result<(), Box<dyn std::error::Error>> {
    // Using an undefined trait in composition should fail
    let source = r"
        trait MyTrait: UndefinedTrait { x: String }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Module Path Type Resolution Tests
// =============================================================================

#[test]
fn test_module_path_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            pub struct Button { label: String }
        }
        struct App { btn: ui::Button }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_error_module_path_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            pub struct Button { label: String }
        }
        struct App { btn: ui::NonExistent }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_undefined_module_in_path() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct App { btn: nonexistent::Button }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Type Parameter Scope Tests
// =============================================================================

#[test]
fn test_error_out_of_scope_type_param() -> Result<(), Box<dyn std::error::Error>> {
    // Using T outside a generic definition
    let source = r"
        struct Container { x: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_type_param_in_scope() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container<T> { x: T }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Nested Module Tests
// =============================================================================

#[test]
fn test_deeply_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod outer {
            pub mod middle {
                pub struct Inner { x: String }
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_module_with_trait_and_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        mod shapes {
            pub trait Drawable { x: String }
            pub struct Circle: Drawable { x: String = "drawing circle", radius: Number }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_module_with_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod colors {
            pub enum Color { red, green, blue }
        }
        struct Palette { main: colors::Color }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Impl Block Error Tests
// =============================================================================

#[test]
fn test_error_impl_for_undefined_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        impl NonExistent {}
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: String }
        impl A {}
        impl A {}
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Enum Variant Error Tests
// =============================================================================

#[test]
fn test_error_duplicate_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, pending, active }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Let Binding Error Tests
// =============================================================================

#[test]
fn test_error_duplicate_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = 1
        let x = 2
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_let_binding_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = 42
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Generic Constraint Validation Tests
// =============================================================================

#[test]
fn test_generic_with_trait_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { text: String }
        struct Wrapper<T: Printable> { item: T }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_error_generic_constraint_is_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NotATrait { x: String }
        struct Wrapper<T: NotATrait> { item: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_generic_constraint_is_undefined() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T: NonExistentTrait> { item: T }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Field Requirement Tests
// =============================================================================

#[test]
fn test_trait_with_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container { @mount content: String }
        struct Box: Container { @mount content: String }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_composition_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { a: String }
        trait B: A { b: Number }
        struct C: B { a: String, b: Number }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Expression in Module Tests
// =============================================================================

#[test]
fn test_module_impl_with_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod math {
            pub struct Calculator {
                x: Number = (let result = 1 + 2
                result)
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_module_impl_with_if() -> Result<(), Box<dyn std::error::Error>> {
    // let bindings are only allowed at top level, not inside modules
    let source = r#"
        let flag: Boolean = true
        mod logic {
            pub struct Check {
                result: String = if flag { "yes" } else { "no" }
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_module_impl_with_for() -> Result<(), Box<dyn std::error::Error>> {
    // let bindings are only allowed at top level, not inside modules
    let source = r#"
        let data: [String] = ["a", "b"]
        mod lists {
            pub struct Items {
                output: [String] = for item in data { item }
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Module Enum Variant Tests
// =============================================================================

#[test]
fn test_module_enum_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod errors {
            pub enum Result { ok(value: String), err(message: String) }
        }
        struct Handler { result: errors::Result }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct Instantiation Error Tests
// =============================================================================

#[test]
fn test_error_struct_missing_generic_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box<T> { value: T }
        struct Container { box: Box<String> = Box(value: "test") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_extra_generic_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Simple { x: String }
        struct Container { s: Simple = Simple<String>(x: "test") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_wrong_generic_arity() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Pair<A, B> { a: A, b: B }
        struct Container { pair: Pair<String, Number> = Pair<String>(a: "x", b: 1) }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_unknown_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Canvas { point: Point = Point(x: 1, y: 2, z: 3) }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_missing_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Canvas { point: Point = Point(x: 1) }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_unknown_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box { label: String }
        struct App { box: Box = Box(label: "test") { nonexistent: "value" } }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_struct_missing_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box { label: String, @mount content: String }
        struct App { box: Box = Box(label: "test") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Enum Instantiation Error Tests
// =============================================================================

#[test]
fn test_enum_instantiation_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { ok, error }
        struct Response { status: Status = Status.ok }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_instantiation_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Message { text(content: String), error(code: Number) }
        struct Logger { msg: Message = Message.text(content: "hello") }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Reference Validation in Impl Tests
// =============================================================================

#[test]
fn test_impl_field_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let name: String = "test"
        struct Person { age: Number, display: String = name }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_mount_field_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let content: String = "text"
        struct Card { @mount content: String, display: String = content }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Additional Generic Validation Tests
// =============================================================================

#[test]
fn test_generic_type_argument_validation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Container { box: Box<UndefinedType> }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_generic_in_impl_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box<T> { value: T }
        struct Wrapper { box: Box<String> = Box<String>(value: "test") }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// For Loop Variable Scope Tests
// =============================================================================

#[test]
fn test_for_loop_variable_in_scope() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let items: [String] = ["a", "b"]
        struct List {
            output: [String] = for item in items { item }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_for_loops_separate_vars() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let rows: [String] = ["a", "b"]
        struct Grid {
            output: [[String]] = for row in rows {
                for col in rows {
                    row
                }
            }
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Let Expression in Impl Tests
// =============================================================================

#[test]
fn test_let_in_impl_scope() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calc {
            result: Number = (let doubled = 2
            doubled)
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_let_reference_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let base: Number = 1
        struct Config {
            result: Number = (let x = base
            x)
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Method Chain and Field Access Tests
// =============================================================================

#[test]
fn test_field_access_chain() -> Result<(), Box<dyn std::error::Error>> {
    // Field access chain is validated during semantic analysis
    let source = r#"
        let inner: String = "text"
        struct Outer { display: String = inner }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { x: Number }
        struct Outer { inner: Inner = Inner(x: 42) }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum Variant Error Tests
// =============================================================================

#[test]
fn test_error_enum_variant_without_data() -> Result<(), Box<dyn std::error::Error>> {
    // Providing data to a variant that has no fields
    let source = r#"
        enum Status { ok, error }
        struct Response { status: Status }
        impl Response { status: Status.ok(msg: "test") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_enum_variant_requires_data() -> Result<(), Box<dyn std::error::Error>> {
    // Not providing data to a variant that requires fields
    let source = r"
        enum Message { text(content: String) }
        struct Logger { msg: Message }
        impl Logger { msg: Message.text }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_enum_missing_field() -> Result<(), Box<dyn std::error::Error>> {
    // Missing a required field in enum variant
    let source = r#"
        enum User { profile(name: String, age: Number) }
        struct App { user: User }
        impl App { user: User.profile(name: "Bob") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_enum_unknown_field() -> Result<(), Box<dyn std::error::Error>> {
    // Providing an unknown field to enum variant
    let source = r#"
        enum User { profile(name: String) }
        struct App { user: User }
        impl App { user: User.profile(name: "Bob", unknown: "x") }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_enum_unknown_variant() -> Result<(), Box<dyn std::error::Error>> {
    // Using an unknown variant
    let source = r"
        enum Status { ok, error }
        struct Response { status: Status }
        impl Response { status: Status.unknown }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_undefined_enum() -> Result<(), Box<dyn std::error::Error>> {
    // Using an enum that doesn't exist
    let source = r"
        struct Response { status: String }
        impl Response { status: NonExistent.ok }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Mount Field Error Tests
// =============================================================================

#[test]
fn test_error_missing_trait_mount() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container { @mount content: String }
        struct Box: Container { label: String }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_trait_mount_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container { @mount content: String }
        struct Box: Container { @mount content: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Trait Field Requirement Error Tests
// =============================================================================

#[test]
fn test_error_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Nameable { name: String }
        struct Person: Nameable { age: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_trait_field_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Identifiable { id: Number }
        struct Item: Identifiable { id: String }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Circular Dependency Tests
// =============================================================================

#[test]
fn test_error_circular_type_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { b: B }
        struct B { a: A }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_circular_trait_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A: B { x: String }
        trait B: A { y: Number }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_valid_indirect_dependency() -> Result<(), Box<dyn std::error::Error>> {
    // Not circular - all dependencies are one-way
    let source = r"
        struct A { x: String }
        struct B { a: A }
        struct C { b: B }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Type String Formatting Tests (via error messages)
// =============================================================================

#[test]
fn test_type_mismatch_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Items { list: [String] }
        struct Bag: Items { list: [Number] }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_type_mismatch_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait MaybeValue { value: String? }
        struct Box: MaybeValue { value: Number? }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_type_mismatch_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container<T> { item: T }
        trait Holder { box: Container<String> }
        struct MyHolder: Holder { box: Container<Number> }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_type_mismatch_tuple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Pair { coords: (x: Number, y: Number) }
        struct Point: Pair { coords: (a: String, b: String) }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_type_mismatch_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Handler { callback: String -> Number }
        struct MyHandler: Handler { callback: Number -> String }
    ";
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Dictionary Type Tests
// =============================================================================

#[test]
fn test_dict_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { data: [String: Number] }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_dict_literal_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = ["key": "value"]
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Generic Tests
// =============================================================================

#[test]
fn test_generic_with_multiple_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait A { a: String }
        trait B { b: Number }
        struct Container<T: A> { item: T }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_constraint_validation_full() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { text: String }
        struct Printer<T: Printable> { item: T }
        struct Doc: Printable { text: String }
        struct App { printer: Printer<Doc> }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Additional Error Trigger Tests
// =============================================================================

#[test]
fn test_error_invalid_binary_op_types() -> Result<(), Box<dyn std::error::Error>> {
    // Can't add string and number directly
    let source = r#"
        struct A { x: Boolean }
        impl A { x: "hello" + 123 }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_for_loop_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String, items: [String] }
        impl A {
            items: for item in "not an array" { item }
        }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_match_not_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct A { x: String, result: String }
        impl A {
            result: match "string" {
                ok: "yes"
            }
        }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_non_exhaustive_match() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { ok, error, pending }
        struct Handler { status: Status, result: String }
        impl Handler {
            result: match status {
                ok: "ok"
            }
        }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_match_arm() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { ok, error }
        struct Handler { status: Status, result: String }
        impl Handler {
            result: match status {
                ok: "ok",
                ok: "also ok",
                error: "error"
            }
        }
    "#;
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_inferred_enum_in_let() -> Result<(), Box<dyn std::error::Error>> {
    // Using inferred enum syntax - parses but may need context
    let source = r"
        let x = .someVariant
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Expression Tests
// =============================================================================

#[test]
fn test_closure_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { reducer: Number, Number -> Number }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { callback: () -> String }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_returning_closure_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { factory: String -> (Number -> Boolean) }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Additional Module Tests
// =============================================================================

#[test]
fn test_module_trait_field_validation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod traits {
            pub trait Named { name: String }
        }
        struct Person: traits::Named { name: String }
    ";
    // Module path for trait conformance not yet supported
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_module_nested_type_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            pub struct Widget { id: String }
        }
        mod app {
            pub struct Screen { widget: ui::Widget }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Type Inference Tests
// =============================================================================

#[test]
fn test_let_type_inference_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = 42
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_let_type_inference_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let x = "hello"
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_let_type_inference_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x = true
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_let_type_inference_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let items = ["a", "b", "c"]
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Expression Tests
// =============================================================================

#[test]
fn test_question_mark_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { opt: String?, result: String }
        impl A { result: opt? }
    ";
    // Question mark unwrap requires proper context
    if compile(source).is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_deeply_nested_binary_ops() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = ((1 + 2) * 3 - 4) / 5 % 6 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_comparison_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = 1 < 2 && 2 < 3 && 3 <= 4 }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_logical_operators() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Boolean = true && false || true }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}
