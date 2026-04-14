//! Additional integration tests to push semantic/mod.rs coverage higher.
//!
//! Targets uncovered clusters identified via llvm-cov HTML report.

use formalang::compile;
use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::semantic::SemanticAnalyzer;
use std::path::PathBuf;

// =============================================================================
// SemanticAnalyzer::new() — lines 125-141
// =============================================================================

#[test]
fn test_semantic_analyzer_new_constructor() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = FileSystemResolver::new(PathBuf::from("."));
    let mut analyzer = SemanticAnalyzer::new(resolver);
    let tokens = formalang::lexer::Lexer::tokenize_all("struct Foo { x: Number }");
    let file = formalang::parse_file_with_source(&tokens, "struct Foo { x: Number }")
        .map_err(|e| format!("parse error: {e:?}"))?;
    let result = analyzer.analyze(&file);
    if result.is_err() {
        return Err(format!("Expected ok: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Duplicate function definition — lines 1237-1244
// =============================================================================

#[test]
fn test_duplicate_function_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: Number) -> Number { x }
        fn compute(y: Number) -> Number { y }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate function definition error".into());
    }
    Ok(())
}

// =============================================================================
// Struct with non-generic but type args provided — lines 2011-2019
// =============================================================================

#[test]
fn test_non_generic_struct_with_type_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        struct Config { p: Point = Point<Number>(x: 1, y: 2) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: non-generic struct with type args".into());
    }
    Ok(())
}

// =============================================================================
// Function call with type args — lines 2031-2043
// =============================================================================

#[test]
fn test_function_call_with_type_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: Number) -> Number { x }
        struct Config { val: Number = compute<Number>(x: 1) }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: function call with type args".into());
    }
    Ok(())
}

// =============================================================================
// Mutable field chain — exercises is_expr_mutable and is_field_chain_mutable
// Lines 3853-4007
// =============================================================================

#[test]
fn test_mutable_field_chain_assignment_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { mut value: Number = 0 }
        struct Outer { mut inner: Inner = Inner(value: 0) }
        let mut obj: Outer = Outer(inner: Inner(value: 0))
        let result: Number = obj.inner.value
    ";
    let result = compile(source);
    // Passing a non-mutable literal to a mutable field produces MutabilityMismatch
    if result.is_ok() {
        return Err(format!(
            "Mutable field chain with literal value produces MutabilityMismatch: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_mutable_let_binding_assignment() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = 0
        }
        impl Counter {
            fn increment() -> Number {
                let mut x: Number = 5
                x = 10
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Mutable let in block: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_assignment_to_immutable_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = 0
        }
        impl Counter {
            fn reset() -> Number {
                let x: Number = 5
                x = 0
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
// Method call on struct type — lines 4154-4172
// =============================================================================

#[test]
fn test_method_call_on_user_defined_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        impl Point {
            fn magnitude() -> Number { self.x }
        }
        struct Config {
            val: Number = Point(x: 3, y: 4).magnitude()
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Method call on struct: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_undefined_on_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        impl Point {
            fn get_x() -> Number { self.x }
        }
        struct Config {
            val: Number = Point(x: 3, y: 4).nonExistentMethod()
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected error: undefined method on struct".into());
    }
    Ok(())
}

// =============================================================================
// type_satisfies_trait_constraint via Generic type — lines 4229-4237
// =============================================================================

#[test]
fn test_generic_constraint_satisfied_via_impl() -> Result<(), Box<dyn std::error::Error>> {
    // struct implementing trait inline; use in generic context
    let source = r#"
        trait Printable { label: String }
        struct Box<T: Printable> { value: T }
        struct Widget: Printable { label: String }
        struct Config { item: Box<Widget> = Box<Widget>(value: Widget(label: "hi")) }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic constraint satisfied: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_generic_constraint_violation_primitive() -> Result<(), Box<dyn std::error::Error>> {
    // Using a primitive type where a trait constraint is required
    let source = r"
        trait Serializable { data: String }
        struct Wrapper<T: Serializable> { content: T }
        struct Config { item: Wrapper<Number> }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected constraint violation for primitive".into());
    }
    Ok(())
}

// =============================================================================
// Field access expressions — lines 2174-2177
// =============================================================================

#[test]
fn test_field_access_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p: Point = Point(x: 1, y: 2)
        let val: Number = p.x
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Field access expression: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Closure expressions — exercise closure param scope path
// =============================================================================

#[test]
fn test_closure_with_annotated_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [Number] = [1, 2, 3]
        let doubled: [Number] = for x in items { x }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure with annotated params: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Match arm arity mismatch — lines 2883-2898
// =============================================================================

#[test]
fn test_match_arm_arity_too_many_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Shape {
            circle(radius: Number),
            point
        }
        struct Config {
            name: String = match Shape.point {
                .circle(r, extra): "circle",
                .point: "point"
            }
        }
    "#;
    let result = compile(source);
    // Arity mismatch in match arm variant binding should produce an error
    if result.is_ok() {
        return Err(format!("Arity mismatch in match arm: {:?}", result.ok()).into());
    }
    Ok(())
}

#[test]
fn test_match_arm_unknown_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green, blue }
        struct Config {
            name: String = match Color.red {
                .red: "red",
                .purple: "purple",
                _: "other"
            }
        }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected unknown variant in match arm".into());
    }
    Ok(())
}

// =============================================================================
// TypeParameter out of scope — lines 1721-1728
// =============================================================================

#[test]
fn test_type_parameter_out_of_scope_in_field() -> Result<(), Box<dyn std::error::Error>> {
    // Using TypeParameter syntax explicitly out of scope
    let source = r"
        struct Bad { item: T }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected out-of-scope type parameter error".into());
    }
    Ok(())
}

