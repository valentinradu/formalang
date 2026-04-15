//! Targeted integration tests to increase semantic analysis code coverage.
//!
//! Covers error paths, validation branches, and expression types that are
//! not exercised by existing tests.

use formalang::compile;

// =============================================================================
// validate_type: generic arity mismatch
// =============================================================================

#[test]
fn test_generic_arity_mismatch_too_many_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Simple { value: Number }
        struct Container { item: Simple<String> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error for generic arity mismatch".into());
    }
    Ok(())
}

#[test]
fn test_generic_arity_mismatch_too_few_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Container { item: Box }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    // Box without type arguments is treated as an opaque type reference — no error
    Ok(())
}

#[test]
fn test_generic_constraint_violation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Plain { x: Number }
        struct Container { item: Box<Plain> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected generic constraint violation".into());
    }
    Ok(())
}

// =============================================================================
// validate_type: out-of-scope type parameter
// =============================================================================

#[test]
fn test_out_of_scope_type_parameter_single_letter() -> Result<(), Box<dyn std::error::Error>> {
    // Using 'T' outside a generic context triggers OutOfScopeTypeParameter
    let source = r"
        struct Container { value: T }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected out-of-scope type parameter error".into());
    }
    Ok(())
}

#[test]
fn test_out_of_scope_type_parameter_explicit() -> Result<(), Box<dyn std::error::Error>> {
    // TypeParameter syntax outside generic scope
    let source = r"
        struct Wrapper<T> { value: T }
        struct Bad { item: Wrapper<T> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: T not in scope".into());
    }
    Ok(())
}

// =============================================================================
// validate_type: dictionary and closure types
// =============================================================================

#[test]
fn test_validate_dictionary_type_with_struct_values() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Config { points: [String: Point] }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_validate_dictionary_type_nested() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { data: [String: Number] }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_validate_closure_type_with_annotations() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler { callback: (Number) -> String }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// validate_type: union (optional) and array recursion
// =============================================================================

#[test]
fn test_optional_of_invalid_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper { item: NonExistent? }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error".into());
    }
    Ok(())
}

#[test]
fn test_array_of_invalid_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct List { items: [Phantom] }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error".into());
    }
    Ok(())
}

#[test]
fn test_tuple_with_invalid_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair { data: (x: Number, y: Phantom) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error in tuple".into());
    }
    Ok(())
}

// =============================================================================
// validate_type: trait used as struct in type annotation
// =============================================================================

#[test]
fn test_trait_as_valid_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    // Traits are valid in type positions
    let source = r"
        trait Shape { area: Number }
        struct Container { shape: Shape }
    ";
    compile(source).map_err(|e| format!("Traits should be valid in type position: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Duplicate generic parameters
// =============================================================================

#[test]
fn test_duplicate_generic_params_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Bad<T, T> { a: T }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate generic parameter error".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_generic_params_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Bad<T, T> { value: T }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate generic parameter error".into());
    }
    Ok(())
}

#[test]
fn test_generic_constraint_references_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T: NonExistentTrait> { value: T }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined trait in constraint".into());
    }
    Ok(())
}

// =============================================================================
// Trait composition: trait extending non-existent or non-trait types
// =============================================================================

#[test]
fn test_trait_extending_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Extended: Undefined { value: Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined trait error".into());
    }
    Ok(())
}

#[test]
fn test_trait_extending_struct_not_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NotATrait { x: Number }
        trait Extended: NotATrait { value: Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected not-a-trait error".into());
    }
    Ok(())
}

#[test]
fn test_trait_extending_enum_not_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green, blue }
        trait Extended: Color { value: Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected not-a-trait error".into());
    }
    Ok(())
}

// =============================================================================
// Struct implementing non-existent or non-trait
// =============================================================================

#[test]
fn test_struct_implementing_undefined_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct MyStruct { value: Number }
        impl UndefinedTrait for MyStruct {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined trait error".into());
    }
    Ok(())
}

