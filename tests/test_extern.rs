//! Tests for extern declarations (#1)
//!
//! Covers: extern type, extern fn, extern impl, and all error cases.

use formalang::{compile, parse_only, CompilerError};

// =============================================================================
// extern type
// =============================================================================

#[test]
fn test_extern_type_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_type_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Buffer<T>
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_type_pub() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
pub extern type Handle
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// extern fn (module-level)
// =============================================================================

#[test]
fn test_extern_fn_no_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern fn create_canvas() -> Canvas
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_fn_with_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern fn render(canvas: Canvas, width: Number, height: Number) -> Boolean
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_fn_no_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern fn flush(canvas: Canvas)
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// extern impl
// =============================================================================

#[test]
fn test_extern_impl_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern impl Canvas {
    fn draw(self, x: Number, y: Number)
    fn width(self) -> Number
    fn height(self) -> Number
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_impl_with_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Handle
extern impl Handle {
    fn id(self) -> Number
    fn is_valid(self) -> Boolean
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_regular_impl_and_extern_impl_coexist() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Socket
extern impl Socket {
    fn send(self, data: String) -> Boolean
    fn close(self)
}
impl Socket {
    fn send_text(self, text: String) -> Boolean {
        self.send(data: text)
    }
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error: extern fn with a body
// =============================================================================

#[test]
fn test_extern_fn_with_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern fn compute() -> Number {
    42
}
"#;
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: extern fn must not have a body".into());
    }
    Ok(())
}

// =============================================================================
// Error: regular fn without a body
// =============================================================================

#[test]
fn test_regular_fn_without_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn compute() -> Number
"#;
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: non-extern fn must have a body".into());
    }
    Ok(())
}

#[test]
fn test_impl_fn_without_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
struct Foo {}
impl Foo {
    fn bar(self) -> Number
}
"#;
    let errors = compile(source)
        .err()
        .ok_or("expected error: impl fn without body")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::RegularFnWithoutBody { .. }));
    if !has_error {
        return Err(format!("expected RegularFnWithoutBody, got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Error: extern impl fn with a body
// =============================================================================

#[test]
fn test_extern_impl_fn_with_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern impl Canvas {
    fn draw(self) {
        42
    }
}
"#;
    let errors = compile(source)
        .err()
        .ok_or("expected error: extern impl fn has body")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::ExternImplWithBody { .. }));
    if !has_error {
        return Err(format!("expected ExternImplWithBody, got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Extern type: field access on extern type is allowed (backend validates fields)
// =============================================================================

#[test]
fn test_field_access_on_extern_type_allowed() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern fn get_canvas() -> Canvas

fn use_canvas() -> Number {
    let c = get_canvas()
    c.width
}
"#;
    // Field access on extern types is allowed at the language level.
    // The backend is responsible for validating extern type field access.
    compile(source).map_err(|e| format!("unexpected compile error: {e:?}"))?;
    Ok(())
}

// =============================================================================
// Extern type: calling extern methods is allowed
// =============================================================================

#[test]
fn test_extern_method_call_allowed() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
extern type Canvas
extern fn create() -> Canvas
extern impl Canvas {
    fn width(self) -> Number
}

fn get_width() -> Number {
    let c = create()
    c.width()
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// AST: FnDef.body is Option — verify via parse_only + AST inspection
// =============================================================================

#[test]
fn test_extern_fn_ast_body_is_none() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::{
        ast::{Definition, Statement},
        parse_only,
    };

    let source = r#"
extern fn compute() -> Number
"#;
    let file = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let def = file
        .statements
        .iter()
        .find_map(|s| {
            if let Statement::Definition(d) = s {
                if let Definition::Function(f) = &**d {
                    return Some(f.as_ref());
                }
            }
            None
        })
        .ok_or("expected a FunctionDef")?;

    if def.body.is_some() {
        return Err("extern fn should have body = None".into());
    }
    Ok(())
}

#[test]
fn test_regular_fn_ast_body_is_some() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::{
        ast::{Definition, Statement},
        parse_only,
    };

    let source = r#"
fn compute() -> Number {
    42
}
"#;
    let file = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let def = file
        .statements
        .iter()
        .find_map(|s| {
            if let Statement::Definition(d) = s {
                if let Definition::Function(f) = &**d {
                    return Some(f.as_ref());
                }
            }
            None
        })
        .ok_or("expected a FunctionDef")?;

    if def.body.is_none() {
        return Err("regular fn should have body = Some(_)".into());
    }
    Ok(())
}
