//! Integration tests for the `FormaLang` compiler
//!
//! These tests exercise the full compile pipeline: Lexer -> Parser -> Semantic Analyzer

use formalang::{compile, compile_to_ir, parse_only, CompilerError};

// =============================================================================
// Basic Definition Tests
// =============================================================================

#[test]
fn test_empty_file() -> Result<(), Box<dyn std::error::Error>> {
    let source = "";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_simple_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_public_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct User {
            name: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            nickname: String?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_default_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            timeout: Number = 30,
            enabled: Boolean = true
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_simple_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_implementing_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }

        struct User {
            name: String,
            age: Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_composition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }

        trait Aged {
            age: Number
        }

        struct Person {
            name: String,
            age: Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_simple_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            active,
            inactive,
            pending
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_with_associated_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Result {
            success(value: String),
            error(message: String, code: Number)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_module_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            struct Button {
                label: String
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Generic Type Tests
// =============================================================================

#[test]
fn test_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_struct_with_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            size: Number
        }

        struct Wrapper<T: Container> {
            item: T
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option<T> {
            some(value: T),
            none
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Primitive Type Tests
// =============================================================================

#[test]
fn test_all_primitive_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct AllTypes {
            s: String,
            n: Number,
            b: Boolean,
            p: Path,
            r: Regex
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_never_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Terminal {
            body: Never
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List {
            items: [String]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Matrix {
            rows: [[Number]]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_tuple_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            coords: (x: Number, y: Number)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Dictionary Type Tests
// =============================================================================

#[test]
fn test_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Cache {
            data: [String: Number]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_optional_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            settings: [String: String]?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NestedCache {
            data: [String: [String: Number]]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_type_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Factory {
            create: () -> String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_type_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Transformer {
            transform: String -> Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_type_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            compute: Number, Number -> Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_optional_closure_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            callback: (String -> Boolean)?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Impl Block Tests
// =============================================================================

#[test]
fn test_impl_block_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Greeting {
            message: String = "Hello, World!"
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_block_with_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner {
            value: Number
        }

        struct Outer {
            inner: Inner = Inner(value: 42)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Expression Tests
// =============================================================================

#[test]
fn test_string_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let greeting = "Hello"
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_number_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count = 42
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_boolean_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let yes = true
        let no = false
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nil_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let nothing = nil
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_path_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let file = /home/user/file.txt
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_regex_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let pattern = r/[a-z]+/i
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items = [1, 2, 3]
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_tuple_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let point = (x: 10, y: 20)
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let config = ["key": "value", "other": "data"]
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_empty_dictionary_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = [:]
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let sum = 1 + 2
        let diff = 5 - 3
        let product = 4 * 2
        let quotient = 10 / 2
        let remainder = 7 % 3
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let lt = 1 < 2
        let gt = 2 > 1
        let le = 1 <= 1
        let ge = 2 >= 2
        let eq = 1 == 1
        let ne = 1 != 2
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_logical() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let and_result = true && false
        let or_result = true || false
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Control Flow Expression Tests
// =============================================================================

#[test]
fn test_if_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Widget {
            content: String = if true { "yes" } else { "no" }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_for_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            value: String
        }

        struct List {
            items: [Item] = for item in ["a", "b", "c"] { Item(value: item) }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_match_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status {
            active,
            inactive
        }

        struct Display {
            status: Status
        }

        impl Display {
            fn text() -> String {
                match self.status {
                    active: "Active",
                    inactive: "Inactive"
                }
            }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Expression Tests
// =============================================================================

#[test]
fn test_closure_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let factory = () -> "created"
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_single_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let double = x -> 2
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_multi_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let add = x, y -> 0
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let greet = name: String -> "Hello"
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_expression_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            value: Number = (let x = 10
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Result {
            value: Number = (let x: Number = 10
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_mut() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            value: Number = (let mut count = 0
            count)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_let_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Computation {
            result: Number = (let a = 1
            let b = 2
            let c = 3
            a)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Parse-Only Tests
// =============================================================================

#[test]
fn test_parse_only_valid_syntax() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
    ";
    parse_only(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_parse_only_invalid_syntax() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name String
        }
    ";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Error Tests - Semantic Errors
// =============================================================================

#[test]
fn test_error_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            status: UndefinedType
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !(errors
        .iter()
        .any(|e| matches!(e, CompilerError::UndefinedType { .. })))
    {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User: UndefinedTrait {
            name: String
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_duplicate_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String
        }
        struct User {
            age: Number
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_missing_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }

        struct User: Named {
            age: Number
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_error_impl_for_undefined_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        impl UndefinedStruct {
            x: "value"
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// Complex Integration Tests
// =============================================================================

#[test]
fn test_complex_ui_component() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Renderable {
            render: String
        }

        struct Theme {
            primaryColor: String,
            fontSize: Number
        }

        struct Button {
            label: String,
            disabled: Boolean = false,
            render: String
        }

        struct Card {
            title: String,
            content: String = "Card component",
            actions: [Button]?
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_data_structures() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option<T> {
            some(value: T),
            none
        }

        enum Result<T, E> {
            ok(value: T),
            err(error: E)
        }

        struct Container<T> {
            items: [T],
            count: Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod ui {
            mod components {
                struct Button {
                    label: String
                }
            }

            struct Theme {
                color: String
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Comment Tests
// =============================================================================

#[test]
fn test_line_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        // This is a comment
        struct User {
            name: String // inline comment
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_block_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        /* Block comment */
        struct User {
            name: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Extern Type Field Tests
// =============================================================================

#[test]
fn test_struct_with_content_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            content: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Layout {
            header: String,
            body: String,
            footer: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Mutable Field Tests
// =============================================================================

#[test]
fn test_mutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut value: Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Reference Tests
// =============================================================================

#[test]
fn test_field_reference() -> Result<(), Box<dyn std::error::Error>> {
    // Field references (self.field) are only valid in impl functions
    let source = r"
        struct User {
            name: String
        }

        impl User {
            fn displayName() -> String {
                self.name
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_variant_reference() -> Result<(), Box<dyn std::error::Error>> {
    // Inferred enum instantiation in struct field default
    let source = r"
        enum Color {
            red,
            blue
        }

        struct Widget {
            color: Color = .red
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_inferred_enum_in_struct_instantiation_args() -> Result<(), Box<dyn std::error::Error>> {
    // Regression test: inferred enum variants inside struct instantiation arguments
    let source = r"
        enum SizeMode { auto, fixed(value: Number) }
        enum RepeatMode { none, horizontal, vertical, both }

        struct Size {
            width: SizeMode,
            height: SizeMode
        }

        struct Pattern {
            size: Size = Size(width: .auto, height: .auto),
            repeat: RepeatMode = .both
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Complete program — exercises every language feature end-to-end
// =============================================================================

#[test]
fn test_complete_program_compiles() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("fixtures/complete.fv");
    compile(source).map_err(|e| format!("complete.fv failed to compile: {e:?}"))?;
    Ok(())
}

#[test]
fn test_complete_program_lowers_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("fixtures/complete.fv");
    let module =
        compile_to_ir(source).map_err(|e| format!("complete.fv failed IR lowering: {e:?}"))?;

    // Enums
    let priority = module
        .enums
        .iter()
        .find(|e| e.name == "Priority")
        .ok_or("Priority enum missing from IR")?;
    if priority.variants.len() != 3 {
        return Err(format!(
            "Priority should have 3 variants, got {}",
            priority.variants.len()
        )
        .into());
    }
    let status = module
        .enums
        .iter()
        .find(|e| e.name == "Status")
        .ok_or("Status enum missing from IR")?;
    if status.variants.len() != 2 {
        return Err(format!(
            "Status should have 2 variants, got {}",
            status.variants.len()
        )
        .into());
    }

    // Traits
    if !module.traits.iter().any(|t| t.name == "Labeled") {
        return Err("Labeled trait missing from IR".into());
    }
    if !module.traits.iter().any(|t| t.name == "Tracked") {
        return Err("Tracked trait missing from IR".into());
    }

    // Structs
    let task = module
        .structs
        .iter()
        .find(|s| s.name == "Task")
        .ok_or("Task struct missing from IR")?;
    if task.fields.len() < 8 {
        return Err(format!(
            "Task should have at least 8 fields, got {}",
            task.fields.len()
        )
        .into());
    }
    let notes = task
        .fields
        .iter()
        .find(|f| f.name == "notes")
        .ok_or("notes field missing")?;
    if !notes.optional {
        return Err("notes field should be optional".into());
    }
    let retry = task
        .fields
        .iter()
        .find(|f| f.name == "retry_count")
        .ok_or("retry_count missing")?;
    if !retry.mutable {
        return Err("retry_count should be mutable".into());
    }

    // Standalone functions
    if !module.functions.iter().any(|f| f.name == "clamp") {
        return Err("clamp function missing from IR".into());
    }
    if !module.functions.iter().any(|f| f.name == "score_label") {
        return Err("score_label function missing from IR".into());
    }

    // Impl block
    let task_struct_id = module
        .structs
        .iter()
        .position(|s| s.name == "Task")
        .ok_or("Task struct not found")? as u32;
    let task_impl = module
        .impls
        .iter()
        .find(
            |i| matches!(i.target, formalang::ir::ImplTarget::Struct(id) if id.0 == task_struct_id),
        )
        .ok_or("Task impl block missing from IR")?;
    for expected in ["is_done", "priority_score", "describe", "next_retry"] {
        if !task_impl.functions.iter().any(|f| f.name == expected) {
            return Err(format!("Task impl missing method '{expected}'").into());
        }
    }

    // Module-level lets
    for expected in ["max_retries", "task_count", "sample", "cfg", "final_score"] {
        if !module.lets.iter().any(|l| l.name == expected) {
            return Err(format!("let binding '{expected}' missing from IR").into());
        }
    }
    let task_count = module.lets.iter().find(|l| l.name == "task_count").unwrap();
    if !task_count.mutable {
        return Err("task_count should be mutable".into());
    }

    Ok(())
}
