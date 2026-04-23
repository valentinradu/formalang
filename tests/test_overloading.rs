//! Tests for function overloading (#3)
//!
//! Same name, different signatures. Resolved by named-argument label set (Mode A)
//! or first-positional-argument type (Mode B).

use formalang::CompilerError;

// =============================================================================
// Happy path: overloading by named-argument label set (Mode A)
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn test_overload_by_label_set() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn greet(en name: String) -> String {
    name
}
fn greet(es name: String) -> String {
    name
}
let a = greet(en: "Alice")
let b = greet(es: "Alicia")
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_overload_by_label_set_multiple_params() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn connect(host: String, port: Number) -> Boolean {
    true
}
fn connect(path: String) -> Boolean {
    true
}
let a = connect(host: "localhost", port: 8080)
let b = connect(path: "/tmp/sock")
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Happy path: overloading by first-positional-argument type (Mode B)
// =============================================================================

#[test]
fn test_overload_by_first_arg_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn process(text: String) -> String {
    text
}
fn process(number: Number) -> Number {
    number
}
let a = process("hello")
let b = process(42)
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn test_overload_by_first_arg_type_bool() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn stringify(value: String) -> String {
    value
}
fn stringify(value: Boolean) -> String {
    "bool"
}
let a = stringify("hello")
let b = stringify(true)
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Happy path: overloads in impl blocks
// =============================================================================

#[test]
fn test_overload_in_impl_block() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
struct Formatter {}
impl Formatter {
    fn format(self, text: String) -> String {
        text
    }
    fn format(self, value: Number) -> String {
        "number"
    }
}
"#;
    compile(source).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// =============================================================================
// Error: ambiguous call (multiple overloads match)
// =============================================================================

#[test]
fn test_ambiguous_call_error() -> Result<(), Box<dyn std::error::Error>> {
    // Two overloads that differ only by parameter name (a vs b) but share
    // the same first-positional-type (Number): a positional call matches
    // both under Mode B.
    let source = r"
fn run(a: Number) -> Number {
    a
}
fn run(b: Number) -> Number {
    b + 1
}
let r = run(42)
";
    let errors = compile(source)
        .err()
        .ok_or("expected error: ambiguous overload call")?;
    let has_error = errors
        .iter()
        .any(|e| matches!(e, CompilerError::AmbiguousCall { function, .. } if function == "run"));
    if !has_error {
        return Err(format!("expected AmbiguousCall for 'run', got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Error: no matching overload
// =============================================================================

#[test]
fn test_no_matching_overload_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn process(text: String) -> String {
    text
}
fn process(number: Number) -> Number {
    number
}
let r = process(true)
";
    let errors = compile(source)
        .err()
        .ok_or("expected error: no matching overload")?;
    let has_error = errors.iter().any(|e| {
        matches!(e, CompilerError::NoMatchingOverload { function, .. } if function == "process")
    });
    if !has_error {
        return Err(format!("expected NoMatchingOverload for 'process', got: {errors:?}").into());
    }
    Ok(())
}

#[test]
fn test_no_matching_overload_by_labels_error() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
fn greet(en name: String) -> String {
    name
}
fn greet(es name: String) -> String {
    name
}
let r = greet(de: "Hallo")
"#;
    let errors = compile(source)
        .err()
        .ok_or("expected error: no matching overload by labels")?;
    let has_error = errors.iter().any(
        |e| matches!(e, CompilerError::NoMatchingOverload { function, .. } if function == "greet"),
    );
    if !has_error {
        return Err(format!("expected NoMatchingOverload for 'greet', got: {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// Each overload gets its own FunctionId (IR level check)
// =============================================================================

#[test]
fn test_overloads_get_distinct_function_ids() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::compile_to_ir;
    let source = r"
fn process(text: String) -> String {
    text
}
fn process(number: Number) -> Number {
    number
}
";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let process_fns: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.name == "process")
        .collect();
    if process_fns.len() != 2 {
        return Err(format!(
            "expected 2 distinct 'process' functions in IR, got {}",
            process_fns.len()
        )
        .into());
    }
    Ok(())
}