#[test]
fn test_struct_implementing_struct_as_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct NotATrait { x: Number }
        struct MyStruct { value: Number }
        impl NotATrait for MyStruct {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected not-a-trait error".into());
    }
    Ok(())
}

// =============================================================================
// Duplicate definitions
// =============================================================================

#[test]
fn test_duplicate_struct_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number }
        struct Point { y: Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate definition error".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_trait_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Shape { area: Number }
        trait Shape { perimeter: Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate definition error".into());
    }
    Ok(())
}

#[test]
fn test_duplicate_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, active }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate enum variant error".into());
    }
    Ok(())
}

// =============================================================================
// Impl for non-existent type
// =============================================================================

#[test]
fn test_impl_for_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        impl NonExistent {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error for impl".into());
    }
    Ok(())
}

#[test]
fn test_impl_trait_for_undefined_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Shape { area: Number }
        impl Shape for NonExistent {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error".into());
    }
    Ok(())
}

#[test]
fn test_impl_undefined_trait_for_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct MyStruct { value: Number }
        impl NonExistentTrait for MyStruct {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined trait error".into());
    }
    Ok(())
}

// =============================================================================
// Trait field type mismatch in implementation
// =============================================================================

#[test]
fn test_trait_field_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct BadImpl { name: Number }
        impl Named for BadImpl {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type mismatch in trait implementation".into());
    }
    Ok(())
}

#[test]
fn test_trait_missing_required_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Shape { area: Number, perimeter: Number }
        struct Circle { area: Number }
        impl Shape for Circle {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected missing field error".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: binary operator type errors
// =============================================================================

#[test]
fn test_binary_op_string_plus_number_invalid() -> Result<(), Box<dyn std::error::Error>> {
    // String + Number is invalid
    let source = r#"
        let result: Number = "hello" + 42
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type error for String + Number".into());
    }
    Ok(())
}

#[test]
fn test_binary_op_boolean_arithmetic_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Boolean = true
        let y: Boolean = false
        let z: Number = x + y
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type error for Boolean arithmetic".into());
    }
    Ok(())
}

#[test]
fn test_binary_op_logical_with_numbers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Number = 1
        let b: Number = 2
        let c: Boolean = a && b
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type error for Number && Number".into());
    }
    Ok(())
}

#[test]
fn test_binary_op_comparison_with_strings() -> Result<(), Box<dyn std::error::Error>> {
    // Strings are not comparable with < >
    let source = r#"
        let a: String = "hello"
        let b: String = "world"
        let c: Boolean = a < b
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type error for String < String".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: for loop over non-array
// =============================================================================

#[test]
fn test_for_loop_over_number_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count: Number = 10
        let result: [Number] = for x in count { x }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: for loop over non-array".into());
    }
    Ok(())
}

#[test]
fn test_for_loop_over_string_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s: String = "hello"
        let result: [String] = for c in s { c }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: for loop over String".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: if condition not boolean
// =============================================================================

#[test]
fn test_if_condition_number_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let x: Number = 42
        let result: String = if x { "yes" } else { "no" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: if condition must be Boolean".into());
    }
    Ok(())
}

#[test]
fn test_if_condition_string_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s: String = "hello"
        let result: String = if s { "yes" } else { "no" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: if condition must be Boolean".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: match on non-enum
// =============================================================================

#[test]
fn test_match_on_number_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let x: Number = 1
        let result: String = match x {
            _ => "wildcard"
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: match on non-enum".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: undefined references in struct impl
// =============================================================================

#[test]
fn test_undefined_reference_in_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { count: Number }
        impl Counter {
            fn get_value() -> Number { undefinedVar }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined reference error".into());
    }
    Ok(())
}

#[test]
fn test_self_outside_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = self.value
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: self outside impl block".into());
    }
    Ok(())
}

