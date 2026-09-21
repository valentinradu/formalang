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
fn connect(host: String, port: I32) -> Boolean {
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
fn process(number: I32) -> I32 {
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
    // A method overloads by the shape of the call, and the call
    // reaches the body it names. This test used to assert only that
    // the declaration compiles — which it did, while every call went
    // to the first method — so it now calls both and checks the
    // answers.
    let source = r#"
pub struct Formatter {
    tag: I32
}

impl Formatter {
    fn format(self, text: String) -> String {
        text
    }
    fn format(self, value: I32, prefix: String) -> String {
        prefix
    }
}

pub fn run_checks() {
    assert(condition: Formatter(tag: 1).format(text: "a") == "a")
    assert(condition: Formatter(tag: 1).format(value: 2, prefix: "p") == "p")
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
    // the same first-positional-type (I32): a positional call matches
    // both under Mode B.
    let source = r"
fn run(a: I32) -> I32 {
    a
}
fn run(b: I32) -> I32 {
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
fn process(number: I32) -> I32 {
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
fn process(number: I32) -> I32 {
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

// =============================================================================
// Overload resolution reaches the IR
// =============================================================================

/// Each call to an overloaded name lowers to the overload it selects.
///
/// `IrModule.function_names` maps a name to a single id, so a later
/// overload overwrote an earlier one and every call to an overloaded
/// name carried whichever id was registered last. The analyser picked
/// the right overload, so the program compiled — and a backend then
/// emitted a call to the wrong function. It surfaced when the example
/// programs were first executed: `run_overload_two(x: 7, p: 3)`
/// returned 7 instead of 21.
#[test]
fn each_call_lowers_to_the_overload_it_selects() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
fn format(value: I32) -> I32 {
    value
}

fn format(value: I32, precision: I32) -> I32 {
    value * precision
}

pub fn one(x: I32) -> I32 {
    format(value: x)
}

pub fn two(x: I32, p: I32) -> I32 {
    format(value: x, precision: p)
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let called_id = |caller: &str| -> Option<u32> {
        let f = module.functions.iter().find(|f| f.name == caller)?;
        let json = serde_json::to_value(f.body.as_ref()?).ok()?;
        json.get("FunctionCall")?
            .get("function_id")?
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
    };

    let one = called_id("one").ok_or("`one` should call a resolved function")?;
    let two = called_id("two").ok_or("`two` should call a resolved function")?;

    if one == two {
        return Err(format!(
            "both calls lowered to function id {one}; the one-argument and \
             two-argument overloads must resolve to different functions"
        )
        .into());
    }

    let arity_of = |id: u32| -> usize {
        module
            .functions
            .get(id as usize)
            .map_or(0, |f| f.params.len())
    };
    if arity_of(one) != 1 {
        return Err(format!(
            "`one` resolved to a {}-parameter overload, expected 1",
            arity_of(one)
        )
        .into());
    }
    if arity_of(two) != 2 {
        return Err(format!(
            "`two` resolved to a {}-parameter overload, expected 2",
            arity_of(two)
        )
        .into());
    }
    Ok(())
}
