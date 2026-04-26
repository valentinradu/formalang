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

// =============================================================================
// Tier-1 audit (item B): IR lowering surfaces unresolved type names as
// `UndefinedType` instead of silently producing `TypeParam(name)`. This
// catches typos and out-of-scope generic parameter references that
// would otherwise leak through to monomorphisation.
// =============================================================================

#[test]
fn test_unresolved_type_in_struct_field_is_loud() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Holder { value: Numbr }
    ";
    let result = compile_to_ir(source);
    let errors = result.err().ok_or("expected an UndefinedType error")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::UndefinedType { name, .. } if name == "Numbr"))
    {
        return Err(format!("expected UndefinedType for `Numbr`, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_in_scope_generic_param_still_lowers_as_typeparam() -> Result<(), Box<dyn std::error::Error>>
{
    // Sanity: a real generic parameter must NOT trigger the new
    // UndefinedType error — it should still resolve to TypeParam(name).
    let source = r"
        pub struct Box<T> { value: T }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let s = module
        .structs
        .iter()
        .find(|s| s.name == "Box")
        .ok_or("Box struct missing")?;
    let value_field = s
        .fields
        .iter()
        .find(|f| f.name == "value")
        .ok_or("value field missing")?;
    if !matches!(&value_field.ty, formalang::ir::ResolvedType::TypeParam(n) if n == "T") {
        return Err(format!("expected value: TypeParam(T), got {:?}", value_field.ty).into());
    }
    Ok(())
}

// =============================================================================
// Tier-1 audit (item D): Inline / no_inline / cold codegen attributes
// surface as keyword prefixes on `fn` and round-trip through the IR.
// =============================================================================

#[test]
fn test_inline_attribute_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::FunctionAttribute;
    let source = r"
        pub inline fn fast(x: Number) -> Number { x + 1 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "fast")
        .ok_or("fast missing")?;
    if f.attributes != vec![FunctionAttribute::Inline] {
        return Err(format!("expected [Inline], got {:?}", f.attributes).into());
    }
    Ok(())
}

#[test]
fn test_multiple_attributes_preserve_order() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::FunctionAttribute;
    let source = r"
        cold no_inline fn rare() -> Number { 0 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "rare")
        .ok_or("rare missing")?;
    if f.attributes != vec![FunctionAttribute::Cold, FunctionAttribute::NoInline] {
        return Err(format!(
            "expected [Cold, NoInline] in source order, got {:?}",
            f.attributes
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_carries_attributes() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::FunctionAttribute;
    let source = r"
        pub cold extern fn abort() -> Never
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "abort")
        .ok_or("abort missing")?;
    if !f.is_extern() {
        return Err("expected is_extern: true".into());
    }
    if f.attributes != vec![FunctionAttribute::Cold] {
        return Err(format!("expected [Cold], got {:?}", f.attributes).into());
    }
    Ok(())
}

// =============================================================================
// Tier-1 audit (item E): extern fn carries an explicit calling
// convention. Bare `extern fn` defaults to C; `extern "C"` and
// `extern "system"` are accepted; unknown ABI strings are rejected.
// =============================================================================

#[test]
fn test_extern_fn_default_abi_is_c() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::ExternAbi;
    let module = compile_to_ir("extern fn fetch(url: String) -> String")
        .map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "fetch")
        .ok_or("fetch missing")?;
    if f.extern_abi != Some(ExternAbi::C) {
        return Err(format!("expected Some(C), got {:?}", f.extern_abi).into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_explicit_c_abi() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::ExternAbi;
    let module = compile_to_ir(r#"extern "C" fn read(fd: Number) -> Number"#)
        .map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "read")
        .ok_or("read missing")?;
    if f.extern_abi != Some(ExternAbi::C) {
        return Err(format!("expected Some(C), got {:?}", f.extern_abi).into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_system_abi() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::ExternAbi;
    let module = compile_to_ir(r#"extern "system" fn GetTickCount() -> Number"#)
        .map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "GetTickCount")
        .ok_or("GetTickCount missing")?;
    if f.extern_abi != Some(ExternAbi::System) {
        return Err(format!("expected Some(System), got {:?}", f.extern_abi).into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_unknown_abi_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let result = compile_to_ir(r#"extern "rustcall" fn weird() -> Number"#);
    if result.is_ok() {
        return Err("expected unknown ABI to be rejected at parse time".into());
    }
    Ok(())
}

#[test]
fn test_regular_fn_has_no_extern_abi() -> Result<(), Box<dyn std::error::Error>> {
    let module = compile_to_ir("pub fn double(n: Number) -> Number { n + n }")
        .map_err(|e| format!("expected success: {e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "double")
        .ok_or("double missing")?;
    if f.extern_abi.is_some() {
        return Err(format!("expected None, got {:?}", f.extern_abi).into());
    }
    Ok(())
}

// =============================================================================
// Tier-1 audit (item G): nested-module hierarchy is preserved on
// `IrModule.modules` so backends can reconstruct namespaced output
// without re-parsing qualified names.
// =============================================================================

#[test]
fn test_module_tree_records_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod shapes {
            pub struct Circle { radius: Number }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let shapes = module
        .modules
        .iter()
        .find(|n| n.name == "shapes")
        .ok_or("expected `shapes` module node")?;
    if shapes.structs.len() != 1 {
        return Err(format!("expected 1 struct in shapes, got {}", shapes.structs.len()).into());
    }
    let circle_id = shapes.structs[0];
    let circle = module
        .get_struct(circle_id)
        .ok_or("Circle id does not resolve")?;
    if circle.name != "shapes::Circle" {
        return Err(format!("expected qualified name, got {}", circle.name).into());
    }
    Ok(())
}

#[test]
fn test_module_tree_preserves_two_level_nesting() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        mod outer {
            mod inner {
                pub struct Deep { x: Number }
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let outer = module
        .modules
        .iter()
        .find(|n| n.name == "outer")
        .ok_or("expected `outer` module")?;
    let inner = outer
        .modules
        .iter()
        .find(|n| n.name == "inner")
        .ok_or("expected `inner` nested under `outer`")?;
    if inner.structs.is_empty() {
        return Err("expected Deep struct id in inner".into());
    }
    Ok(())
}

// =============================================================================
// Tier-1 escape analysis extension: a closure that captures a function-
// local binding cannot escape the function frame even when wrapped in
// an aggregate (struct, enum, tuple, array, dict).
// =============================================================================

#[test]
fn test_struct_returned_with_closure_capturing_local_rejected(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Box { callback: () -> Number }
        fn make() -> Box {
            let local: Number = 1
            Box(callback: () -> local)
        }
    ";
    let result = compile(source);
    let errors = result
        .err()
        .ok_or("expected ClosureCaptureEscapesLocalBinding")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }))
    {
        return Err(format!(
            "expected ClosureCaptureEscapesLocalBinding when struct-wrapped closure captures \
             a function-local; got {errors:?}"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_struct_returned_with_closure_capturing_module_let_ok(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let factor: Number = 2
        struct Box { callback: () -> Number }
        fn make() -> Box {
            Box(callback: () -> factor)
        }
    ";
    compile(source).map_err(|e| format!("expected success: {e:?}"))?;
    Ok(())
}

#[test]
fn test_closure_assigned_to_outer_mut_binding_capturing_local_rejected(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn outer() -> Number {
            let mut f: () -> Number = () -> 0
            {
                let local: Number = 5
                f = () -> local
            }
            f()
        }
    ";
    let result = compile(source);
    let errors = result
        .err()
        .ok_or("expected ClosureCaptureEscapesLocalBinding")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }))
    {
        return Err(format!(
            "expected ClosureCaptureEscapesLocalBinding when closure assigned to outer mut \
             binding captures inner-scope local; got {errors:?}"
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_tuple_returned_with_closure_capturing_local_rejected(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn make() -> (n: Number, f: () -> Number) {
            let local: Number = 7
            (n: 0, f: () -> local)
        }
    ";
    let result = compile(source);
    let errors = result
        .err()
        .ok_or("expected ClosureCaptureEscapesLocalBinding")?;
    if !errors
        .iter()
        .any(|e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }))
    {
        return Err(format!(
            "expected ClosureCaptureEscapesLocalBinding for tuple-wrapped escape; got {errors:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Tier-1 audit (item E2): `Trait` in a value-producing type position
// (param, return, field, let annotation, closure params/return) is a
// hard error. FormaLang has no dynamic dispatch; trait-bounded values
// must go through generic parameters so the concrete type is known
// after monomorphisation.
// =============================================================================

#[test]
fn test_trait_as_function_param_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Drawable { fn draw(self) -> Number }
        fn render(d: Drawable) -> Number { 0 }
    ";
    let errors = compile(source)
        .err()
        .ok_or("expected TraitUsedAsValueType")?;
    if !errors.iter().any(|e| {
        matches!(
            e,
            CompilerError::TraitUsedAsValueType { trait_name, .. } if trait_name == "Drawable"
        )
    }) {
        return Err(format!("expected TraitUsedAsValueType, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_trait_as_let_annotation_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Drawable { fn draw(self) -> Number }
        struct Circle { r: Number }
        impl Drawable for Circle { fn draw(self) -> Number { 0 } }
        let d: Drawable = Circle(r: 1)
    ";
    let errors = compile(source)
        .err()
        .ok_or("expected TraitUsedAsValueType")?;
    if !errors.iter().any(|e| {
        matches!(
            e,
            CompilerError::TraitUsedAsValueType { trait_name, .. } if trait_name == "Drawable"
        )
    }) {
        return Err(format!("expected TraitUsedAsValueType, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_trait_as_struct_field_type_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Drawable { fn draw(self) -> Number }
        struct Container { d: Drawable }
    ";
    let errors = compile(source)
        .err()
        .ok_or("expected TraitUsedAsValueType")?;
    if !errors.iter().any(|e| {
        matches!(
            e,
            CompilerError::TraitUsedAsValueType { trait_name, .. } if trait_name == "Drawable"
        )
    }) {
        return Err(format!("expected TraitUsedAsValueType, got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_trait_as_generic_constraint_ok() -> Result<(), Box<dyn std::error::Error>> {
    // `<T: Drawable>` is the canonical legal form — Drawable here is a
    // *constraint*, not a value type. Should compile.
    let source = r"
        trait Drawable { fn draw(self) -> Number }
        struct Circle { r: Number }
        impl Drawable for Circle { fn draw(self) -> Number { 0 } }
        fn render<T: Drawable>(d: T) -> Number { 0 }
    ";
    compile(source).map_err(|e| format!("expected success: {e:?}"))?;
    Ok(())
}

#[test]
fn test_trait_as_impl_target_ok() -> Result<(), Box<dyn std::error::Error>> {
    // `impl Trait for Foo` is the canonical legal form — Drawable is
    // an *impl target*, not a value type.
    let source = r"
        trait Drawable { fn draw(self) -> Number }
        struct Circle { r: Number }
        impl Drawable for Circle { fn draw(self) -> Number { 0 } }
    ";
    compile(source).map_err(|e| format!("expected success: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Tier-1 audit (item E2 / Phase 2d + 2e): mono now specialises generic
// functions and devirtualises trait-bounded calls inside them. End-to-
// end check: `paint<T: Drawable>(shape: T)` invoked with Circle yields
// `paint__Circle` whose body has Static dispatch on the Drawable impl.
// =============================================================================

fn walk_for_dispatch(
    expr: &formalang::ir::IrExpr,
    found_static: &mut bool,
    found_virtual_concrete: &mut bool,
) {
    use formalang::ir::{DispatchKind, IrExpr};
    if let IrExpr::MethodCall {
        method, dispatch, ..
    } = expr
    {
        if method == "area" {
            match dispatch {
                DispatchKind::Static { .. } => *found_static = true,
                DispatchKind::Virtual { .. } => *found_virtual_concrete = true,
            }
        }
    }
    if let IrExpr::Block {
        statements, result, ..
    } = expr
    {
        for stmt in statements {
            if let formalang::ir::IrBlockStatement::Expr(e) = stmt {
                walk_for_dispatch(e, found_static, found_virtual_concrete);
            }
        }
        walk_for_dispatch(result, found_static, found_virtual_concrete);
    }
}

#[test]
fn test_monomorphise_devirtualises_trait_bounded_call() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    let source = r"
        trait Drawable { fn area(self) -> Number }
        struct Circle { r: Number }
        impl Drawable for Circle {
            fn area(self) -> Number { self.r }
        }
        pub fn paint<T: Drawable>(shape: T) -> Number {
            shape.area()
        }
        pub let p: Number = paint(Circle(r: 1))
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass::default());
    let result = pipeline.run(module).map_err(|e| format!("{e:?}"))?;
    let paint_specialised = result
        .functions
        .iter()
        .find(|f| f.name.starts_with("paint__"))
        .ok_or("expected paint__... specialised function")?;
    let body = paint_specialised
        .body
        .as_ref()
        .ok_or("paint specialised body missing")?;
    let mut found_static = false;
    let mut found_virtual_concrete = false;
    walk_for_dispatch(body, &mut found_static, &mut found_virtual_concrete);
    if found_virtual_concrete {
        return Err("paint__Circle still contains Virtual dispatch on concrete receiver".into());
    }
    if !found_static {
        return Err("expected paint__Circle body to dispatch via Static".into());
    }
    // Generic original should be gone.
    if result.functions.iter().any(|f| f.name == "paint") {
        return Err("generic `paint` should have been dropped after specialisation".into());
    }
    Ok(())
}

#[test]
fn test_monomorphise_specialises_generic_function() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    // Simpler check focused on Phase 2e alone (no trait dispatch).
    let source = r#"
        pub fn identity<T>(x: T) -> T { x }
        pub let n: Number = identity(1)
        pub let s: String = identity("hi")
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass::default());
    let result = pipeline.run(module).map_err(|e| format!("{e:?}"))?;
    if result.functions.iter().any(|f| f.name == "identity") {
        return Err("generic `identity` should have been dropped".into());
    }
    let specialised: Vec<&str> = result
        .functions
        .iter()
        .filter(|f| f.name.starts_with("identity__"))
        .map(|f| f.name.as_str())
        .collect();
    if specialised.len() != 2 {
        return Err(format!(
            "expected 2 identity specialisations (Number + String), got: {specialised:?}"
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Generic traits PR: a generic trait used as a constraint with concrete
// args specialises end-to-end via MonomorphisePass.
// =============================================================================

#[test]
fn test_generic_trait_specialises() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    let source = r"
        pub trait Eq<T> {
            fn eq(self, other: T) -> Boolean
        }
        pub struct Number2 { value: Number }
        impl Eq<Number2> for Number2 {
            fn eq(self, other: Number2) -> Boolean { true }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass::default());
    let result = pipeline.run(module).map_err(|e| format!("mono: {e:?}"))?;
    // Original generic Eq is gone; specialised clone Eq__... remains.
    if result.traits.iter().any(|t| t.name == "Eq") {
        return Err("generic `Eq` should have been dropped".into());
    }
    if !result.traits.iter().any(|t| t.name.starts_with("Eq__")) {
        return Err(format!(
            "expected Eq__... specialisation, got: {:?}",
            result.traits.iter().map(|t| &t.name).collect::<Vec<_>>()
        )
        .into());
    }
    // The impl block targets Number2 with the specialised trait_id.
    let number2_id = result
        .structs
        .iter()
        .position(|s| s.name == "Number2")
        .and_then(|i| u32::try_from(i).ok().map(formalang::StructId))
        .ok_or("Number2 missing")?;
    let imp = result
        .impls
        .iter()
        .find(|i| matches!(i.target, formalang::ir::ImplTarget::Struct(id) if id == number2_id))
        .ok_or("Number2 impl missing")?;
    let tr_ref = imp.trait_ref.as_ref().ok_or("impl missing trait_ref")?;
    if !tr_ref.args.is_empty() {
        return Err(format!(
            "expected specialised trait_ref to have empty args, got {:?}",
            tr_ref.args
        )
        .into());
    }
    let trait_def = result
        .get_trait(tr_ref.trait_id)
        .ok_or("trait_id does not resolve")?;
    if !trait_def.name.starts_with("Eq__") {
        return Err(format!("expected impl to point at Eq__..., got: {}", trait_def.name).into());
    }
    Ok(())
}

#[test]
fn test_generic_trait_constraint_specialises() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::MonomorphisePass;
    // A generic-bounded function constrained on `Container<Number>`
    // should also drive specialisation of Container.
    let source = r"
        pub trait Container<T> {
            fn get(self) -> Number
        }
        pub struct Box { value: Number }
        impl Container<Number> for Box {
            fn get(self) -> Number { self.value }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut pipeline = formalang::Pipeline::new().pass(MonomorphisePass::default());
    let result = pipeline.run(module).map_err(|e| format!("mono: {e:?}"))?;
    if result.traits.iter().any(|t| t.name == "Container") {
        return Err("generic Container should be dropped after specialisation".into());
    }
    if !result
        .traits
        .iter()
        .any(|t| t.name.starts_with("Container__"))
    {
        return Err("expected Container__... specialisation".into());
    }
    Ok(())
}

#[test]
fn test_top_level_definitions_not_in_module_tree() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Top { x: Number }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    if !module.modules.is_empty() {
        return Err(format!(
            "expected no nested modules, got {} entries",
            module.modules.len()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_impl_method_attribute() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::FunctionAttribute;
    let source = r"
        pub struct Counter { n: Number = 0 }
        impl Counter {
            inline fn next(self) -> Number { self.n + 1 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("expected success: {e:?}"))?;
    let imp = module.impls.first().ok_or("no impl")?;
    let next = imp
        .functions
        .iter()
        .find(|f| f.name == "next")
        .ok_or("next missing")?;
    if next.attributes != vec![FunctionAttribute::Inline] {
        return Err(format!("expected [Inline], got {:?}", next.attributes).into());
    }
    Ok(())
}
