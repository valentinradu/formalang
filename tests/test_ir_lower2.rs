//! Additional tests for lower.rs coverage: inferred enums, closures,
//! destructuring patterns, method calls on enums, `string_to_resolved_type`,
//! block let destructuring, `lower_let_binding` patterns, module/function lowering.

#![allow(clippy::expect_used)]

use formalang::ast::BinaryOperator;
use formalang::ast::ParamConvention;
use formalang::ast::PrimitiveType;
use formalang::compile_to_ir;
use formalang::ir::{eliminate_dead_code, IrExpr, ResolvedType};

// =============================================================================
// Inferred enum instantiation in functions
// =============================================================================

#[test]
fn test_lower_inferred_enum_in_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green, blue }
        fn get_color() -> Color { .red }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "get_color")
        .ok_or("get_color")?;
    let IrExpr::EnumInst { variant, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected EnumInst, got {:?}", func.body.as_ref()).into());
    };
    if variant != "red" {
        return Err(format!("expected 'red', got '{variant}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower closures (non-event)
// =============================================================================

#[test]
fn test_lower_pipe_closure_in_struct_field_default() -> Result<(), Box<dyn std::error::Error>> {
    // Renamed from `test_lower_general_closure` (audit #53): the
    // original name said nothing about the actual scenario. The test
    // verifies that a pipe-style closure (`|x: I32| 42`) used as a
    // struct field default lowers to `IrExpr::Closure` with the
    // expected single named parameter.
    let source = r"
        struct Config {
            transform: (I32) -> I32 = |x: I32| 42
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Closure { params, body, .. } = expr else {
        return Err(format!("Expected Closure, got {expr:?}").into());
    };
    if params.len() != 1 {
        return Err(format!("expected 1 param, got {}", params.len()).into());
    }
    if params.first().ok_or("index out of bounds")?.1 != "x" {
        return Err(format!(
            "expected param 'x', got '{}'",
            params.first().ok_or("index out of bounds")?.1
        )
        .into());
    }
    if !matches!(
        body.as_ref(),
        IrExpr::Literal { .. }
            | IrExpr::FunctionCall { .. }
            | IrExpr::EnumInst { .. }
            | IrExpr::Reference { .. }
            | IrExpr::LetRef { .. }
    ) {
        return Err(format!("Closure body should be an expression, got {body:?}").into());
    }
    Ok(())
}

// =============================================================================
// Lower let binding: array destructuring
// =============================================================================

#[test]
fn test_lower_let_array_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let [a, b] = [1, 2]
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    // Array destructuring creates separate lets for each element
    let a = module.lets.iter().find(|l| l.name == "a");
    let b = module.lets.iter().find(|l| l.name == "b");
    if a.is_none() {
        return Err("Should have 'a' binding".into());
    }
    if b.is_none() {
        return Err("Should have 'b' binding".into());
    }
    Ok(())
}

#[test]
fn test_lower_two_independent_simple_lets() -> Result<(), Box<dyn std::error::Error>> {
    // Two `let a: I32 = 1` / `let b: I32 = 2` should produce two
    // separate top-level `IrLet` entries. Name was previously
    // `test_lower_let_tuple_destructuring` which was misleading — this
    // doesn't exercise tuple destructuring at all.
    let source = r"
        let a: I32 = 1
        let b: I32 = 2
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    if module.lets.len() != 2 {
        return Err(format!("expected 2 let bindings, got {}", module.lets.len()).into());
    }
    Ok(())
}

#[test]
fn test_lower_let_struct_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    // Struct let binding (not destructuring — test that struct type resolves)
    let source = r"
        struct Point { x: I32 = 0, y: I32 = 0 }
        let p: Point = Point(x: 3, y: 4)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let p = module.lets.iter().find(|l| l.name == "p").ok_or("p")?;
    if !matches!(p.ty, ResolvedType::Struct(_)) {
        return Err(format!("Expected Struct type for p, got {:?}", p.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower block statement: tuple destructuring inside block
// =============================================================================

#[test]
fn test_lower_block_tuple_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    // Block let with tuple pattern - use positional tuple syntax
    let source = r"
        struct Config { val: I32 = {
            let x: I32 = 1
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = expr else {
        return Err(format!("Expected Block, got {expr:?}").into());
    };
    if statements.is_empty() {
        return Err("Block should have at least one statement".into());
    }
    Ok(())
}

#[test]
fn test_lower_block_struct_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: I32 = 0, y: I32 = 0 }
        struct Config { val: I32 = {
            let p: Point = Point(x: 1, y: 2)
            p.x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let expr = module
        .structs
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = expr else {
        return Err(format!("Expected Block, got {expr:?}").into());
    };
    if statements.is_empty() {
        return Err("Block should have at least one statement".into());
    }
    Ok(())
}

#[test]
fn test_lower_block_array_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { val: I32 = {
            let items: [I32] = [1, 2]
            42
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = expr else {
        return Err(format!("Expected Block, got {expr:?}").into());
    };
    if statements.is_empty() {
        return Err("Block should have at least one statement".into());
    }
    Ok(())
}

// =============================================================================
// Lower: module definitions
// =============================================================================

#[test]
fn test_lower_module_with_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod shapes {
            struct Circle { radius: I32 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    // Module struct should be qualified
    let has_circle = module.structs.iter().any(|s| s.name.contains("Circle"));
    if !has_circle {
        return Err("Should have Circle struct from module".into());
    }
    Ok(())
}

#[test]
fn test_lower_module_with_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod colors {
            enum Color { red, green, blue }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let has_color = module.enums.iter().any(|e| e.name.contains("Color"));
    if !has_color {
        return Err("Should have Color enum from module".into());
    }
    Ok(())
}

#[test]
fn test_lower_module_with_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod shapes {
            trait Drawable { area: I32 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let has_trait = module.traits.iter().any(|t| t.name.contains("Drawable"));
    if !has_trait {
        return Err("Should have Drawable trait from module".into());
    }
    Ok(())
}

#[test]
fn test_lower_nested_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod outer {
            mod inner {
                struct Point { x: I32, y: I32 }
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let has_point = module.structs.iter().any(|s| s.name.contains("Point"));
    if !has_point {
        return Err("Should have Point from nested module".into());
    }
    Ok(())
}

// =============================================================================
// Lower: standalone functions
// =============================================================================

#[test]
fn test_lower_function_with_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn add(x: I32, y: I32) -> I32 { x + y }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "add")
        .ok_or("add function")?;
    if func.params.len() != 2 {
        return Err(format!("expected 2 params, got {}", func.params.len()).into());
    }
    if func.return_type.is_none() {
        return Err("expected return type".into());
    }
    Ok(())
}

#[test]
fn test_lower_function_default_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        fn greet(name: String = "World") -> String { name }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "greet")
        .ok_or("greet")?;
    if func.params.len() != 1 {
        return Err(format!("expected 1 param, got {}", func.params.len()).into());
    }
    if func
        .params
        .first()
        .ok_or("index out of bounds")?
        .default
        .is_none()
    {
        return Err("Should have default param".into());
    }
    Ok(())
}

// =============================================================================
// Lower: method call on enum impl
// =============================================================================

#[test]
fn test_lower_method_call_on_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        impl Status {
            fn is_active() -> Boolean { true }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    if module.impls.is_empty() {
        return Err("expected at least one impl block".into());
    }
    Ok(())
}

// =============================================================================
// Lower: match with variant bindings
// =============================================================================

#[test]
fn test_lower_match_with_variant_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Shape { circle, rect }
        fn area(s: Shape) -> I32 {
            match s {
                .circle: 1,
                _: 0
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "area")
        .ok_or("area")?;
    let IrExpr::Match { arms, .. } = func.body.as_ref().expect("expected function body") else {
        return Err(format!(
            "Expected Match in function body, got {:?}",
            func.body.as_ref()
        )
        .into());
    };
    if arms.is_empty() {
        return Err("expected at least one arm".into());
    }
    Ok(())
}

// =============================================================================
// Lower: field access on struct
// =============================================================================

#[test]
fn test_lower_field_access() -> Result<(), Box<dyn std::error::Error>> {
    // Field access via impl block (self.x)
    let source = r"
        struct Point { x: I32 = 0, y: I32 = 0 }
        impl Point {
            fn get_x() -> I32 { self.x }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("index out of bounds")?;
    let func = impl_block
        .functions
        .iter()
        .find(|f| f.name == "get_x")
        .ok_or("get_x")?;
    let IrExpr::SelfFieldRef { field, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected SelfFieldRef, got {:?}", func.body.as_ref()).into());
    };
    if field != "x" {
        return Err(format!("expected field 'x', got '{field}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: method call on struct
// =============================================================================

#[test]
fn test_lower_method_call() -> Result<(), Box<dyn std::error::Error>> {
    // Method call within impl (self.method())
    let source = r"
        struct Counter { count: I32 = 0 }
        impl Counter {
            fn increment() -> I32 { self.count + 1 }
            fn double_increment() -> I32 { self.increment() }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let impl_block = &module.impls.first().ok_or("index out of bounds")?;
    let func = impl_block
        .functions
        .iter()
        .find(|f| f.name == "double_increment")
        .ok_or("double_increment")?;
    let IrExpr::MethodCall { method, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected MethodCall, got {:?}", func.body.as_ref()).into());
    };
    if method != "increment" {
        return Err(format!("expected method 'increment', got '{method}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: self reference in impl
// =============================================================================

#[test]
fn test_lower_self_reference_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { count: I32 = 0 }
        impl Counter {
            fn doubled() -> I32 { self.count * 2 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("index out of bounds")?;
    let func = impl_block.functions.first().ok_or("index out of bounds")?;
    // Body should contain SelfFieldRef
    let IrExpr::BinaryOp { left, .. } = func.body.as_ref().expect("expected function body") else {
        return Err(format!("Expected BinaryOp, got {:?}", func.body.as_ref()).into());
    };
    let IrExpr::SelfFieldRef { field, .. } = left.as_ref() else {
        return Err(format!("Expected SelfFieldRef on left of BinaryOp, got {left:?}").into());
    };
    if field != "count" {
        return Err(format!("expected field 'count', got '{field}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: bare self reference
// =============================================================================

#[test]
fn test_lower_bare_self_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Widget { value: I32 = 0 }
        impl Widget {
            fn identity() -> Widget { self }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("index out of bounds")?;
    let func = impl_block.functions.first().ok_or("index out of bounds")?;
    let IrExpr::Reference { path, .. } = func.body.as_ref().expect("expected function body") else {
        return Err(format!("Expected Reference(self), got {:?}", func.body.as_ref()).into());
    };
    let first_path = path.first().ok_or("index out of bounds")?;
    if first_path != "self" {
        return Err(format!("expected path[0] = 'self', got '{first_path}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: let reference (LetRef)
// =============================================================================

#[test]
fn test_lower_let_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let base: I32 = 10
        let doubled: I32 = base * 2
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module
        .lets
        .iter()
        .find(|l| l.name == "doubled")
        .ok_or("doubled")?;
    let IrExpr::BinaryOp { left, .. } = &binding.value else {
        return Err(format!("Expected BinaryOp, got {:?}", binding.value).into());
    };
    let IrExpr::LetRef { name, .. } = left.as_ref() else {
        return Err(format!("Expected LetRef, got {left:?}").into());
    };
    if name != "base" {
        return Err(format!("expected 'base', got '{name}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: string_to_resolved_type branches
// =============================================================================

#[test]
fn test_lower_let_with_string_type_annotation() -> Result<(), Box<dyn std::error::Error>> {
    // This exercises string_to_resolved_type for primitive types
    let source = r#"
        let name: String = "hello"
        let count: I32 = 42
        let flag: Boolean = true
        let p: Path = /home/user/file
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    if module.lets.len() != 4 {
        return Err(format!("expected 4 lets, got {}", module.lets.len()).into());
    }
    Ok(())
}

#[test]
fn test_lower_let_struct_type_from_string() -> Result<(), Box<dyn std::error::Error>> {
    // When a let binding uses a struct type looked up by name
    let source = r"
        struct Config { val: I32 = 0 }
        let c: Config = Config(val: 42)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module.lets.iter().find(|l| l.name == "c").ok_or("c")?;
    let ResolvedType::Struct(struct_id) = &binding.ty else {
        return Err(format!("Expected Struct type for let binding, got {:?}", binding.ty).into());
    };
    let struct_def = module.get_struct(*struct_id).ok_or("struct not found")?;
    if struct_def.name != "Config" {
        return Err(format!("expected 'Config', got '{}'", struct_def.name).into());
    }
    Ok(())
}

// =============================================================================
// Lower: type annotation path (type without annotation inferred from expr)
// =============================================================================

#[test]
fn test_lower_let_type_inferred_from_expr() -> Result<(), Box<dyn std::error::Error>> {
    // Let binding without type annotation: type inferred from expression
    let source = r"let x = 42";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module.lets.iter().find(|l| l.name == "x").ok_or("x")?;
    if !matches!(binding.ty, ResolvedType::Primitive(PrimitiveType::I32)) {
        return Err(format!("Expected I32 type, got {:?}", binding.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: dict access value type extraction
// =============================================================================

#[test]
fn test_lower_dict_access_type_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let dict: [String: I32] = ["key": 1]
        let val: I32 = dict["key"]
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module.lets.iter().find(|l| l.name == "val").ok_or("val")?;
    let IrExpr::DictAccess { .. } = &binding.value else {
        return Err(format!("Expected DictAccess, got {:?}", binding.value).into());
    };
    if !matches!(binding.ty, ResolvedType::Primitive(PrimitiveType::I32)) {
        return Err(format!("Expected I32 value type from dict, got {:?}", binding.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: generic struct instantiation
// =============================================================================

#[test]
fn test_lower_generic_struct_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box<T> { value: T }
        struct Config { wrapped: Box<I32> = Box<I32>(value: 42) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    if module.structs.is_empty() {
        return Err("expected at least one struct".into());
    }
    Ok(())
}

// =============================================================================
// Lower: empty array literal
// =============================================================================

#[test]
fn test_lower_empty_array() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [I32] = []
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module
        .lets
        .iter()
        .find(|l| l.name == "items")
        .ok_or("items")?;
    let IrExpr::Array { elements, ty } = &binding.value else {
        return Err(format!("Expected Array, got {:?}", binding.value).into());
    };
    if !elements.is_empty() {
        return Err(format!("expected empty array, got {} elements", elements.len()).into());
    }
    // Audit #41: an annotated empty-array let should lift the
    // annotation's element type into the IR Array node so backends see
    // `Array(I32)`, not `Array(Never)`.
    let ResolvedType::Array(inner) = ty else {
        return Err(format!("expected Array type, got {ty:?}").into());
    };
    if !matches!(
        inner.as_ref(),
        ResolvedType::Primitive(PrimitiveType::I32)
    ) {
        return Err(format!("expected Array(I32), got Array({inner:?})").into());
    }
    Ok(())
}

// =============================================================================
// Lower: empty dict literal
// =============================================================================

#[test]
fn test_lower_empty_dict_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let d: [String: I32] = [:]
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let b = module
        .lets
        .iter()
        .find(|l| l.name == "d")
        .ok_or("let binding 'd' should be in IR")?;
    let IrExpr::DictLiteral { entries, .. } = &b.value else {
        return Err(format!(
            "'d' value should lower to IrExpr::DictLiteral, got {:?}",
            b.value
        )
        .into());
    };
    if !entries.is_empty() {
        return Err(format!(
            "empty dict literal should have no entries, got {}",
            entries.len()
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Lower: for loop type resolution from non-array collection
// =============================================================================

#[test]
fn test_lower_for_loop_from_dict() -> Result<(), Box<dyn std::error::Error>> {
    // For loop over a variable reference
    let source = r"
        let items: [I32] = [1, 2, 3]
        let mapped: [I32] = for x in items { 0 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module
        .lets
        .iter()
        .find(|l| l.name == "mapped")
        .ok_or("mapped")?;
    let IrExpr::For { var, .. } = &binding.value else {
        return Err(format!("Expected For, got {:?}", binding.value).into());
    };
    if var != "x" {
        return Err(format!("expected var 'x', got '{var}'").into());
    }
    Ok(())
}

// =============================================================================
// Lower: block with no statements (returns result directly)
// =============================================================================

#[test]
fn test_lower_block_with_no_statements() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { val: I32 = { 42 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let expr = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    // Block with no statements should return result directly (not wrapped in Block)
    if !matches!(expr, IrExpr::Literal { .. } | IrExpr::Block { .. }) {
        return Err(format!("Expected Literal or Block, got {expr:?}").into());
    }
    Ok(())
}

// =============================================================================
// Lower: LetExpr (let expression in body)
// =============================================================================

#[test]
fn test_lower_let_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: I32) -> I32 {
            let y: I32 = x + 1
            y
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "compute")
        .ok_or("compute")?;
    // The function body should be a block with a let statement
    let IrExpr::Block { statements, .. } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!("Expected Block, got {:?}", func.body.as_ref()).into());
    };
    if statements.is_empty() {
        return Err("Block should have at least one statement".into());
    }
    Ok(())
}

// =============================================================================
// Lower: impl with function in module
// =============================================================================

#[test]
fn test_lower_module_with_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod geometry {
            struct Circle { radius: I32 }
            impl Circle {
                fn area() -> I32 { self.radius }
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    if module.impls.is_empty() {
        return Err("Should have impl block from module".into());
    }
    Ok(())
}

// =============================================================================
// Lower: DCE on module with impl and let bindings
// =============================================================================

#[test]
fn test_lower_dce_on_impl_functions() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Checker { val: I32 = 0 }
        impl Checker {
            fn test() -> I32 { if true { 1 } else { 2 } }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let impl_block = optimized
        .impls
        .iter()
        .find(|i| !i.functions.is_empty())
        .ok_or("impl")?;
    let func = impl_block.functions.first().ok_or("index out of bounds")?;
    if !matches!(
        func.body.as_ref().expect("expected function body"),
        IrExpr::Literal { .. } | IrExpr::If { .. }
    ) {
        return Err(format!("Unexpected body: {:?}", func.body.as_ref()).into());
    }
    Ok(())
}

// =============================================================================
// Lower: external type resolution
// =============================================================================

#[test]
fn test_lower_builtin_function_calls() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        extern fn sin(x: I32) -> I32
        extern fn cos(x: I32) -> I32
        fn builtin_test() -> I32 { sin(x: 0) + cos(x: 0) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "builtin_test")
        .ok_or("builtin_test")?;
    let IrExpr::BinaryOp {
        left, right, op, ..
    } = func.body.as_ref().expect("expected function body")
    else {
        return Err(format!(
            "Expected BinaryOp with builtin calls, got {:?}",
            func.body.as_ref()
        )
        .into());
    };
    if *op != BinaryOperator::Add {
        return Err(format!("expected Add op, got {op:?}").into());
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
// Lower: group expression (parenthesized)
// =============================================================================

#[test]
fn test_lower_group_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let val: I32 = (1 + 2) * 3
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module.lets.iter().find(|l| l.name == "val").ok_or("val")?;
    let IrExpr::BinaryOp { op, .. } = &binding.value else {
        return Err(format!(
            "Expected BinaryOp from grouped expr, got {:?}",
            binding.value
        )
        .into());
    };
    if *op != BinaryOperator::Mul {
        return Err(format!("Expected Mul op, got {op:?}").into());
    }
    Ok(())
}

// =============================================================================
// Lower: optional struct field
// =============================================================================

#[test]
fn test_lower_optional_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { name: String?, count: I32 = 0 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    if field.name != "name" {
        return Err(format!("expected field 'name', got '{}'", field.name).into());
    }
    if !field.optional {
        return Err("name should be optional".into());
    }
    Ok(())
}

// =============================================================================
// Lower: method call dispatch kind
// =============================================================================

#[test]
fn test_lower_method_call_static_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    // A method on a concrete struct type should lower to a static-dispatch
    // MethodCall pointing at the impl block that provides the body.
    use formalang::ir::DispatchKind;

    let source = r"
        struct Counter { count: I32 = 0 }
        impl Counter {
            fn bump() -> I32 { self.count + 1 }
            fn bump_twice() -> I32 { self.bump() }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let impl_block = module.impls.first().ok_or("no impl block")?;
    let func = impl_block
        .functions
        .iter()
        .find(|f| f.name == "bump_twice")
        .ok_or("bump_twice not found")?;
    let body = func.body.as_ref().ok_or("no body on bump_twice")?;
    let IrExpr::MethodCall { dispatch, .. } = body else {
        return Err(format!("expected MethodCall, got {body:?}").into());
    };
    match dispatch {
        DispatchKind::Static { .. } => Ok(()),
        DispatchKind::Virtual { .. } => {
            Err("expected static dispatch on concrete struct, got virtual".into())
        }
    }
}

#[test]
fn test_lower_method_call_static_dispatch_on_struct_instance(
) -> Result<(), Box<dyn std::error::Error>> {
    // A method invoked on a named struct instance should lower to static
    // dispatch pointing at the impl block that registers the method.
    use formalang::ir::DispatchKind;

    let source = r"
        struct Counter { count: I32 = 0 }
        impl Counter {
            fn bump() -> I32 { self.count + 1 }
        }
        pub fn entry() -> I32 {
            Counter().bump()
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let func = module
        .functions
        .iter()
        .find(|f| f.name == "entry")
        .ok_or("entry not found")?;
    let body = func.body.as_ref().ok_or("no body")?;
    let IrExpr::MethodCall { dispatch, .. } = body else {
        return Err(format!("expected MethodCall, got {body:?}").into());
    };
    match dispatch {
        DispatchKind::Static { .. } => Ok(()),
        DispatchKind::Virtual { .. } => {
            Err("expected static dispatch on `Counter` instance, got virtual".into())
        }
    }
}

// =============================================================================
// Closure captures (§2 closure environment)
// =============================================================================

#[test]
fn test_closure_captures_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub fn make_adder(sink n: I32) -> (I32) -> I32 {
            |x: I32| x + n
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let body = module
        .functions
        .iter()
        .find(|f| f.name == "make_adder")
        .and_then(|f| f.body.as_ref())
        .ok_or("make_adder body missing")?;
    let IrExpr::Closure { captures, .. } = body else {
        return Err(format!("expected Closure, got {body:?}").into());
    };
    // The closure body references `n`, which is a parameter of make_adder.
    let captured: Vec<&str> = captures.iter().map(|(n, _, _)| n.as_str()).collect();
    if !captured.contains(&"n") {
        return Err(format!("expected `n` in captures, got {captured:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_no_captures_when_pure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub fn square() -> (I32) -> I32 {
            |x: I32| x * x
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let body = module
        .functions
        .iter()
        .find(|f| f.name == "square")
        .and_then(|f| f.body.as_ref())
        .ok_or("square body missing")?;
    let IrExpr::Closure { captures, .. } = body else {
        return Err(format!("expected Closure, got {body:?}").into());
    };
    if !captures.is_empty() {
        return Err(format!("expected no captures, got {captures:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_does_not_capture_own_params() -> Result<(), Box<dyn std::error::Error>> {
    // The closure's own parameters must not appear in its capture list.
    let source = r"
        pub fn combine(sink a: I32, sink b: I32) -> (I32) -> I32 {
            |x: I32| x + a + b
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let body = module
        .functions
        .iter()
        .find(|f| f.name == "combine")
        .and_then(|f| f.body.as_ref())
        .ok_or("combine body missing")?;
    let IrExpr::Closure { captures, .. } = body else {
        return Err(format!("expected Closure, got {body:?}").into());
    };
    let names: Vec<&str> = captures.iter().map(|(n, _, _)| n.as_str()).collect();
    if names.contains(&"x") {
        return Err(format!("closure param `x` leaked into captures: {names:?}").into());
    }
    if !names.contains(&"a") || !names.contains(&"b") {
        return Err(format!("expected captures for `a` and `b`, got {names:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_captures_inherit_sink_param_convention() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #32: a closure capturing a `sink` parameter records `Sink`
    // (so backends know ownership transferred to the closure).
    let source = r"
        pub fn make_adder(sink n: I32) -> (I32) -> I32 {
            |x: I32| x + n
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let body = module
        .functions
        .iter()
        .find(|f| f.name == "make_adder")
        .and_then(|f| f.body.as_ref())
        .ok_or("make_adder body missing")?;
    let IrExpr::Closure { captures, .. } = body else {
        return Err(format!("expected Closure, got {body:?}").into());
    };
    let n_conv = captures
        .iter()
        .find_map(|(name, c, _)| (name == "n").then_some(*c))
        .ok_or("expected `n` capture")?;
    if n_conv != ParamConvention::Sink {
        return Err(format!("expected `n` captured as Sink, got {n_conv:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_captures_inherit_module_let_conventions() -> Result<(), Box<dyn std::error::Error>>
{
    // Audit #32: a closure declared at module level inherits its captures'
    // conventions from the enclosing `let` / `let mut` bindings.
    let source = r"
        let immutable: I32 = 1
        pub let mut mutable: I32 = 2
        let c: () -> I32 = () -> immutable + mutable
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let c = module
        .lets
        .iter()
        .find(|l| l.name == "c")
        .ok_or("c missing")?;
    let IrExpr::Closure { captures, .. } = &c.value else {
        return Err(format!("expected Closure, got {:?}", c.value).into());
    };
    let by_name: std::collections::HashMap<&str, ParamConvention> = captures
        .iter()
        .map(|(n, conv, _)| (n.as_str(), *conv))
        .collect();
    let imm = by_name
        .get("immutable")
        .ok_or("expected `immutable` capture")?;
    let mu = by_name.get("mutable").ok_or("expected `mutable` capture")?;
    if *imm != ParamConvention::Let {
        return Err(format!("expected `immutable` captured as Let, got {imm:?}").into());
    }
    if *mu != ParamConvention::Mut {
        return Err(format!("expected `mutable` captured as Mut, got {mu:?}").into());
    }
    Ok(())
}