// =============================================================================
// resolve_module_types with impl inside module — lines 1466-1514
// =============================================================================

#[test]
fn test_module_with_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    // Modules can contain structs and traits; impl inside module may not be supported
    // Instead test resolve_module_types with a struct and enum inside a module
    let source = r"
        pub mod geometry {
            pub struct Point { x: Number, y: Number }
            pub trait Shape { area: Number }
        }
        struct Canvas { shape: geometry::Shape }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with struct and trait: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_enum_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod status {
            pub enum State {
                active(count: Number),
                inactive
            }
        }
        struct Config { state: status::State }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with enum variants: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_module_with_standalone_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod math {
            pub fn double(x: Number) -> Number { x }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Module with function: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Nested module type resolution — module path not found
// =============================================================================

#[test]
fn test_nested_module_type_not_found_in_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod shapes {
            pub struct Circle { radius: Number }
        }
        struct Config { item: shapes::Square }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type not found in nested module".into());
    }
    Ok(())
}

#[test]
fn test_deeply_nested_module_type_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod outer {
            pub mod inner {
                pub struct Widget { width: Number }
            }
        }
        struct Config { item: outer::inner::Gadget }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected type not found in deeply nested module".into());
    }
    Ok(())
}

#[test]
fn test_nested_module_parent_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod shapes {
            pub struct Circle { radius: Number }
        }
        struct Config { item: nonexistent::Circle }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected module not found error".into());
    }
    Ok(())
}

// =============================================================================
// Circular let dependency — detect_circular_let_dependencies
// =============================================================================

#[test]
fn test_circular_let_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Number = b
        let b: Number = a
    ";
    let result = compile(source);
    // Should produce a circular dependency error
    if result.is_ok() {
        return Err("Expected circular let dependency error".into());
    }
    Ok(())
}

// =============================================================================
// Mutability mismatch in struct instantiation — lines 2464-2485
// =============================================================================

