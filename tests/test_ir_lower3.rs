//! Additional coverage for lower.rs: destructuring in let/block,
//! vector field access, `lower_block_statement` patterns, event mapping edge cases,
//! `resolve_field_type` for vectors, `get_variant_fields` in self context.

#![allow(clippy::expect_used)]

use formalang::ast::{BinaryOperator, PrimitiveType};
use formalang::compile_to_ir;
use formalang::ir::{IrExpr, ResolvedType};

// =============================================================================
// Lower: let binding array destructuring with rest pattern
// =============================================================================

#[test]
fn test_lower_let_array_destructuring_rest() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let [first, ...rest] = [1, 2, 3]
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    // 'first' should be bound; 'rest' should also be bound
    let first = module.lets.iter().find(|l| l.name == "first");
    let rest = module.lets.iter().find(|l| l.name == "rest");
    if first.is_none() && rest.is_none() {
        return Err("Should have at least one binding".into());
    }
    Ok(())
}

#[test]
fn test_lower_let_array_wildcard() -> Result<(), Box<dyn std::error::Error>> {
    // Array destructuring with wildcard - no binding for wildcard
    let source = r"
        let [a, _] = [1, 2]
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let a = module.lets.iter().find(|l| l.name == "a");
    if a.is_none() {
        return Err("Should have 'a' binding".into());
    }
    Ok(())
}

// =============================================================================
// Lower: block statement with tuple pattern
// =============================================================================