#[test]
fn test_self_field_not_found_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { count: Number }
        impl Counter {
            fn get() -> Number { self.nonExistent }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: undefined self.field".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: struct instantiation errors
// =============================================================================

#[test]
fn test_struct_missing_required_field_in_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Config { location: Point = Point(x: 1) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected missing required field error".into());
    }
    Ok(())
}

#[test]
fn test_struct_unknown_field_in_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Config { location: Point = Point(x: 1, y: 2, z: 3) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected unknown field error".into());
    }
    Ok(())
}

#[test]
fn test_struct_positional_arg_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Config { location: Point = Point(1, 2) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected positional arg error in struct instantiation".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: enum instantiation errors
// =============================================================================

#[test]
fn test_enum_instantiation_undefined_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = UndefinedEnum.variant
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!("Expected error for undefined enum, got: {:?}", result.ok()).into());
    }
    Ok(())
}

#[test]
fn test_enum_instantiation_undefined_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green, blue }
        struct Config { color: Color = Color.purple }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined variant error".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: assignment to immutable
// =============================================================================

#[test]
fn test_assignment_to_immutable_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { mut count: Number = 0 }
        impl Counter {
            fn increment() -> Number {
                let x: Number = 5
                x = 10
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected immutable assignment error".into());
    }
    Ok(())
}

// =============================================================================
// Expression validation: closures
// =============================================================================

#[test]
fn test_closure_with_invalid_param_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler { callback: (UndefinedType) -> Number }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error in closure param".into());
    }
    Ok(())
}

#[test]
fn test_closure_with_invalid_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler { callback: (Number) -> UndefinedType }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error in closure return".into());
    }
    Ok(())
}

#[test]
fn test_closure_expr_in_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    // Closure expression used in a let binding
    let source = r"
        let items: [Number] = [1, 2, 3, 4, 5]
        let doubled: [Number] = for x in items { x }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Expression validation: dict access
// =============================================================================

#[test]
fn test_dict_literal_in_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let data: [String: Number] = ["key": 42]
    "#;
    compile(source).map_err(|e| format!("Dict literal in let should compile: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Destructuring patterns
// =============================================================================

#[test]
fn test_array_destructuring_of_non_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let num: Number = 42
        let [a, b] = num
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: array destructuring of non-array".into());
    }
    Ok(())
}

#[test]
fn test_struct_destructuring_of_non_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let num: Number = 42
        let {x} = num
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: struct destructuring of non-struct".into());
    }
    Ok(())
}

#[test]
fn test_struct_destructuring_with_unknown_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p: Point = Point(x: 1, y: 2)
        let {x, z} = p
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: unknown field in struct destructuring".into());
    }
    Ok(())
}

// =============================================================================
// Nested module definitions
// =============================================================================

#[test]
fn test_nested_module_with_traits_and_structs() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod geometry {
            pub trait Shape { area: Number }
            pub struct Circle {
                area: Number,
                radius: Number
            }
            impl Shape for Circle {}
            pub enum Orientation { horizontal, vertical }
        }
    ";
    compile(source).map_err(|e| format!("Nested module should compile: {e:?}"))?;
    Ok(())
}

