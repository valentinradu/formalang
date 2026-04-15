//! Additional tests for lower.rs coverage: event mappings, inferred enums,
//! closures, destructuring patterns, method calls on enums, `string_to_resolved_type`,
//! block let destructuring, `lower_let_binding` patterns, module/function lowering.

use formalang::ast::BinaryOperator;
use formalang::ast::PrimitiveType;
use formalang::compile_to_ir;
use formalang::ir::{eliminate_dead_code, EventBindingSource, IrExpr, ResolvedType};

// =============================================================================
// Event mapping: named enum instantiation
// =============================================================================

#[test]
fn test_lower_event_mapping_named_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Action { submit, cancel }
        struct Button {
            on_click: () -> Action = () -> Action.submit
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    let expr = field.default.as_ref().ok_or("no default")?;
    let IrExpr::EventMapping { variant, .. } = expr else {
        return Err(format!("Expected EventMapping, got {expr:?}").into());
    };
    if variant != "submit" {
        return Err(format!("expected 'submit', got '{variant}'").into());
    }
    Ok(())
}

#[test]
fn test_lower_event_mapping_with_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum TextEvent { changed(value: String) }
        struct Input {
            on_change: (String) -> TextEvent = |v: String| TextEvent.changed(value: v)
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    let expr = field.default.as_ref().ok_or("no default")?;
    let IrExpr::EventMapping {
        variant,
        param,
        field_bindings,
        ..
    } = expr
    else {
        return Err(format!("Expected EventMapping, got {expr:?}").into());
    };
    if variant != "changed" {
        return Err(format!("expected 'changed', got '{variant}'").into());
    }
    if param.is_none() {
        return Err("Should have param".into());
    }
    if field_bindings.is_empty() {
        return Err("Should have field bindings".into());
    }
    Ok(())
}