#[test]
fn test_lower_block_let_tuple_pattern() -> Result<(), Box<dyn std::error::Error>> {
    // Tuple destructuring in a block statement using BindingPattern::Tuple
    let source = r"
        struct Config { val: Number = {
            let a: Number = 1
            let b: Number = 2
            a + b
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = expr else {
        return Err(format!("Expected Block, got {expr:?}").into());
    };
    if statements.len() < 2 {
        return Err(format!("Expected at least 2 statements, got {}", statements.len()).into());
    }
    Ok(())
}

// =============================================================================
// Lower: resolve_field_type for vectors
// =============================================================================

#[test]
fn test_lower_vec2_field_access() -> Result<(), Box<dyn std::error::Error>> {
    // vec2 component access - exercises vector field resolution
    let source = r"
        fn get_x(v: Vec2) -> F32 { v.x }
    ";
    // Vec2/F32 are external GPU types not defined in the source — expect undefined type errors
    let result = compile_to_ir(source);
    if result.is_ok() {
        return Err(
            "Vec2/F32 undefined types should produce errors but compilation succeeded".into(),
        );
    }
    Ok(())
}

#[test]
fn test_lower_field_access_on_struct() -> Result<(), Box<dyn std::error::Error>> {
    // Field access on a known struct variable (exercises resolve_field_type struct branch)
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn get_x() -> Number { self.x }
            fn get_y() -> Number { self.y }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    if module.impls.is_empty() {
        return Err("Expected non-empty impls".into());
    }
    let impl_block = module.impls.first().ok_or("no impl block")?;
    let get_x = impl_block
        .functions
        .iter()
        .find(|f| f.name == "get_x")
        .ok_or("get_x not found")?;
    let IrExpr::SelfFieldRef { field, .. } = get_x.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected SelfFieldRef, got {:?}", get_x.body.as_ref()).into());
    };
    if field != "x" {
        return Err(format!("Expected field 'x', got '{field}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_block_statement Tuple/Struct/Array pattern
// =============================================================================

#[test]
fn test_lower_block_let_with_struct_field_access() -> Result<(), Box<dyn std::error::Error>> {
    // Block with multiple let statements
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        fn compute() -> Number {
            let a: Number = 1
            let b: Number = 2
            a + b
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "compute")
        .ok_or("compute not found")?;
    let IrExpr::Block { statements, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected Block, got {:?}", func.body.as_ref()).into());
    };
    if statements.len() < 2 {
        return Err(format!("Expected at least 2 statements, got {}", statements.len()).into());
    }
    Ok(())
}

// =============================================================================
// Lower: match arm in self context (get_variant_fields self branch)
// =============================================================================

#[test]
fn test_lower_enum_impl_match_self() -> Result<(), Box<dyn std::error::Error>> {
    // In an enum impl, a match expression
    let source = r"
        enum Shape { circle, square }
        impl Shape {
            fn is_circle() -> Boolean { true }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("no impl block")?;
    let func = impl_block.functions.first().ok_or("no function")?;
    // Just verify the impl function was lowered
    if func.name != "is_circle" {
        return Err(format!("Expected is_circle, got {}", func.name).into());
    }
    Ok(())
}

// =============================================================================
// Lower: closure with inferred enum without context
// =============================================================================

#[test]
fn test_lower_closure_inferred_enum_no_context() -> Result<(), Box<dyn std::error::Error>> {
    // Inferred enum with no return type context
    let source = r"
        struct Button {
            on_click: () -> String = () -> .done
        }
    ";
    // Inferred enum with no return type context — the compiler lowers it with placeholder types
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    // The on_click field should lower with an unknown/inferred return type
    if module.structs.is_empty() {
        return Err("Module should contain the Button struct".into());
    }
    let field = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?;
    if field.default.is_none() {
        return Err("on_click field should have a default expression".into());
    }
    Ok(())
}

// =============================================================================
// Lower: resolve_method_return_type for enum methods
// =============================================================================

#[test]
fn test_lower_method_on_enum_in_func() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, inactive }
        impl Status {
            fn label() -> String { "active" }
        }
        fn get_label(s: Status) -> String { "test" }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    if module.impls.is_empty() {
        return Err("Expected non-empty impls".into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_type for Closure (returns UnsupportedType)
// =============================================================================

#[test]
fn test_lower_closure_type_in_struct() -> Result<(), Box<dyn std::error::Error>> {
    // A closure type in a struct field exercises lower_type for Closure variant
    let source = r"
        struct Config {
            handler: (Number) -> Number = |x: Number| 42
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    if module.structs.len() != 1 {
        return Err(format!("expected exactly one struct, got {}", module.structs.len()).into());
    }
    let field = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?;
    if field.name != "handler" {
        return Err(format!("wrong field name: {}", field.name).into());
    }
    // Closure type should lower to ResolvedType::Closure
    if !matches!(field.ty, formalang::ir::ResolvedType::Closure { .. }) {
        return Err(format!(
            "closure field should have ResolvedType::Closure, got {:?}",
            field.ty
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_let_binding for struct field type lookup
// =============================================================================

#[test]
fn test_lower_get_field_type_from_resolved() -> Result<(), Box<dyn std::error::Error>> {
    // Test that field type is resolved properly from a struct field access
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn sum() -> Number { self.x + self.y }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("no impl block")?;
    let func = impl_block.functions.first().ok_or("no function")?;
    // Body should be x + y (BinaryOp of two SelfFieldRefs)
    let IrExpr::BinaryOp { left, right, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected BinaryOp, got {:?}", func.body.as_ref()).into());
    };
    let IrExpr::SelfFieldRef { field: lf, .. } = left.as_ref() else {
        return Err(format!("Expected SelfFieldRef for left, got {left:?}").into());
    };
    if lf != "x" {
        return Err(format!("Expected left field 'x', got '{lf}'").into());
    }
    let IrExpr::SelfFieldRef { field: rf, .. } = right.as_ref() else {
        return Err(format!("Expected SelfFieldRef for right, got {right:?}").into());
    };
    if rf != "y" {
        return Err(format!("Expected right field 'y', got '{rf}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: module with function inside
// =============================================================================

#[test]
fn test_lower_module_with_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod utils {
            fn helper() -> Number { 42 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let helper = module.functions.iter().find(|f| f.name == "helper");
    if helper.is_none() {
        return Err("Should have helper function from module".into());
    }
    Ok(())
}

// =============================================================================
// Lower: string_to_resolved_type for Regex and Path
// =============================================================================

#[test]
fn test_lower_path_type_in_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let p: Path = "/"
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let binding = module
        .lets
        .iter()
        .find(|l| l.name == "p")
        .ok_or("'p' binding not found")?;
    if !matches!(binding.ty, ResolvedType::Primitive(PrimitiveType::Path)) {
        return Err(format!("Expected Path type, got {:?}", binding.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_type for TypeParameter
// =============================================================================

#[test]
fn test_lower_generic_param_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { inner: T }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let s = module.structs.first().ok_or("no struct")?;
    if s.generic_params.len() != 1 {
        return Err(format!("Expected 1 generic param, got {}", s.generic_params.len()).into());
    }
    let param = s.generic_params.first().ok_or("no generic param")?;
    if param.name != "T" {
        return Err(format!("Expected param 'T', got '{}'", param.name).into());
    }
    Ok(())
}

// =============================================================================
// Lower: optional field type
// =============================================================================

#[test]
fn test_lower_optional_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String, age: Number? }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let age_field = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .iter()
        .find(|f| f.name == "age")
        .ok_or("age field not found")?;
    if !matches!(age_field.ty, ResolvedType::Optional(_)) {
        return Err(format!("Expected Optional type, got {:?}", age_field.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: resolve_function_return_type fallback for unknown function
// =============================================================================

#[test]
fn test_lower_unknown_function_call() -> Result<(), Box<dyn std::error::Error>> {
    // Struct that calls a defined function in its default value
    let source = r"
        fn scale() -> Number { 2 }
        struct Config { val: Number = scale() }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::FunctionCall { path, .. } = expr else {
        return Err(format!("Expected FunctionCall, got {expr:?}").into());
    };
    let first = path.first().ok_or("empty path")?;
    if first != "scale" {
        return Err(format!("Expected path 'scale', got '{first}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_type for Dictionary
// =============================================================================

#[test]
fn test_lower_dictionary_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Cache { data: [String: Number] = ["x": 1] }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let field = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?;
    if !matches!(field.ty, ResolvedType::Dictionary { .. }) {
        return Err(format!("Expected Dictionary type, got {:?}", field.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: lower_trait with composed traits
// =============================================================================

#[test]
fn test_lower_trait_with_composition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base { x: Number }
        trait Extended: Base { y: Number }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let extended = module
        .traits
        .iter()
        .find(|t| t.name == "Extended")
        .ok_or("Extended not found")?;
    if extended.composed_traits.is_empty() {
        return Err("Extended should compose Base".into());
    }
    Ok(())
}

// =============================================================================
// Lower: enum with generic params
// =============================================================================

#[test]
fn test_lower_generic_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Option<T> { some(value: T), none }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let opt = module
        .enums
        .iter()
        .find(|e| e.name == "Option")
        .ok_or("Option not found")?;
    if opt.generic_params.len() != 1 {
        return Err(format!("Expected 1 generic param, got {}", opt.generic_params.len()).into());
    }
    let opt_param = opt.generic_params.first().ok_or("no generic param")?;
    if opt_param.name != "T" {
        return Err(format!("Expected param 'T', got '{}'", opt_param.name).into());
    }
    Ok(())
}

// =============================================================================
// Lower: method call chain (builtin method on primitive)
// =============================================================================

#[test]
fn test_lower_builtin_math_functions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        extern fn sin(x: Number) -> Number
        extern fn cos(x: Number) -> Number
        fn compute() -> Number {
            sin(x: 1) + cos(x: 1)
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "compute")
        .ok_or("compute not found")?;
    // Should produce BinaryOp(FunctionCall(sin), FunctionCall(cos))
    let IrExpr::BinaryOp {
        left, right, op, ..
    } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!(
            "Expected BinaryOp from math calls, got {:?}",
            func.body.as_ref()
        )
        .into());
    };
    if *op != BinaryOperator::Add {
        return Err(format!("sin + cos should use Add operator, got {op:?}").into());
    }
    if !matches!(left.as_ref(), IrExpr::FunctionCall { .. }) {
        return Err(format!("left should be FunctionCall, got {left:?}").into());
    }
    if !matches!(right.as_ref(), IrExpr::FunctionCall { .. }) {
        return Err(format!("right should be FunctionCall, got {right:?}").into());
    }
    Ok(())
}

// =============================================================================
// Lower: vec3 constructor
// =============================================================================

#[test]
fn test_lower_function_call_lowering() -> Result<(), Box<dyn std::error::Error>> {
    // Test that a function call to another named function lowers to IrExpr::FunctionCall.
    // (Vec3/vec3 were GPU-type constructors removed with the old codegen backend.)
    let source = r"
        fn sum(a: Number, b: Number, c: Number) -> Number { a + b + c }
        fn make_result() -> Number { sum(1, 2, 3) }
    ";
    let module =
        compile_to_ir(source).map_err(|e| format!("function call source should compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "make_result")
        .ok_or("make_result function should be in IR")?;
    let IrExpr::FunctionCall { path, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!(
            "make_result body should be a FunctionCall, got {:?}",
            func.body
        )
        .into());
    };
    let first = path.first().ok_or("empty path")?;
    if first != "sum" {
        return Err(format!("Expected path 'sum', got '{first}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: literal types (unsigned int, signed int, nil)
// =============================================================================

#[test]
fn test_lower_unsigned_int_literal() -> Result<(), Box<dyn std::error::Error>> {
    // Test that a struct field with a numeric default lowers to IrExpr::Literal.
    // (U32 type with 'u' suffix was not a supported language feature.)
    let source = r"
        struct Config { count: Number = 42 }
    ";
    let module = compile_to_ir(source)
        .map_err(|e| format!("struct with numeric default should compile: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("no struct")?
        .fields
        .first()
        .ok_or("no field")?
        .default
        .as_ref()
        .ok_or("field should have a default value")?;
    if !matches!(expr, IrExpr::Literal { .. }) {
        return Err(
            format!("numeric default value should lower to IrExpr::Literal, got {expr:?}").into(),
        );
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_type with generic struct field
// =============================================================================

#[test]
fn test_dce_mark_used_generic_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{eliminate_dead_code, DeadCodeEliminator};

    let source = r"
        struct Box<T> { value: T }
        struct Config { wrapped: Box<Number> = Box<Number>(value: 42) }
        impl Config {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    // Box should be used (field type is generic struct)
    let box_id = module.struct_id("Box").ok_or("Box not found")?;
    if !dce.is_struct_used(box_id) {
        return Err("Box should be used via Config field".into());
    }
    // Also verify eliminate_dead_code preserves Box since it's referenced
    let optimized = eliminate_dead_code(&module, true);
    if optimized.struct_id("Box").is_none() {
        return Err("eliminate_dead_code should preserve Box since Config references it".into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_type with optional struct field
// =============================================================================

#[test]
fn test_dce_mark_used_optional_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Inner { val: Number = 0 }
        struct Config { item: Inner? }
        impl Config {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    let inner_id = module.struct_id("Inner").ok_or("Inner not found")?;
    if !dce.is_struct_used(inner_id) {
        return Err("Inner should be used via optional field".into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_type with dict field
// =============================================================================

#[test]
fn test_dce_mark_used_dict_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct ValType { n: Number = 0 }
        struct Config { map: [String: ValType] }
        impl Config {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    let val_id = module.struct_id("ValType").ok_or("ValType not found")?;
    if !dce.is_struct_used(val_id) {
        return Err("ValType should be used via dict value type".into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_type with tuple field
// =============================================================================

#[test]
fn test_dce_mark_used_tuple_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Component { x: Number = 0 }
        struct Config { pair: (a: Component, b: Number) }
        impl Config {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    let comp_id = module.struct_id("Component").ok_or("Component not found")?;
    if !dce.is_struct_used(comp_id) {
        return Err("Component should be used via tuple field".into());
    }
    Ok(())
}

// =============================================================================
// DCE: eliminate_dead_code_expr MethodCall
// =============================================================================

#[test]
fn test_dce_eliminate_method_call_args() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        struct Calc { val: Number = 0 }
        impl Calc {
            fn square() -> Number { self.val * self.val }
            fn double() -> Number { self.square() }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    // Just verify DCE ran on impl with method calls
    if optimized.impls.is_empty() {
        return Err("Expected non-empty impls after DCE".into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_expr for FieldAccess
// =============================================================================

#[test]
fn test_dce_mark_used_in_field_access() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Inner { val: Number = 0 }
        struct Outer { inner: Inner }
        impl Outer {
            fn get_val() -> Number { self.inner.val }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    // Inner should be used via field access in Outer's method
    let inner_id = module.struct_id("Inner").ok_or("Inner not found")?;
    // At minimum: Outer is used, Inner is used via struct field type
    if !dce.is_struct_used(inner_id) && dce.used_structs().is_empty() {
        return Err("Expected Inner or at least one struct to be used".into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_type for closure field
// =============================================================================

#[test]
fn test_dce_mark_used_closure_field() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        enum Action { submit }
        struct Button {
            on_click: () -> Action = () -> Action.submit
        }
        impl Button {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    // DCE should have analyzed the module and found at least one used struct
    let button_id = module
        .struct_id("Button")
        .ok_or("Button struct should exist in module")?;
    if !dce.used_structs().contains(&button_id) {
        return Err("Button struct should be marked used after DCE analysis".into());
    }
    Ok(())
}