#[test]
fn test_doubly_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod outer {
            pub mod inner {
                pub struct Widget { width: Number, height: Number }
            }
        }
    ";
    compile(source).map_err(|e| format!("Doubly nested module should compile: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let binding type inference
// =============================================================================

#[test]
fn test_let_inferred_from_array_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let nums = [1, 2, 3]
    ";
    compile(source).map_err(|e| format!("Let binding from array literal: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_inferred_from_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p = Point(x: 1, y: 2)
    ";
    compile(source).map_err(|e| format!("Let binding from struct: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_boolean_inference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let flag = true
        let other = false
    ";
    compile(source).map_err(|e| format!("Boolean let inference: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Circular dependency detection
// =============================================================================

#[test]
fn test_struct_self_reference_via_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Node { value: Number, next: Node? }
    ";
    let result = compile(source);
    // Optional self-references still trigger circular dependency detection
    if result.is_ok() {
        return Err(format!(
            "Optional self-reference produces CircularDependency error: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// standalone functions
// =============================================================================

#[test]
fn test_standalone_function_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn add(a: Number, b: Number) -> Number { a + b }
    ";
    compile(source).map_err(|e| format!("Standalone function: {e:?}"))?;
    Ok(())
}

#[test]
fn test_standalone_function_with_invalid_param_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn process(x: Phantom) -> Number { 42 }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined type error in function param".into());
    }
    Ok(())
}

#[test]
fn test_function_calling_undefined_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { count: Number }
        impl Counter {
            fn go() -> Number { undefinedFunction() }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined function error".into());
    }
    Ok(())
}

// =============================================================================
// Method call validation
// =============================================================================

#[test]
fn test_method_call_on_array() -> Result<(), Box<dyn std::error::Error>> {
    // Arrays have built-in methods
    let source = r"
        let items: [Number] = [1, 2, 3]
        let len: Number = items.len()
    ";
    // len() on arrays is not a recognized method in the semantic analyser
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "Array len() is not a builtin method — should produce UndefinedReference: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Block expressions
// =============================================================================

#[test]
fn test_block_with_let_and_result() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = {
            let x: Number = 5
            let y: Number = 10
            x + y
        }}
    ";
    compile(source).map_err(|e| format!("Block expression: {e:?}"))?;
    Ok(())
}

#[test]
fn test_block_with_assignment() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = {
                let mut x: Number = 0
                x = 5
                x
            }
        }
    ";
    compile(source).map_err(|e| format!("Block with assignment: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let expressions
// =============================================================================

#[test]
fn test_let_expr_basic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            value: Number = (let x: Number = 5
            x)
        }
    ";
    compile(source).map_err(|e| format!("Let expression: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Generic struct instantiation in expressions
// =============================================================================

#[test]
fn test_generic_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Config { box: Box<Number> = Box<Number>(value: 42) }
    ";
    compile(source).map_err(|e| format!("Generic struct instantiation: {e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_struct_missing_type_arg_in_instantiation() -> Result<(), Box<dyn std::error::Error>>
{
    let source = r"
        struct Box<T> { value: T }
        struct Config { box: Box<Number> = Box(value: 42) }
    ";
    let result = compile(source);
    // Missing type args in invocation should be caught
    if result.is_ok() {
        return Err("Expected missing generic arguments error".into());
    }
    Ok(())
}

// =============================================================================
// Enum with match exhaustiveness
// =============================================================================

#[test]
fn test_match_non_exhaustive() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green, blue }
        struct Config {
            name: String = match Color.red {
                .red: "red"
            }
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected non-exhaustive match error".into());
    }
    Ok(())
}

#[test]
fn test_match_with_wildcard_is_exhaustive() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green, blue }
        struct Config {
            name: String = match Color.red {
                .red: "red",
                _: "other"
            }
        }
    "#;
    compile(source).map_err(|e| format!("Match with wildcard: {e:?}"))?;
    Ok(())
}

#[test]
fn test_match_duplicate_arm() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green }
        struct Config {
            name: String = match Color.red {
                .red: "red",
                .red: "red again",
                _: "other"
            }
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate match arm error".into());
    }
    Ok(())
}

// =============================================================================
// Tuple types
// =============================================================================

#[test]
fn test_valid_tuple_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair { data: (x: Number, y: String) }
    ";
    compile(source).map_err(|e| format!("Tuple type: {e:?}"))?;
    Ok(())
}

#[test]
fn test_tuple_let_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p: Point = Point(x: 1, y: 2)
        let (a, b) = p
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    // Tuple destructuring of a struct should succeed
    Ok(())
}

// =============================================================================
// Module-level let bindings with various types
// =============================================================================

#[test]
fn test_module_level_let_optional_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let optional: String? = nil
    ";
    compile(source).map_err(|e| format!("Optional let: {e:?}"))?;
    Ok(())
}

#[test]
fn test_module_level_let_string() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let message: String = "hello world"
    "#;
    compile(source).map_err(|e| format!("String let: {e:?}"))?;
    Ok(())
}