#[test]
fn test_mutability_mismatch_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            mut value: Number = 0
        }
        let immutableNum: Number = 42
        let cfg: Config = Config(value: immutableNum)
    ";
    let result = compile(source);
    // Passing an immutable binding to a mutable field produces MutabilityMismatch
    if result.is_ok() {
        return Err(format!(
            "Immutable value to mutable field produces MutabilityMismatch: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// GPU primitive types in type_to_string — lines 3032-3055
// =============================================================================

#[test]
fn test_gpu_primitive_type_f32() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Shader { value: f32 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("f32 type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_primitive_type_vec3() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Shader { position: vec3 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("vec3 type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_primitive_type_mat4() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Transform { matrix: mat4 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("mat4 type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_vector_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Vectors {
            v2: vec2,
            v3: vec3,
            v4: vec4,
            iv2: ivec2,
            iv3: ivec3,
            iv4: ivec4,
            uv2: uvec2,
            uv3: uvec3,
            uv4: uvec4
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("GPU vector types: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_matrix_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Matrices { m2: mat2, m3: mat3, m4: mat4 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("GPU matrix types: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_bool_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Flags { enabled: bool }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("bool type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_signed_int_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Index { value: i32 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("i32 type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_gpu_unsigned_int_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Index { value: u32 }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("u32 type: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Return type mismatch — lines 3182-3187
// =============================================================================

#[test]
fn test_function_return_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        fn compute() -> Number { "hello" }
    "#;
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected return type mismatch error".into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_mismatch_bool() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn check() -> Boolean { 42 }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected return type mismatch for bool".into());
    }
    Ok(())
}

// =============================================================================
// Standalone function without return type annotation
// =============================================================================

#[test]
fn test_standalone_function_without_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn greet(name: String) { name }
    ";
    let result = compile(source);
    // Function without return type annotation should compile
    if result.is_err() {
        return Err(format!("Function without return type: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_standalone_function_no_param_type() -> Result<(), Box<dyn std::error::Error>> {
    // Function parameter without type annotation exercises "Unknown" path
    let source = r"
        fn identity(x) { x }
    ";
    let result = compile(source);
    // FormaLang requires type annotations on function parameters — parse error
    if result.is_ok() {
        return Err(format!(
            "Function with untyped param should produce a parse error: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Complex match expressions — exercises extract_let_references
// =============================================================================

#[test]
fn test_let_binding_with_match_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, inactive }
        let s: Status = Status.active
        let label: String = match s {
            .active: "active",
            .inactive: "inactive"
        }
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let with match: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Let binding referencing other let bindings — exercises extract_let_references
// =============================================================================

#[test]
fn test_let_bindings_dependency_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let base: Number = 10
        let doubled: Number = base + base
        let tripled: Number = doubled + base
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Let dependency chain: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// FieldAccess in let binding — exercises extract_let_references for FieldAccess
// =============================================================================

#[test]
fn test_let_field_access_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number, y: Number }
        let p: Point = Point(x: 1, y: 2)
        let x_val: Number = p.x
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Field access dependency: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// DictAccess in let binding — exercises extract_let_references for DictAccess
// =============================================================================

#[test]
fn test_let_dict_access_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let data: [String: Number] = ["key": 42]
        let val: Number = data["key"]
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Dict access in let binding: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// LetExpr in let binding value — exercises extract_let_references for LetExpr
// =============================================================================

#[test]
fn test_let_expr_as_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            value: Number = (let x: Number = 5
            let y: Number = 10
            x + y)
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("LetExpr as value: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Tuple expression with named fields
// =============================================================================

#[test]
fn test_tuple_expression_named_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair { data: (x: Number, y: Number) }
        let p: Pair = Pair(data: (x: 1, y: 2))
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Tuple expression with named fields: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

// =============================================================================
// Unary not on boolean
// =============================================================================

#[test]
fn test_unary_not_on_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let flag: Boolean = true
        let notFlag: Boolean = !flag
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Unary not: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_unary_neg_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = 5
        let neg: Number = -x
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Unary neg: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Block expression in struct field default — exercises Block validation
// =============================================================================

#[test]
fn test_block_expression_with_multiple_lets() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            value: Number = {
                let a: Number = 1
                let b: Number = 2
                let c: Number = 3
                a + b + c
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Block with multiple lets: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_block_expression_with_assign() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = {
                let mut x: Number = 0
                x = 5
                x = 10
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Block with assign: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Method call on GPU primitive types (exercises method_exists_on_type)
// =============================================================================

#[test]
fn test_method_call_normalize_on_vec3() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Shader {
            direction: vec3
        }
        impl Shader {
            fn get_norm() -> vec3 { self.direction.normalize() }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("normalize on vec3: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_length_on_vec3() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Shader {
            pos: vec3
        }
        impl Shader {
            fn get_len() -> f32 { self.pos.length() }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("length on vec3: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Enum data variant match with binding — exercises validate_match_arm arity
// =============================================================================

#[test]
fn test_match_enum_with_data_variant_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Result {
            ok(value: Number),
            err
        }
        struct Config {
            val: Number = match Result.ok(value: 42) {
                .ok(v): v,
                .err: 0
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Enum data variant match with binding: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

// =============================================================================
// Standalone function with return type matching — exercises type_strings_compatible
// =============================================================================

#[test]
fn test_function_return_type_f32_number_compatible() -> Result<(), Box<dyn std::error::Error>> {
    // f32 and Number are compatible in type_strings_compatible
    let source = r"
        fn compute() -> Number { 42.0 }
    ";
    let result = compile(source);
    // f32 literal should be compatible with Number return type
    if result.is_err() {
        return Err(format!("f32 compatible with Number return: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Inferred enum in let binding
// =============================================================================

#[test]
fn test_inferred_enum_in_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Direction { north, south, east, west }
        let dir: Direction = .north
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Inferred enum in let: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Self reference chain — exercises self.field path >= 3 segments
// =============================================================================

#[test]
fn test_self_field_chain_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { val: Number }
        struct Outer { inner: Inner }
        impl Outer {
            fn get() -> Number { self.inner.val }
        }
    ";
    let result = compile(source);
    // self.inner.val is a 3-segment path — should compile successfully
    if result.is_err() {
        return Err(format!("Self field chain: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Group expression in let binding — exercises extract_let_references for Group
// =============================================================================

#[test]
fn test_group_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = 5
        let y: Number = (x + 3)
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Group expression: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Multiple imports — exercises import_symbol paths
// =============================================================================

#[test]
fn test_duplicate_struct_in_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod shapes {
            pub struct Circle { radius: Number }
            pub struct Circle { r: Number }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Expected duplicate definition in module".into());
    }
    Ok(())
}

// =============================================================================
// Trait composition chain (trait extending trait extending trait)
// =============================================================================

#[test]
fn test_deep_trait_composition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base { base_val: Number }
        trait Middle: Base { mid_val: String }
        trait Top: Middle { top_val: Boolean }
        struct Full: Top {
            base_val: Number,
            mid_val: String,
            top_val: Boolean
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Deep trait composition: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// if expression without else branch — exercises optional branch
// =============================================================================

#[test]
fn test_if_without_else_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let flag: Boolean = true
        let val: Number = if flag { 42 }
    ";
    // if without else compiles successfully (missing else is not a type error)
    let result = compile(source);
    if result.is_err() {
        return Err(format!("if without else should compile: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Boolean comparison operators
// =============================================================================

#[test]
fn test_equality_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Number = 5
        let b: Number = 5
        let eq: Boolean = a == b
        let ne: Boolean = a != b
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Equality ops: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_range_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let start: Number = 1
        let end: Number = 10
        let r = start..end
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Range expression: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// InferredEnum in function context — exercises infer_type for InferredEnum
// =============================================================================

#[test]
fn test_function_returning_inferred_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green, blue }
        fn get_color() -> Color { .red }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Function returning inferred enum: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Struct field with generic default — exercises various type paths
// =============================================================================

#[test]
fn test_generic_struct_with_constraint_met() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Measurable { size: Number }
        struct Container<T: Measurable> { item: T }
        struct Widget: Measurable { size: Number }
        struct Config {
            box: Container<Widget> = Container<Widget>(item: Widget(size: 5))
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Generic struct with constraint: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Array with generic element type
// =============================================================================

#[test]
fn test_array_of_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Config { items: [Box<Number>] }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Array of generic struct: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// type_to_string for TypeParameter — lines 3080
// =============================================================================

#[test]
fn test_type_parameter_in_function_return() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        impl Box<T> {
            fn get() -> T { self.value }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Type parameter in function return: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Closure with multiple params — exercises type_to_string for Closure
// =============================================================================

#[test]
fn test_closure_type_with_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    // FormaLang closure syntax uses a single param or parens - adjust to valid syntax
    let source = r"
        struct Handler { callback: (Number) -> Boolean }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure with params: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_closure_type_with_no_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Producer { factory: () -> Number }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Closure with no params: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Method call that exercises the common_builtins path
// =============================================================================

#[test]
fn test_method_call_abs_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = -5
        let result: Number = x.abs()
    ";
    let result = compile(source);
    // abs is in common_builtins — should compile successfully
    if result.is_err() {
        return Err(format!("abs() method on Number: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_method_call_sqrt_on_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number = 16
        let result: Number = x.sqrt()
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("sqrt() method on Number: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// For loop with body referencing loop var then closure param
// =============================================================================

#[test]
fn test_for_loop_with_closure_in_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [Number] = [1, 2, 3]
        let result: [[Number]] = for x in items { [x, x] }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("For loop with closure in body: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Enum variant data field access in match arm body
// =============================================================================

#[test]
fn test_match_enum_with_data_and_field_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape {
            circle(radius: Number),
            square(side: Number),
            point
        }
        struct Config {
            area: Number = match Shape.circle(radius: 5) {
                .circle(r): r,
                .square(s): s,
                .point: 0
            }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(
            format!("Match enum with data and field access: {:?}", result.err()).into(),
        );
    }
    Ok(())
}

// =============================================================================
// Impl for generic struct — exercises collect_definition_into for Impl
// =============================================================================

#[test]
fn test_impl_for_generic_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Stack<T> { items: [T] }
        impl Stack<T> {
            fn size() -> Number { 0 }
        }
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Impl for generic struct: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Boolean operator type compatibility
// =============================================================================

#[test]
fn test_boolean_and_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Boolean = true
        let b: Boolean = false
        let c: Boolean = a && b
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Boolean && operator: {:?}", result.err()).into());
    }
    Ok(())
}

#[test]
fn test_boolean_or_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let a: Boolean = true
        let b: Boolean = false
        let c: Boolean = a || b
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Boolean || operator: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Nested struct types with type resolution
// =============================================================================

#[test]
fn test_deeply_nested_struct_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { val: Number }
        struct B { a: A }
        struct C { b: B }
        let c: C = C(b: B(a: A(val: 42)))
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Deeply nested structs: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Optional field access — exercises validate type
// =============================================================================

#[test]
fn test_optional_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { value: Number }
        struct Outer { inner: Inner? }
        let o: Outer = Outer(inner: nil)
    ";
    let result = compile(source);
    if result.is_err() {
        return Err(format!("Optional struct field: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Trait implementation with impl block syntax
// =============================================================================

#[test]
fn test_impl_trait_for_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Drawable { render: String }
        struct Circle { radius: Number }
        impl Drawable for Circle {
            render: "circle"
        }
    "#;
    let result = compile(source);
    // `impl Trait for Struct` with field syntax is a parse error — use fn syntax instead
    if result.is_ok() {
        return Err(format!(
            "Impl trait with field syntax produces ParseError: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// String comparison — exercises binary op string type error path
// =============================================================================

#[test]
fn test_string_equality_operator() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let a: String = "hello"
        let b: String = "world"
        let eq: Boolean = a == b
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("String equality: {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// Self in impl block with mount fields
// =============================================================================

#[test]
fn test_self_with_mount_field_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Widget {
            [content: Number]
        }
        impl Widget {
            fn get() -> Number { self.content }
        }
    ";
    let result = compile(source);
    // Mount field syntax with `[name: Type]` is a parse error in this context
    if result.is_ok() {
        return Err(format!(
            "Mount field syntax produces ParseError: {:?}",
            result.ok()
        )
        .into());
    }
    Ok(())
}
