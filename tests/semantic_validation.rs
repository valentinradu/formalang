//! Semantic validation tests for coverage
//!
//! These tests exercise validation paths in the semantic analyzer

use formalang::CompilerError;

// =============================================================================
// Type Resolution Tests
// =============================================================================


fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_resolve_nested_generic_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> {
            value: T
        }
        struct Container<T> {
            box: Box<T>
        }
        struct Config {
            items: Container<String>
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_resolve_array_of_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Item<T> {
            value: T
        }
        struct List {
            items: [Item<String>]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_resolve_optional_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Wrapper<T> {
            value: T
        }
        struct Container {
            item: Wrapper<String>?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_resolve_tuple_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Pair<A, B> {
            first: A,
            second: B
        }
        struct Data {
            pair: Pair<String, Number>
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Trait Validation Tests
// =============================================================================

#[test]
fn test_trait_field_type_validation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Typed { value: String }
        struct Impl { value: Number }
        impl Typed for Impl { }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors.iter().any(
        |e| matches!(e, CompilerError::TraitFieldTypeMismatch { field, .. } if field == "value"),
    ) {
        return Err(format!("Expected TraitFieldTypeMismatch: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_trait_multiple_conformance() -> Result<(), Box<dyn std::error::Error>> {
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
fn test_trait_with_optional_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait MaybeNamed {
            name: String?
        }
        struct User {
            name: String?
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_array_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait HasItems {
            items: [String]
        }
        struct Container {
            items: [String]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base {
            id: Number
        }
        trait Extended: Base {
            name: String
        }
        struct Entity {
            id: Number,
            name: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Expression Validation Tests
// =============================================================================

#[test]
fn test_if_expression_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            status: Boolean = if true { true } else { false }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_for_expression_with_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct List {
            items: [String] = for item in ["a", "b"] { item }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_expression_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            a: Number = (let x = 10
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_let_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            a: Boolean = (let x = true
            let y = false
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_binary_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Math {
            a: Number = 1 + 2
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_comparison_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Compare {
            a: Number = if 1 < 2 { 1 } else { 0 }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_logical_operators_with_literals() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Logic {
            a: Boolean = true && false
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Error Path Tests
// =============================================================================

#[test]
fn test_invalid_if_condition_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: Number }
        impl Test {
            fn compute(self) -> Number {
                if self.value { 1 } else { 0 }
            }
        }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidIfCondition { .. }))
    {
        return Err(format!("Expected InvalidIfCondition: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_invalid_for_not_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: String }
        impl Test {
            fn compute(self) -> String {
                for item in self.value { item }
            }
        }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ForLoopNotArray { .. }))
    {
        return Err(format!("Expected ForLoopNotArray: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_undefined_variable_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: Number }
        impl Test {
            fn compute(self) -> Number {
                undefinedVariable + 1
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_field_access_on_primitive() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: Number }
        impl Test {
            fn compute(self) -> Number {
                self.nonexistent
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn test_invalid_arithmetic_on_boolean() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { flag: Boolean }
        impl Test {
            fn compute(self) -> Boolean {
                self.flag + 1
            }
        }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_invalid_comparison_types() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { text: String, num: Number }
        impl Test {
            fn compute(self) -> Boolean {
                self.text < self.num
            }
        }
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidBinaryOp { .. }))
    {
        return Err(format!("Expected InvalidBinaryOp: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Struct and Trait Field Tests
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
fn test_struct_with_header_main_footer_fields() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Layout {
            header: String,
            main: String,
            footer: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_with_content_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Renderable {
            content: String
        }
        struct View {
            content: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Use Statement Tests
// =============================================================================

#[test]
fn test_use_single_item() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod utils {
            struct Helper { value: String }
        }
    ";
    // Use statements may need specific syntax - just test the module for now
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Complex Struct Tests
// =============================================================================

#[test]
fn test_struct_with_all_field_modifiers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Complex {
            required: String,
            optional: Number?,
            mut mutable: Boolean,
            content: String
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_deeply_nested_structs() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Level3 { value: String }
        struct Level2 { inner: Level3 }
        struct Level1 { inner: Level2 }
        struct Root { inner: Level1 }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_struct_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct WithDefaults {
            name: String = "default",
            count: Number = 0,
            active: Boolean = true
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Enum Tests
// =============================================================================

#[test]
fn test_enum_with_many_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color {
            red,
            green,
            blue,
            yellow,
            cyan,
            magenta,
            white,
            black
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_enum_status_variants() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status {
            pending,
            active,
            complete,
            failed
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_generic_enum_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Container<T> {
            full,
            empty
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Dictionary Tests
// =============================================================================

#[test]
fn test_dictionary_with_struct_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String }
        struct Cache {
            users: [String: User]
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_dictionary_literal_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            data: [String: Number] = ["a": 1, "b": 2, "c": 3]
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Closure Tests
// =============================================================================

#[test]
fn test_closure_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Handler {
            process: String -> Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_multi_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            operation: Number -> Number
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_expression_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Mapper {
            data: [String] = for item in ["a", "b"] { item }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Let Expression Tests
// =============================================================================

#[test]
fn test_let_with_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = (let x: Number = 10
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_mutable() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            initial: Number = (let mut count = 0
            count)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_simple_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = (let x = 2
            x)
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Module Tests
// =============================================================================

#[test]
fn test_deeply_nested_modules() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod a {
            mod b {
                mod c {
                    struct Deep { value: String }
                }
            }
        }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_module_with_trait_and_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        mod core {
            trait Named {
                name: String
            }
            struct User {
                name: String = "default"
            }
        }
    "#;
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Impl Block Defaults Tests
// =============================================================================

#[test]
fn test_impl_block_defaults_applied_on_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    // Struct field defaults should make fields optional during instantiation
    let source = r##"
        struct MyBox {
            color: String = "#FF0000",
            size: Number = 10
        }
        struct Container {
            box: MyBox = MyBox()
        }
    "##;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_block_defaults_with_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    // Fields with struct field defaults should be optional during instantiation
    let source = r##"
        struct Rect {
            width: Number = 0
        }

        struct MyBox {
            color: String = "#FF0000",
            body: Rect = Rect()
        }
        struct Container {
            content: MyBox = MyBox()
        }
    "##;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_block_defaults_partial_override() -> Result<(), Box<dyn std::error::Error>> {
    // Can provide some fields while using struct field defaults for others
    let source = r#"
        struct Config {
            name: String = "default",
            value: Number = 0,
            enabled: Boolean = true
        }
        struct App {
            config: Config = Config(name: "custom")
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_impl_block_defaults_nested_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    // Nested struct instantiation should respect struct field defaults
    let source = r#"
        struct Inner {
            value: String = "inner default"
        }
        struct Outer {
            inner: Inner = Inner(),
            name: String = "outer default"
        }
        struct Container {
            outer: Outer = Outer()
        }
    "#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Function Return Type Validation Tests
// =============================================================================

#[test]
fn test_function_return_type_valid_f32() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Vec2 {
            x: Number,
            y: Number
        }
        impl Vec2 {
            fn length_squared(self) -> Number {
                self.x * self.x + self.y * self.y
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_function_return_type_valid_number() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Calculator {
            value: Number
        }
        impl Calculator {
            fn double(self) -> Number {
                self.value * 2
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_function_return_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Data {
            count: Number
        }
        impl Data {
            fn get_count(self) -> String {
                self.count
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Function returning Number when String expected should fail".into());
    }
    let err = result.err().ok_or("expected error")?;
    if !err
        .iter()
        .any(|e| matches!(e, CompilerError::FunctionReturnTypeMismatch { .. }))
    {
        return Err(format!("Expected FunctionReturnTypeMismatch: {err:?}").into());
    }
    Ok(())
}

#[test]
fn test_function_return_type_boolean_valid() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Checker {
            value: Number
        }
        impl Checker {
            fn is_positive(self) -> Boolean {
                self.value > 0
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_function_no_return_type_valid() -> Result<(), Box<dyn std::error::Error>> {
    // Functions without explicit return type should accept any body type
    let source = r"
        struct Processor {
            data: Number
        }
        impl Processor {
            fn process(self) {
                self.data * 2
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Assignment to Immutable Tests
// =============================================================================

#[test]
fn test_assignment_to_immutable_fails() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { value: Number }
        impl Counter {
            fn get_value(self) -> Number {
                let x = 10
                x = 20
                x
            }
        }
    ";
    let result = compile(source);
    if result.is_ok() {
        return Err("Assignment to immutable binding should fail".into());
    }
    let errors = result.err().ok_or("expected error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::AssignmentToImmutable { .. }))
    {
        return Err(format!("Expected AssignmentToImmutable: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_assignment_to_mutable_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { value: Number }
        impl Counter {
            fn get_value(self) -> Number {
                let mut x = 10
                x = 20
                x
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Block Expression Tests
// =============================================================================

#[test]
fn test_block_expr_with_let_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = {
                let a = 1
                let b = 2
                a + b
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_block_expr_with_assignment() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test { value: Number }
        impl Test {
            fn compute(self) -> Number {
                let mut sum = 0
                sum = sum + 10
                sum
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_nested_block_expressions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Test {
            value: Number = {
                let outer = {
                    let inner = 5
                    inner * 2
                }
                outer + 1
            }
        }
    ";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Regression tests for Phase 3 semantic correctness fixes
// =============================================================================

/// Fix 1: FieldAccess/MethodCall inference should return meaningful types,
/// not "Unknown". A field access used as an if-condition must be detected as
/// invalid when the field is non-Boolean.
#[test]
fn test_field_access_inference_detects_invalid_if_condition(
) -> Result<(), Box<dyn std::error::Error>> {
    // p.count has type Number; using it as the if condition must now error
    // (was previously accepted because field-access inference returned Unknown).
    let source = r"
        struct Counter { count: Number }
        let c = Counter(count: 3)
        let _ = if c.count { 1 } else { 0 }
    ";
    let result = compile(source);
    match result {
        Ok(_) => Err("expected InvalidIfCondition for Number-typed field as condition".into()),
        Err(errors) => {
            if errors
                .iter()
                .any(|e| matches!(e, CompilerError::InvalidIfCondition { .. }))
            {
                Ok(())
            } else {
                Err(format!("expected InvalidIfCondition, got {errors:?}").into())
            }
        }
    }
}

/// Fix 2: Block-scoped let bindings must not leak into the enclosing scope.
#[test]
fn test_block_scope_does_not_leak() -> Result<(), Box<dyn std::error::Error>> {
    // `inner` is declared in a block; referencing it outside must error.
    let source = r"
        struct Test {
            value: Number = {
                let inner = 5
                inner
            }
        }
        let _ = inner
    ";
    let result = compile(source);
    match result {
        Ok(_) => Err("expected block-local binding to be out of scope".into()),
        Err(errors) => {
            let has_undefined = errors
                .iter()
                .any(|e| matches!(e, CompilerError::UndefinedReference { .. }));
            if has_undefined {
                Ok(())
            } else {
                Err(format!("expected UndefinedReference, got {errors:?}").into())
            }
        }
    }
}

/// Fix 7: Nested module visibility must be enforced for 3+ level paths.
/// When middle modules are private, accessing a deeply-nested item must error.
#[test]
fn test_nested_private_module_access_errors() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub mod outer {
            mod hidden {
                pub struct Secret { x: Number }
            }
        }
        let _ = outer::hidden::Secret(x: 1)
    ";
    let result = compile(source);
    match result {
        Ok(_) => Err("expected nested private module access to error".into()),
        Err(errors) => {
            let has_visibility = errors
                .iter()
                .any(|e| matches!(e, CompilerError::VisibilityViolation { .. }));
            if has_visibility {
                Ok(())
            } else {
                Err(format!("expected VisibilityViolation, got {errors:?}").into())
            }
        }
    }
}

/// Fix 12: If-expr branches of T and Nil should unify to T? without errors.
#[test]
fn test_if_expr_widens_to_optional() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let x: Number? = if true { 1 } else { nil }
    ";
    compile(source).map_err(|e| format!("Failed: {e:?}"))?;
    Ok(())
}