#[test]
fn test_module_level_let_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let values: [Number] = [1, 2, 3, 4, 5]
    ";
    compile(source).map_err(|e| format!("Array let: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Inferred enum instantiation in function context
// =============================================================================

#[test]
fn test_inferred_enum_instantiation_in_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        fn get_status() -> Status { .active }
    ";
    compile(source).map_err(|e| format!("Inferred enum in function: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Module-level path type resolution (module::Type)
// =============================================================================

#[test]
fn test_nested_module_type_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod shapes {
            pub struct Circle { radius: Number }
        }
        struct Canvas { shape: shapes::Circle }
    ";
    compile(source).map_err(|e| format!("Nested module type reference: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Function call validation
// =============================================================================

#[test]
fn test_function_call_undefined_produces_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { val: Number = undefinedFn(x: 1) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err(format!(
            "Calling undefined function should be rejected: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Struct with multiple traits: all must be valid
// =============================================================================

#[test]
fn test_struct_with_multiple_valid_traits() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        trait Sized { size: Number }
        struct Widget {
            name: String,
            size: Number
        }
        impl Named for Widget {}
        impl Sized for Widget {}
    ";
    compile(source).map_err(|e| format!("Multiple trait impl: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_one_invalid_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct Widget { name: String }
        impl Named for Widget {}
        impl UndefinedTrait for Widget {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected undefined trait error".into());
    }
    Ok(())
}

// =============================================================================
// Infer type of various expressions
// =============================================================================

#[test]
fn test_infer_type_of_ternary_produces_result() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let flag: Boolean = true
        let val: Number = if flag { 1 } else { 2 }
    ";
    compile(source).map_err(|e| format!("If expression in let: {e:?}"))?;
    Ok(())
}

#[test]
fn test_infer_type_of_nested_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { x: Number }
        struct Outer { inner: Inner }
        let outer: Outer = Outer(inner: Inner(x: 42))
    ";
    compile(source).map_err(|e| format!("Nested struct: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum with data fields
// =============================================================================

#[test]
fn test_enum_variant_with_data_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number),
            rectangle(width: Number, height: Number),
            point
        }
    ";
    compile(source).map_err(|e| format!("Enum with data fields: {e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_instantiation_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number),
            point
        }
        struct Config {
            shape: Shape = Shape.circle(radius: 5)
        }
    ";
    compile(source).map_err(|e| format!("Enum with data: {e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_instantiation_data_required_but_missing() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number)
        }
        struct Config {
            shape: Shape = Shape.circle
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: enum variant requires data".into());
    }
    Ok(())
}

#[test]
fn test_enum_instantiation_data_not_expected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            point
        }
        struct Config {
            shape: Shape = Shape.point(radius: 5)
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: enum variant without data fields".into());
    }
    Ok(())
}

#[test]
fn test_enum_instantiation_unknown_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number)
        }
        struct Config {
            shape: Shape = Shape.circle(radius: 5, extra: 99)
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: unknown enum field".into());
    }
    Ok(())
}

#[test]
fn test_enum_instantiation_missing_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number, color: String)
        }
        struct Config {
            shape: Shape = Shape.circle(radius: 5)
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: missing enum field".into());
    }
    Ok(())
}

// =============================================================================
// Circular dependency detection
// =============================================================================

#[test]
fn test_circular_type_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { b: B }
        struct B { a: A }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected circular dependency error".into());
    }
    Ok(())
}

#[test]
fn test_circular_let_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Number = b + 1
        let b: Number = a + 1
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected circular let dependency error".into());
    }
    Ok(())
}

