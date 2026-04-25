//! Tests for all 11 compiler gaps implemented in the gap-filling pass.

#![allow(clippy::indexing_slicing)]

use formalang::compile_to_ir;
use formalang::CompilerError;

// =============================================================================
// Gap 1: Assignment type checking
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_assignment_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Assigning a String to a Number binding should produce TypeMismatch
    let source = r#"
        fn f() -> Number {
            let mut n: Number = 1
            n = "text"
            n
        }
    "#;
    let result = compile(source);
    let errors = result.err().ok_or("expected TypeMismatch error")?;
    let has_mismatch = errors
        .iter()
        .any(|e| matches!(e, CompilerError::TypeMismatch { .. }));
    if !has_mismatch {
        return Err(format!("expected TypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_assignment_type_match_ok() -> Result<(), Box<dyn std::error::Error>> {
    // Assigning Number to a Number binding should succeed
    let source = r"
        fn f() -> Number {
            let mut n: Number = 1
            n = 2
            n
        }
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 2: Default field value type checking
// =============================================================================

#[test]
fn test_struct_default_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Providing a String default for a Number field should produce TypeMismatch
    let source = r#"
        struct S { x: Number = "text" }
    "#;
    let result = compile(source);
    let errors = result.err().ok_or("expected TypeMismatch error")?;
    let has_mismatch = errors
        .iter()
        .any(|e| matches!(e, CompilerError::TypeMismatch { .. }));
    if !has_mismatch {
        return Err(format!("expected TypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_struct_default_type_ok() -> Result<(), Box<dyn std::error::Error>> {
    // A Number default for a Number field should succeed
    let source = r"
        struct S { x: Number = 0 }
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 3: Generic struct constraint validation
// =============================================================================

#[test]
fn test_generic_struct_constraint_violation() -> Result<(), Box<dyn std::error::Error>> {
    // Container<T: Named> instantiated with a type that doesn't implement Named
    let source = r"
        trait Named { name: String }
        struct Container<T: Named> { value: T }
        struct Plain { x: Number }
        let c = Container<Plain>(value: Plain(x: 1))
    ";
    let result = compile(source);
    let errors = result
        .err()
        .ok_or("expected GenericConstraintViolation error")?;
    let has_violation = errors
        .iter()
        .any(|e| matches!(e, CompilerError::GenericConstraintViolation { .. }));
    if !has_violation {
        return Err(format!("expected GenericConstraintViolation, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_generic_struct_constraint_ok() -> Result<(), Box<dyn std::error::Error>> {
    // Container<T: Named> instantiated with a type that explicitly implements Named
    let source = r#"
        trait Named { name: String }
        struct Container<T: Named> { value: T }
        struct Person { name: String }
        impl Named for Person {}
        let c = Container<Person>(value: Person(name: "Alice"))
    "#;
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 4: Module visibility enforcement
// =============================================================================

#[test]
fn test_private_item_from_outside_module() -> Result<(), Box<dyn std::error::Error>> {
    // Accessing a private struct from outside its module should give VisibilityViolation
    let source = r"
        mod shapes {
            struct Circle { radius: Number }
        }
        let c = shapes::Circle(radius: 5)
    ";
    let result = compile(source);
    let errors = result.err().ok_or("expected VisibilityViolation error")?;
    let has_violation = errors
        .iter()
        .any(|e| matches!(e, CompilerError::VisibilityViolation { .. }));
    if !has_violation {
        return Err(format!("expected VisibilityViolation, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_pub_item_from_outside_module() -> Result<(), Box<dyn std::error::Error>> {
    // Accessing a pub struct from outside its module should succeed
    let source = r"
        mod shapes {
            pub struct Circle { radius: Number }
        }
        let c = shapes::Circle(radius: 5)
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 5: Overloading Mode B (first-positional-arg type matching)
// =============================================================================

#[test]
fn test_overload_mode_b_number() -> Result<(), Box<dyn std::error::Error>> {
    // Two overloads differing only in first param type; call with Number should resolve
    let source = r#"
        fn process(n: Number) -> String { "number" }
        fn process(label: String, n: Number) -> String { "string" }
        let r = process(42)
    "#;
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_overload_mode_b_string() -> Result<(), Box<dyn std::error::Error>> {
    // Two overloads differing only in first param type; call with String should resolve
    let source = r#"
        fn process(n: Number) -> String { "number" }
        fn process(s: String) -> String { s }
        let r = process("hello")
    "#;
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 6: IrTrait fields preserved
// =============================================================================

#[test]
fn test_ir_trait_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named {
            name: String
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("IR error: {e:?}"))?;
    if module.traits.is_empty() {
        return Err("expected at least one trait in IR".into());
    }
    let trait_def = &module.traits[0];
    if trait_def.fields.is_empty() {
        return Err(format!(
            "expected trait fields to be preserved, got: {:?}",
            trait_def.fields
        )
        .into());
    }
    if trait_def.fields[0].name != "name" {
        return Err(format!(
            "expected field name 'name', got '{}'",
            trait_def.fields[0].name
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Gap 7: IR generic param constraints preserved
// =============================================================================

#[test]
fn test_ir_generic_param_constraints_preserved() -> Result<(), Box<dyn std::error::Error>> {
    // Struct with generic param that has a constraint: the IR must preserve the constraint.
    let source = r"
        trait Named { name: String }
        struct Container<T: Named> { value: T }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("IR error: {e:?}"))?;
    // Container is the second struct (Named trait comes first as IrTrait, Container as IrStruct)
    let container = module
        .structs
        .iter()
        .find(|s| s.name == "Container")
        .ok_or("Container struct not found in IR")?;
    if container.generic_params.is_empty() {
        return Err("expected generic params on Container".into());
    }
    let t_param = &container.generic_params[0];
    if t_param.constraints.is_empty() {
        return Err(format!("expected constraint on T, got empty. param: {t_param:?}").into());
    }
    Ok(())
}

// =============================================================================
// Gap 8: Dictionary literal type inference
// =============================================================================

#[test]
fn test_dict_literal_type_inferred() -> Result<(), Box<dyn std::error::Error>> {
    // The inferred type [String: Number] should match the declared type
    let source = r#"
        fn get_map() -> [String: Number] { ["a": 1] }
    "#;
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 9: Match arm type consistency
// =============================================================================

#[test]
fn test_match_arm_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // Match arms returning different non-Unknown types should produce TypeMismatch
    let source = r#"
        enum Color { red, blue }
        fn describe(c: Color) -> Number {
            match c {
                red: 1,
                blue: "text"
            }
        }
    "#;
    let result = compile(source);
    let errors = result.err().ok_or("expected TypeMismatch error")?;
    let has_mismatch = errors
        .iter()
        .any(|e| matches!(e, CompilerError::TypeMismatch { .. }));
    if !has_mismatch {
        return Err(format!("expected TypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_match_arm_type_ok() -> Result<(), Box<dyn std::error::Error>> {
    // Match arms all returning Number should succeed
    let source = r"
        enum Color { red, blue }
        fn describe(c: Color) -> Number {
            match c {
                red: 1,
                blue: 2
            }
        }
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 10: IfExpr branch type consistency
// =============================================================================

#[test]
fn test_if_branch_type_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    // If branches returning Number and String should produce TypeMismatch
    let source = r#"
        fn f(b: Boolean) -> Number {
            if b { 1 } else { "text" }
        }
    "#;
    let result = compile(source);
    let errors = result.err().ok_or("expected TypeMismatch error")?;
    let has_mismatch = errors
        .iter()
        .any(|e| matches!(e, CompilerError::TypeMismatch { .. }));
    if !has_mismatch {
        return Err(format!("expected TypeMismatch, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_if_branch_type_ok() -> Result<(), Box<dyn std::error::Error>> {
    // Both branches returning Number should succeed
    let source = r"
        fn f(b: Boolean) -> Number {
            if b { 1 } else { 2 }
        }
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Gap 11: Closure expression type inference
// =============================================================================

#[test]
fn test_closure_type_inferred() -> Result<(), Box<dyn std::error::Error>> {
    // Closure x -> x with declared type Number -> Number should compile
    // The inferred type "Number -> Number" now matches the annotation
    let source = r"
        let f: Number -> Number = x -> x
    ";
    compile(source).map_err(|e| format!("should succeed: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_body_mismatched_return_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B9: a closure literal with an untyped param used to have
    // the param resolve to "Unknown", which unified with anything and
    // hid genuine return-type mismatches in the body. Now the let
    // annotation seeds the param's type and the body is type-checked
    // against the declared return type.
    let source = r"
        let f: Number -> Boolean = x -> x + 1
    ";
    let err = compile(source)
        .err()
        .ok_or("expected TypeMismatch for body returning Number, declared Boolean")?;
    let has = err
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::TypeMismatch { expected, found, .. } if expected == "Boolean" && found == "Number"));
    if !has {
        return Err(format!("expected TypeMismatch(Boolean, Number), got {err:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_body_correct_return_type_accepted() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B9 positive case: an untyped-param closure whose body
    // returns the declared type still compiles.
    let source = r"
        let f: Number -> Number = x -> x + 1
    ";
    compile(source).map_err(|e| format!("expected success, got {e:?}"))?;
    Ok(())
}

#[test]
fn test_dict_literal_value_heterogeneity_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B11: dict literals were allowed to mix value types
    // silently. Now `["a": 1, "b": "two"]` must surface a TypeMismatch.
    let source = r#"
        let d = ["a": 1, "b": "two"]
    "#;
    let err = compile(source).err().ok_or("expected TypeMismatch")?;
    if !err
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::TypeMismatch { .. }))
    {
        return Err(format!("expected TypeMismatch, got {err:?}").into());
    }
    Ok(())
}

#[test]
fn test_dict_literal_key_heterogeneity_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B11: same for keys — `[1: "x", "two": "y"]` mixes Number
    // and String keys.
    let source = r#"
        let d = [1: "x", "two": "y"]
    "#;
    let err = compile(source).err().ok_or("expected TypeMismatch")?;
    if !err
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::TypeMismatch { .. }))
    {
        return Err(format!("expected TypeMismatch, got {err:?}").into());
    }
    Ok(())
}

#[test]
fn test_dict_literal_homogeneous_accepted() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B11 positive case: a dict whose every entry has the same
    // shape compiles.
    let source = r#"
        let d = ["a": 1, "b": 2, "c": 3]
    "#;
    compile(source).map_err(|e| format!("expected success, got {e:?}"))?;
    Ok(())
}

#[test]
fn test_let_general_type_mismatch_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B12: a let binding's value must be type-compatible with
    // its declared annotation. Previously only the nil-vs-non-optional
    // case was checked, so `let f: m::Foo = "wrong"` compiled silently.
    let source = r#"
        mod m {
            pub struct Foo { x: Number = 0 }
        }
        let f: m::Foo = "wrong"
    "#;
    let err = compile(source).err().ok_or("expected TypeMismatch")?;
    if !err
        .iter()
        .any(|e| matches!(e, formalang::CompilerError::TypeMismatch { .. }))
    {
        return Err(format!("expected TypeMismatch, got {err:?}").into());
    }
    Ok(())
}

#[test]
fn test_let_general_type_match_accepted() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B12 positive case.
    let source = r"
        mod m {
            pub struct Foo { x: Number = 0 }
        }
        let f: m::Foo = m::Foo()
    ";
    compile(source).map_err(|e| format!("expected success, got {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_arg_to_function_picks_up_expected_param_types(
) -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B19: a closure literal passed as an argument to a
    // function expecting a closure type used to lower with
    // `param_tys: [(_, TypeParam("Unknown"))]`. With bidirectional
    // inference at the call site, the closure now picks up the
    // function's declared param type.
    use formalang::ast::PrimitiveType;
    use formalang::ir::ResolvedType;
    let source = r"
        fn apply(f: Number -> Number, x: Number) -> Number {
            x
        }
        let result: Number = apply(x -> x, 1)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    // Find the `result` let; its value is the FunctionCall to apply.
    let result_let = module
        .lets
        .iter()
        .find(|l| l.name == "result")
        .ok_or("expected let result")?;
    let formalang::ir::IrExpr::FunctionCall { args, .. } = &result_let.value else {
        return Err(format!("expected FunctionCall, got {:?}", result_let.value).into());
    };
    let (_, first_arg) = args.first().ok_or("expected first arg")?;
    let formalang::ir::IrExpr::Closure { params, .. } = first_arg else {
        return Err(format!("expected Closure as first arg, got {first_arg:?}").into());
    };
    let (_, _, param_ty) = params.first().ok_or("expected at least one param")?;
    if !matches!(param_ty, ResolvedType::Primitive(PrimitiveType::Number)) {
        return Err(format!("expected closure param to lower as Number, got {param_ty:?}").into());
    }
    Ok(())
}

#[test]
fn test_closure_arg_to_method_picks_up_expected_param_types(
) -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B19 mirror: same bidirectional inference for method
    // call arguments. `target.run(x -> x + 1)` should give the
    // closure's `x` the method's declared param type.
    use formalang::ast::PrimitiveType;
    use formalang::ir::ResolvedType;
    let source = r"
        struct Engine { rpm: Number = 0 }
        impl Engine {
            fn run(self, f: Number -> Number) -> Number {
                self.rpm
            }
        }
        let e: Engine = Engine()
        let result: Number = e.run(x -> x)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let result_let = module
        .lets
        .iter()
        .find(|l| l.name == "result")
        .ok_or("expected let result")?;
    let formalang::ir::IrExpr::MethodCall { args, .. } = &result_let.value else {
        return Err(format!("expected MethodCall, got {:?}", result_let.value).into());
    };
    let (_, first_arg) = args.first().ok_or("expected first arg")?;
    let formalang::ir::IrExpr::Closure { params, .. } = first_arg else {
        return Err(format!("expected Closure as first arg, got {first_arg:?}").into());
    };
    let (_, _, param_ty) = params.first().ok_or("expected at least one param")?;
    if !matches!(param_ty, ResolvedType::Primitive(PrimitiveType::Number)) {
        return Err(format!("expected closure param to lower as Number, got {param_ty:?}").into());
    }
    Ok(())
}

#[test]
fn test_method_dispatch_on_qualified_receiver() -> Result<(), Box<dyn std::error::Error>> {
    // Audit2 B14: a method call on a value whose type is qualified
    // (`m::Foo`) used to fail with `UndefinedReference` because
    // `method_exists_on_type` only matched bare receiver names.
    let source = r"
        mod m {
            pub struct Foo { x: Number = 0 }
            impl Foo {
                fn double(self) -> Number { self.x + self.x }
            }
        }
        let f: m::Foo = m::Foo()
        let v: Number = f.double()
    ";
    compile(source).map_err(|e| format!("expected success, got {e:?}"))?;
    Ok(())
}
