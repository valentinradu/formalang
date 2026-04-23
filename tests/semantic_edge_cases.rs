//! Semantic analyzer edge case tests

use formalang::CompilerError;

// =============================================================================
// Visibility Tests
// =============================================================================


fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_pub_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub trait Visible {
            value: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_pub_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub enum PublicStatus {
            active,
            inactive
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_pub_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod api {
            pub struct Endpoint {
                path: String
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Field Type Tests
// =============================================================================

#[test]
fn test_optional_field_with_default() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            name: String? = nil
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_mutable_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct State {
            mut current: String?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_of_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Items {
            values: [String?]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_optional_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Container {
            items: [String]?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Type Tests
// =============================================================================

#[test]
fn test_nested_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Container {
            nested: Box<Box<String>>
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_of_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T> {
            item: T
        }
        struct List {
            items: [Wrapper<String>]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_tuple_with_optionals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            point: (x: Number, y: Number?)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Expression Edge Cases
// =============================================================================

#[test]
fn test_nested_arrays() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let matrix = [[1, 2], [3, 4]]
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_empty_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let empty = []
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_grouped_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let grouped = (1 + 2) * 3
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_complex_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let result = 1 + 2 * 3 - 4 / 2
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_logical_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let result = true || false && true
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_comparison_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a = 1 < 2
        let b = 2 > 1
        let c = a && b
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct Instantiation Tests
// =============================================================================

#[test]
fn test_struct_instantiation_with_type_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Box<T> {
            value: T
        }
        struct Container {
            box: Box<String> = Box<String>(value: "hello")
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_instantiation_shorthand() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point {
            x: Number,
            y: Number
        }
        struct Line {
            start: Point = Point(x: 0, y: 0),
            end: Point
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Impl Block Tests
// =============================================================================

#[test]
fn test_impl_with_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Item {
            name: String
        }
        struct List {
            items: [Item] = [Item(name: "a"), Item(name: "b")]
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Result {
            value: String = if true { "yes" } else { "no" }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_with_for() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item {
            id: Number
        }
        struct Collection {
            items: [Item] = for i in [1, 2, 3] { Item(id: i) }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct Field Tests
// =============================================================================

#[test]
fn test_struct_with_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item {
            value: String
        }
        struct Container {
            items: [Item]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_trait_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Renderable {
            content: String
        }
        struct View {
            body: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Dictionary Tests
// =============================================================================

#[test]
fn test_dictionary_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Cache {
            data: [String: Number] = ["a": 1, "b": 2]
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let nested = ["outer": ["inner": "value"]]
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Type Tests
// =============================================================================

#[test]
fn test_closure_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            onEvent: String -> Boolean
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct EventBus {
            handlers: [() -> String]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Error Recovery Tests
// =============================================================================

#[test]
fn test_multiple_errors() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Unknown1 }
        struct B { y: Unknown2 }
        struct C { z: Unknown3 }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if errors.len() < 3 {
        return Err(format!("Expected >= 3 errors, got {}: {errors:?}", errors.len()).into());
    }
    if !errors
        .iter()
        .all(|e| matches!(e, CompilerError::UndefinedType { .. }))
    {
        return Err(format!("Expected UndefinedType errors: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_partial_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Valid {
            name: String
        }
        struct Invalid {
            data: MissingType
        }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::UndefinedType { name, .. } if name == "MissingType"))
    {
        return Err(format!("Expected UndefinedType for MissingType: {errors:?}").into());
    }
    Ok(())
}