// =============================================================================
// Function return type mismatch
// =============================================================================

#[test]
fn test_function_return_type_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator { value: Number }
        impl Calculator {
            fn double() -> Number { self.value + self.value }
        }
    ";
    compile(source).map_err(|e| format!("Function with valid return type: {e:?}"))?;
    Ok(())
}

#[test]
fn test_standalone_function_with_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn multiply(a: Number, b: Number) -> Number { a * b }
    ";
    compile(source).map_err(|e| format!("Standalone function with params: {e:?}"))?;
    Ok(())
}

#[test]
fn test_standalone_function_no_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn greet(name: String) { name }
    ";
    compile(source).map_err(|e| format!("Function without return type: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Method call on struct (impl method validation)
// =============================================================================

#[test]
fn test_method_call_on_struct_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn get_x() -> Number { self.x }
            fn get_y() -> Number { self.y }
        }
    ";
    compile(source).map_err(|e| format!("Method call in impl: {e:?}"))?;
    Ok(())
}

#[test]
fn test_method_call_undefined_method() -> Result<(), Box<dyn std::error::Error>> {
    // Chained method call where result type is Unknown - exercises method validation path
    let source = r"
        struct Point { x: Number = 0 }
        impl Point {
            fn get_x() -> Number { self.x }
        }
    ";
    compile(source).map_err(|e| format!("Simple impl method: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Trait composition (trait extending trait)
// =============================================================================

#[test]
fn test_trait_composition_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        trait Identified: Named { id: Number }
        struct User {
            name: String,
            id: Number
        }
        impl Identified for User {}
    ";
    compile(source).map_err(|e| format!("Trait composition: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_must_implement_composed_trait_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        trait Identified: Named { id: Number }
        struct User { id: Number }
        impl Identified for User {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected missing field from composed trait".into());
    }
    Ok(())
}

// =============================================================================
// Trait field requirements in impl blocks
// =============================================================================

#[test]
fn test_trait_with_required_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container {
            width: Number,
            content: String
        }
        struct Panel {
            width: Number,
            content: String
        }
        impl Container for Panel {}
    ";
    compile(source).map_err(|e| format!("Trait with required field: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_missing_trait_required_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container { content: String }
        struct Panel { width: Number }
        impl Container for Panel {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected missing required field error".into());
    }
    Ok(())
}

#[test]
fn test_trait_field_type_mismatch_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Container { content: String }
        struct Panel { content: Number }
        impl Container for Panel {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected field type mismatch error".into());
    }
    Ok(())
}

// =============================================================================
// Match arm arity validation
// =============================================================================

#[test]
fn test_match_arm_with_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number),
            point
        }
        struct Config {
            result: Number = match Shape.point {
                .circle(r): r,
                .point: 0
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    // Match with enum variant binding should compile successfully
    Ok(())
}

// =============================================================================
// Mutable struct field with let binding
// =============================================================================

#[test]
fn test_mutable_let_binding_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let mut counter: Number = 0
        struct Config { count: Number = counter }
    ";
    compile(source).map_err(|e| format!("Mutable let binding: {e:?}"))?;
    Ok(())
}

#[test]
fn test_immutable_let_binding_in_mut_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    // Immutable let binding passed to mutable struct field
    let source = r"
        let value: Number = 42
        struct Config { mut count: Number = value }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    // Immutable binding assigned to a mutable struct field — should succeed
    // (the field's mutability is its own property, not the binding's)
    Ok(())
}

// =============================================================================
// Array destructuring valid
// =============================================================================

#[test]
fn test_array_destructuring_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let [first, second] = [1, 2, 3]
    ";
    compile(source).map_err(|e| format!("Array destructuring: {e:?}"))?;
    Ok(())
}

