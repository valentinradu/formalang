//! Tests for extern declarations (#1)
//!
//! Covers: extern fn, extern impl, and all error cases.
//! Note: `extern type` has been removed — types are always normal structs.

use formalang::{parse_only, CompilerError};

// =============================================================================
// extern fn (module-level)
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_extern_fn_no_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: Number, height: Number }
extern fn create_canvas() -> Canvas
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_fn_with_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: Number, height: Number }
extern fn render(canvas: Canvas, width: Number, height: Number) -> Boolean
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_fn_no_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: Number, height: Number }
extern fn flush(canvas: Canvas)
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// extern impl
// =============================================================================

#[test]
fn test_extern_impl_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: Number, height: Number }
extern impl Canvas {
    fn draw(self, x: Number, y: Number)
    fn get_width(self) -> Number
    fn get_height(self) -> Number
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_extern_impl_with_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Handle { raw: Number }
extern impl Handle {
    fn id(self) -> Number
    fn is_valid(self) -> Boolean
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_regular_impl_and_extern_impl_coexist() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Socket { host: String }
extern impl Socket {
    fn send(self, data: String) -> Boolean
    fn close(self)
}
impl Socket {
    fn send_text(self, text: String) -> Boolean {
        self.send(data: text)
    }
}
";
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error: extern fn with a body
// =============================================================================

#[test]
fn test_extern_fn_with_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
extern fn compute() -> Number {
    42
}
";
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
    let source = r"
fn compute() -> Number
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: non-extern fn must have a body".into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_is_extern_flag_threads_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #28: extern_fn_parser sets `is_extern: true` on the AST
    // FunctionDef, and the IR lowerer trusts that flag rather than
    // re-deriving from `body.is_none()`.
    let module = formalang::compile_to_ir("pub extern fn fetch(url: String) -> String")
        .map_err(|e| format!("{e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "fetch")
        .ok_or("fetch missing")?;
    if !f.is_extern {
        return Err("expected fetch to be marked is_extern in IR".into());
    }
    if f.body.is_some() {
        return Err("expected extern fn to have no body in IR".into());
    }
    Ok(())
}

#[test]
fn test_regular_fn_is_extern_flag_threads_to_ir() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #28: function_def_parser sets `is_extern: false`; verify
    // the IR mirrors that for ordinary fn definitions.
    let module = formalang::compile_to_ir("pub fn add(a: Number, b: Number) -> Number { a + b }")
        .map_err(|e| format!("{e:?}"))?;
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "add")
        .ok_or("add missing")?;
    if f.is_extern {
        return Err("expected regular fn to have is_extern=false in IR".into());
    }
    if f.body.is_none() {
        return Err("expected regular fn to have a body in IR".into());
    }
    Ok(())
}

#[test]
fn test_impl_fn_without_body_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Foo {}
impl Foo {
    fn bar(self) -> Number
}
";
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
    let source = r"
struct Canvas { width: Number, height: Number }
extern impl Canvas {
    fn draw(self) {
        42
    }
}
";
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
// extern type keyword is now a parse error
// =============================================================================

#[test]
fn test_extern_type_keyword_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
extern type Foo
";
    let result = parse_only(source);
    if result.is_ok() {
        return Err("expected parse error: `extern type` is no longer a valid production".into());
    }
    Ok(())
}

// =============================================================================
// Struct with extern impl compiles correctly
// =============================================================================

#[test]
fn test_struct_with_extern_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: Number, height: Number }
extern impl Canvas { fn draw(self, x: Number, y: Number) }
extern fn create_canvas(width: Number, height: Number) -> Canvas
";
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

    let source = r"
extern fn compute() -> Number
";
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

    let source = r"
fn compute() -> Number {
    42
}
";
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