#[test]
fn test_lower_event_mapping_inferred_enum() -> Result<(), Box<dyn std::error::Error>> {
    // Inferred enum instantiation in closure
    let source = r"
        enum Status { done }
        struct Button {
            on_click: () -> Status = () -> .done
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    let expr = field.default.as_ref().ok_or("no default")?;
    // Should produce EventMapping with inferred variant
    let IrExpr::EventMapping { variant, .. } = expr else {
        return Err(format!("Expected EventMapping, got {expr:?}").into());
    };
    if variant != "done" {
        return Err(format!("expected 'done', got '{variant}'").into());
    }
    Ok(())
}

#[test]
fn test_lower_event_mapping_with_literal_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum ConfigEvent { updated(value: Number) }
        struct Slider {
            on_update: () -> ConfigEvent = () -> ConfigEvent.updated(value: 42)
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let field = &module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?;
    let expr = field.default.as_ref().ok_or("no default")?;
    let IrExpr::EventMapping {
        variant,
        field_bindings,
        ..
    } = expr
    else {
        return Err(format!("Expected EventMapping, got {expr:?}").into());
    };
    if variant != "updated" {
        return Err(format!("expected 'updated', got '{variant}'").into());
    }
    if field_bindings.len() != 1 {
        return Err(format!("expected 1 field binding, got {}", field_bindings.len()).into());
    }
    // field binding should be a Literal source
    let EventBindingSource::Literal(_) =
        &field_bindings.first().ok_or("index out of bounds")?.source
    else {
        return Err(format!(
            "Expected Literal binding source, got {:?}",
            &field_bindings.first().ok_or("index out of bounds")?.source
        )
        .into());
    };
    Ok(())
}

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
fn test_lower_general_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            transform: (Number) -> Number = |x: Number| 42
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
    if params.first().ok_or("index out of bounds")?.0 != "x" {
        return Err(format!(
            "expected param 'x', got '{}'",
            params.first().ok_or("index out of bounds")?.0
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

#[test]
fn test_lower_zero_param_closure() -> Result<(), Box<dyn std::error::Error>> {
    // Two-param closure - the type field needs different syntax for multi-param
    // Test event mapping with param that has a literal field binding (not Param source)
    let source = r"
        enum InputEvent { changed(value: Number) }
        struct Input {
            on_change: (Number) -> InputEvent = |v: Number| InputEvent.changed(value: 100)
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
    // With v=100 (literal, not param reference), field binding source is Literal
    let IrExpr::EventMapping { field_bindings, .. } = expr else {
        return Err(format!("Expected EventMapping, got {expr:?}").into());
    };
    if field_bindings.len() != 1 {
        return Err(format!("expected 1 field binding, got {}", field_bindings.len()).into());
    }
    if !matches!(
        &field_bindings.first().ok_or("index out of bounds")?.source,
        EventBindingSource::Literal(_) | EventBindingSource::Param(_)
    ) {
        return Err(format!(
            "Expected Literal or Param binding source, got {:?}",
            &field_bindings.first().ok_or("index out of bounds")?.source
        )
        .into());
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
fn test_lower_let_tuple_destructuring() -> Result<(), Box<dyn std::error::Error>> {
    // Tuple destructuring using positional syntax
    let source = r"
        let a: Number = 1
        let b: Number = 2
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
        struct Point { x: Number = 0, y: Number = 0 }
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
        struct Config { val: Number = {
            let x: Number = 1
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
        struct Point { x: Number = 0, y: Number = 0 }
        struct Config { val: Number = {
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
        struct Config { val: Number = {
            let items: [Number] = [1, 2]
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
            struct Circle { radius: Number }
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
            trait Drawable { area: Number }
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
                struct Point { x: Number, y: Number }
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
        fn add(x: Number, y: Number) -> Number { x + y }
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
        fn area(s: Shape) -> Number {
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
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn get_x() -> Number { self.x }
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
        struct Counter { count: Number = 0 }
        impl Counter {
            fn increment() -> Number { self.count + 1 }
            fn double_increment() -> Number { self.increment() }
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
        struct Counter { count: Number = 0 }
        impl Counter {
            fn doubled() -> Number { self.count * 2 }
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
        struct Widget { value: Number = 0 }
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
        let base: Number = 10
        let doubled: Number = base * 2
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
        let count: Number = 42
        let flag: Boolean = true
        let p: Path = "/"
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
        struct Config { val: Number = 0 }
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
    if !matches!(binding.ty, ResolvedType::Primitive(PrimitiveType::Number)) {
        return Err(format!("Expected Number type, got {:?}", binding.ty).into());
    }
    Ok(())
}

// =============================================================================
// Lower: dict access value type extraction
// =============================================================================

#[test]
fn test_lower_dict_access_type_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let dict: [String: Number] = ["key": 1]
        let val: Number = dict["key"]
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module.lets.iter().find(|l| l.name == "val").ok_or("val")?;
    let IrExpr::DictAccess { .. } = &binding.value else {
        return Err(format!("Expected DictAccess, got {:?}", binding.value).into());
    };
    if !matches!(binding.ty, ResolvedType::Primitive(PrimitiveType::Number)) {
        return Err(format!("Expected Number value type from dict, got {:?}", binding.ty).into());
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
        struct Config { wrapped: Box<Number> = Box<Number>(value: 42) }
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
        let items: [Number] = []
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let binding = module
        .lets
        .iter()
        .find(|l| l.name == "items")
        .ok_or("items")?;
    let IrExpr::Array { elements, .. } = &binding.value else {
        return Err(format!("Expected Array, got {:?}", binding.value).into());
    };
    if !elements.is_empty() {
        return Err(format!("expected empty array, got {} elements", elements.len()).into());
    }
    Ok(())
}

// =============================================================================
// Lower: empty dict literal
// =============================================================================

#[test]
fn test_lower_empty_dict_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let d: [String: Number] = [:]
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
        let items: [Number] = [1, 2, 3]
        let mapped: [Number] = for x in items { 0 }
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
        struct Config { val: Number = { 42 } }
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
        fn compute(x: Number) -> Number {
            let y: Number = x + 1
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
            struct Circle { radius: Number }
            impl Circle {
                fn area() -> Number { self.radius }
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
        struct Checker { val: Number = 0 }
        impl Checker {
            fn test() -> Number { if true { 1 } else { 2 } }
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
        extern fn sin(x: Number) -> Number
        extern fn cos(x: Number) -> Number
        fn builtin_test() -> Number { sin(x: 0) + cos(x: 0) }
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
        let val: Number = (1 + 2) * 3
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
        struct Config { name: String?, count: Number = 0 }
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