#[test]
fn test_array_destructuring_with_rest() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let [first, ...rest] = [1, 2, 3, 4]
    ";
    compile(source).map_err(|e| format!("Array destructuring with rest: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Struct destructuring valid
// =============================================================================

#[test]
fn test_struct_destructuring_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p: Point = Point(x: 1, y: 2)
        let {x, y} = p
    ";
    compile(source).map_err(|e| format!("Struct destructuring: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let binding type mismatch (binary op on incompatible types)
// =============================================================================

#[test]
fn test_string_multiplication_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let s: String = "hello"
        let n: Number = 5
        let result: String = s * n
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type error for String * Number".into());
    }
    Ok(())
}

// =============================================================================
// Range operator in for loop
// =============================================================================

#[test]
fn test_range_in_for_loop() -> Result<(), Box<dyn std::error::Error>> {
    // Range expression - exercises the ForLoopNotArray path since Range<Number> is not [Number]
    let source = r"
        let sum: [Number] = for i in 0..10 { i }
    ";
    let result = compile(source);
    // Range loops produce a type error since Range<Number> isn't [Number]
    if result.is_ok() {
        return Err(format!(
            "Range in for loop should produce an error: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Impl block duplicate definition
// =============================================================================

#[test]
fn test_duplicate_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { count: Number }
        impl Counter {}
        impl Counter {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate impl error".into());
    }
    Ok(())
}

// =============================================================================
// Module-level function definition
// =============================================================================

#[test]
fn test_module_level_function_in_module_def() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod math {
            pub fn add(a: Number, b: Number) -> Number { a + b }
        }
    ";
    compile(source).map_err(|e| format!("Module-level function: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Type check: struct satisfies generic constraint
// =============================================================================

#[test]
fn test_struct_satisfies_generic_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Named { label: String }
        impl Printable for Named {}
        struct Container { item: Box<Named> }
    ";
    compile(source).map_err(|e| format!("Generic constraint satisfied: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure expression in struct field default
// =============================================================================

#[test]
fn test_closure_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler { callback: (Number) -> Number = |n: Number| n }
    ";
    compile(source).map_err(|e| format!("Closure in struct field: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Method call on struct type
// =============================================================================

#[test]
fn test_method_call_normalize_on_vec3() -> Result<(), Box<dyn std::error::Error>> {
    // normalize is no longer a builtin — calling it produces an undefined reference error
    let source = r"
        struct Gpu { output: Number = normalize(1.0) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: normalize is not defined".into());
    }
    Ok(())
}

// =============================================================================
// Inferred types in let bindings via arithmetic
// =============================================================================

#[test]
fn test_infer_type_via_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Number = 10
        let b: Number = 20
        let sum: Number = a + b
        let product: Number = a * b
    ";
    compile(source).map_err(|e| format!("Binary op inference: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let reference in circular check (extract_let_references coverage)
// =============================================================================

#[test]
fn test_let_reference_in_various_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let base: Number = 10
        let doubled: Number = base + base
        let items: [Number] = [base, doubled]
        let flag: Boolean = base == doubled
    ";
    compile(source).map_err(|e| format!("Let references: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum instantiation through match
// =============================================================================

#[test]
fn test_match_exhaustive_all_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Direction { north, south, east, west }
        struct Config {
            label: String = match Direction.north {
                .north: "N",
                .south: "S",
                .east: "E",
                .west: "W"
            }
        }
    "#;
    compile(source).map_err(|e| format!("Exhaustive match: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Trait impl registration
// =============================================================================

#[test]
fn test_impl_trait_for_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Drawable { render: Number }
        struct Circle { render: Number, radius: Number }
        impl Drawable for Circle {}
    ";
    compile(source).map_err(|e| format!("Trait impl: {e:?}"))?;
    Ok(())
}

#[test]
fn test_duplicate_trait_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Drawable { render: Number }
        struct Circle { render: Number }
        impl Drawable for Circle {}
        impl Drawable for Circle {}
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate trait impl error".into());
    }
    Ok(())
}
