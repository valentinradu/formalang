//! Deep input must give a result. It must not stop the process.
//!
//! The compiler returns a `Result` for every input. A stack overflow
//! does not return: it aborts the whole process. For a build tool that
//! is one failed build. For an editor or a language server that holds
//! the compiler in its own process, it is a crash of the host.
//!
//! Two phases have a limit on depth. The parser computes a nesting
//! score from the tokens and refuses a score above 1024 with a
//! `ParseError` (`src/parser/nesting.rs`). The semantic pass refuses an
//! expression deeper than 500 with "Expression nesting exceeded the
//! compiler recursion limit". So a program below the limits must
//! compile, and a program above them must come back as an error.
//! Neither may abort.
//!
//! A stack overflow kills the test binary too, so each probe runs in a
//! child process: this same binary, started again with an environment
//! variable that names the source file. The child runs every entry
//! point on a thread with a stack of eight megabytes, which is the size of the main
//! thread on Linux. The parent then reads how the child stopped.
//!
//! There are two tests per construct. `..._below_the_limit` uses depths
//! of 64 and 256, which the semantic limit accepts. The
//! `..._above_the_limit` test uses depths of 1024, 10 000 and 100 000.

#![expect(
    clippy::panic,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::integer_division,
    clippy::format_push_string,
    reason = "a probe that cannot start is a broken harness, and a probe \
              that aborts must fail the test loudly; the child talks to its \
              parent over stdout; the generators build small sources with \
              small numbers"
)]

use formalang::ir::ConstantFoldingPass;
use formalang::semantic::node_finder::find_node_at_offset;
use formalang::semantic::queries::QueryProvider;
use formalang::{compile_to_ir, compile_with_analyzer, parse_only, report_errors, Pipeline};
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The environment variable that puts the binary in child mode. It
/// holds the path of the source file to probe.
const VAR: &str = "FORMALANG_DEPTH_LADDER_SOURCE";

/// The line that the child prints when every entry point returned.
const DONE: &str = "DEPTH-LADDER-PROBE-RETURNED";

/// The stack of the probe thread: the size of a main thread on Linux.
const STACK: usize = 8 * 1024 * 1024;

/// How long one probe may run. A probe that runs longer is a hang, and
/// a hang is a defect too.
const TIMEOUT: Duration = Duration::from_secs(120);

/// Depths that the semantic limit (500) accepts.
const BELOW: &[usize] = &[64, 256];

/// Depths that the parser limit or the semantic limit rejects.
const ABOVE: &[usize] = &[1024, 10_000, 100_000];

/// Run every entry point over one source. Each call must return.
fn probe_every_entry_point(source: &str) -> usize {
    let mut returned = 0;

    let _ = parse_only(source);
    returned += 1;

    if let Ok((file, analyzer)) = compile_with_analyzer(source) {
        let provider = QueryProvider::new(analyzer.symbols());
        let _ = provider.get_all_completions();
        let _ = provider.get_hover_for_symbol("x");
        for offset in [0, source.len() / 2, source.len()] {
            let _ = find_node_at_offset(&file, offset);
        }
    }
    returned += 1;

    match compile_to_ir(source) {
        Ok(module) => {
            let _ = serde_json::to_string(&module);
            let _ = Pipeline::new()
                .pass(ConstantFoldingPass::new())
                .run(module.clone());
            let _ = Pipeline::for_codegen().run(module);
        }
        Err(errors) => {
            let _ = report_errors(&errors, source, "probe.fv");
        }
    }
    returned += 1;

    returned
}

/// The child side. With the variable set, probe the named file and
/// print the marker. Without it, probe a small program, so that the
/// test still checks that the probe returns.
#[test]
fn child_probe() {
    let source = std::env::var(VAR).map_or_else(
        |_| "pub let x: I32 = 1\n".to_string(),
        |path| std::fs::read_to_string(&path).expect("the probe source must be readable"),
    );
    let returned = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || probe_every_entry_point(&source))
        .expect("the probe thread must start")
        .join()
        .expect("the probe thread must not panic");
    assert_eq!(returned, 3, "every entry point must return");
    println!("{DONE}");
}

/// How one child stopped.
enum Outcome {
    Returned,
    Aborted(String),
    Hung,
}

/// Start the child over `source` and wait for it.
fn run_child(source: &str) -> Outcome {
    let exe = std::env::current_exe().expect("the test binary must have a path");
    let mut file = tempfile::NamedTempFile::new().expect("a temporary file must open");
    file.write_all(source.as_bytes())
        .expect("the probe source must write");

    let mut child = Command::new(exe)
        .env(VAR, file.path())
        .arg(crate::common::test_name(module_path!(), "child_probe"))
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the child must start");

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Hung;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("cannot wait for the child: {e}"),
        }
    }
    let output = child
        .wait_with_output()
        .expect("the child output must read");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains(DONE) {
        return Outcome::Returned;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
    Outcome::Aborted(format!("status {:?}: {last}", output.status))
}

/// Probe one construct at each depth, and fail with every depth that
/// did not return.
fn ladder(name: &str, depths: &[usize], generate: fn(usize) -> String) {
    let mut failures = Vec::new();
    for &depth in depths {
        match run_child(&generate(depth)) {
            Outcome::Returned => {}
            Outcome::Aborted(why) => failures.push(format!("depth {depth}: aborted ({why})")),
            Outcome::Hung => failures.push(format!(
                "depth {depth}: still running after {}s",
                TIMEOUT.as_secs()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{name}: the compiler did not return a result:\n  {}",
        failures.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// The generators. Each one returns a whole program nested `n` deep.
// ---------------------------------------------------------------------------

fn parens(n: usize) -> String {
    format!("pub let x: I32 = {}1{}\n", "(".repeat(n), ")".repeat(n))
}

fn chain(op: &str, atom: &str, n: usize) -> String {
    let mut s = String::from("pub let x = ");
    for _ in 0..n {
        s.push_str(atom);
        s.push(' ');
        s.push_str(op);
        s.push(' ');
    }
    s.push_str(atom);
    s.push('\n');
    s
}

fn add_chain(n: usize) -> String {
    chain("+", "1", n)
}
fn sub_chain(n: usize) -> String {
    chain("-", "1", n)
}
fn mul_chain(n: usize) -> String {
    chain("*", "1", n)
}
fn div_chain(n: usize) -> String {
    chain("/", "1", n)
}
fn rem_chain(n: usize) -> String {
    chain("%", "1", n)
}
fn and_chain(n: usize) -> String {
    chain("&&", "true", n)
}
fn or_chain(n: usize) -> String {
    chain("||", "false", n)
}
fn eq_chain(n: usize) -> String {
    chain("==", "true", n)
}
fn string_concat_chain(n: usize) -> String {
    chain("+", "\"a\"", n)
}

fn negations(n: usize) -> String {
    format!("pub let x: I32 = {}1\n", "-".repeat(n))
}

fn nots(n: usize) -> String {
    format!("pub let x: Boolean = {}true\n", "!".repeat(n))
}

fn arrays(n: usize) -> String {
    format!("pub let x = {}1{}\n", "[".repeat(n), "]".repeat(n))
}

fn dictionaries(n: usize) -> String {
    format!("pub let x = {}1{}\n", "[\"k\": ".repeat(n), "]".repeat(n))
}

fn tuples(n: usize) -> String {
    format!("pub let x = {}1{}\n", "(a: ".repeat(n), ")".repeat(n))
}

fn if_in_condition(n: usize) -> String {
    let condition = format!(
        "{}true{}",
        "if ".repeat(n),
        " { true } else { false }".repeat(n)
    );
    format!("pub let x: I32 = if {condition} {{ 1 }} else {{ 2 }}\n")
}

fn if_in_branch(n: usize) -> String {
    format!(
        "pub let x: I32 = {}1{}\n",
        "if true { ".repeat(n),
        " } else { 2 }".repeat(n)
    )
}

fn else_if_chain(n: usize) -> String {
    let mut s = String::from("pub fn f(a: I32) -> I32 {\n    if a == 0 { 0 }");
    for i in 1..=n {
        s.push_str(&format!(" else if a == {i} {{ {i} }}"));
    }
    s.push_str(" else { -1 }\n}\n");
    s
}

fn matches(n: usize) -> String {
    format!(
        "pub enum E {{ a, b }}\npub fn f(e: E) -> I32 {{\n    {}1{}\n}}\n",
        "match e { .a: ".repeat(n),
        ", .b: 2 }".repeat(n)
    )
}

fn blocks(n: usize) -> String {
    format!(
        "pub fn f() -> I32 {{\n    let x = {}1{}\n    x\n}}\n",
        "{ ".repeat(n),
        " }".repeat(n)
    )
}

fn closures(n: usize) -> String {
    format!("pub let x = {}1\n", "() -> ".repeat(n))
}

fn nested_calls(n: usize) -> String {
    format!(
        "pub fn f(v: I32) -> I32 {{ v }}\npub let x: I32 = {}1{}\n",
        "f(v: ".repeat(n),
        ")".repeat(n)
    )
}

fn field_chain(n: usize) -> String {
    format!("pub let t = (a: 1)\npub let x = t{}\n", ".a".repeat(n))
}

fn method_chain(n: usize) -> String {
    format!(
        "pub struct S {{ v: I32 }}\nimpl S {{\n    fn m(self) -> S {{ self }}\n}}\n\
         pub let x = S(v: 1){}\n",
        ".m()".repeat(n)
    )
}

fn array_type_nesting(n: usize) -> String {
    format!("pub let x: {}I32{} = []\n", "[".repeat(n), "]".repeat(n))
}

fn optional_type_nesting(n: usize) -> String {
    format!("pub let x: I32{} = nil\n", "?".repeat(n))
}

fn generic_type_nesting(n: usize) -> String {
    format!(
        "pub struct Box<T> {{ v: T }}\npub fn f(b: {}I32{}) -> I32 {{ 1 }}\n",
        "Box<".repeat(n),
        ">".repeat(n)
    )
}

fn function_type_nesting(n: usize) -> String {
    format!("pub fn f(g: {}I32) -> I32 {{ 1 }}\n", "(I32) -> ".repeat(n))
}

fn nested_modules(n: usize) -> String {
    format!(
        "{}pub let x: I32 = 1\n{}",
        "pub mod m {\n".repeat(n),
        "}\n".repeat(n)
    )
}

fn nested_for_loops(n: usize) -> String {
    format!(
        "pub fn f() -> I32 {{\n    let x = {}1{}\n    1\n}}\n",
        "for i in 0..1 { ".repeat(n),
        " }".repeat(n)
    )
}

fn nested_block_comments(n: usize) -> String {
    format!(
        "{}{}\npub let x: I32 = 1\n",
        "/* ".repeat(n),
        " */".repeat(n)
    )
}

fn sequential_lets(n: usize) -> String {
    let mut s = String::from("pub fn f() -> I32 {\n    let v0 = 0\n");
    for i in 1..=n {
        s.push_str(&format!("    let v{i} = v{} + 1\n", i - 1));
    }
    s.push_str(&format!("    v{n}\n}}\n"));
    s
}

fn match_arm_count(n: usize) -> String {
    let mut s = String::from("pub fn f(a: I32) -> I32 {\n    match a {\n");
    for i in 0..n {
        s.push_str(&format!("        {i}: {i},\n"));
    }
    s.push_str("        _: -1\n    }\n}\n");
    s
}

fn long_string(n: usize) -> String {
    format!("pub let s: String = \"{}\"\n", "x".repeat(n))
}

fn unclosed_ifs(n: usize) -> String {
    format!(
        "pub fn answer() -> I32 {{\n{}",
        "let d = if v { let v = v * 2".repeat(n)
    )
}

/// A long string literal is not deep, but the compiler must still
/// return for it.
#[test]
fn a_long_string_literal_returns() {
    ladder("long_string", &[1 << 16, 1 << 20, 10 << 20], long_string);
}

/// A nest of `if` blocks that are not closed (`FOUND_DEFECTS.md` item
/// 5). Twelve of them is 400 bytes of source.
#[test]
fn unclosed_ifs_return() {
    ladder("unclosed_ifs", &[2, 4, 8, 12], unclosed_ifs);
}

/// Two tests per construct: one below the semantic limit and one
/// above it.
macro_rules! ladder_tests {
    ($($below:ident, $above:ident => $generate:ident;)*) => {
        $(
            #[test]
            fn $below() {
                ladder(stringify!($generate), BELOW, $generate);
            }

            #[test]
            fn $above() {
                ladder(stringify!($generate), ABOVE, $generate);
            }
        )*
    };
}

ladder_tests! {
    parens_below_the_limit, parens_above_the_limit => parens;
    add_chain_below_the_limit, add_chain_above_the_limit => add_chain;
    sub_chain_below_the_limit, sub_chain_above_the_limit => sub_chain;
    mul_chain_below_the_limit, mul_chain_above_the_limit => mul_chain;
    div_chain_below_the_limit, div_chain_above_the_limit => div_chain;
    rem_chain_below_the_limit, rem_chain_above_the_limit => rem_chain;
    and_chain_below_the_limit, and_chain_above_the_limit => and_chain;
    or_chain_below_the_limit, or_chain_above_the_limit => or_chain;
    eq_chain_below_the_limit, eq_chain_above_the_limit => eq_chain;
    string_concat_below_the_limit, string_concat_above_the_limit => string_concat_chain;
    negations_below_the_limit, negations_above_the_limit => negations;
    nots_below_the_limit, nots_above_the_limit => nots;
    arrays_below_the_limit, arrays_above_the_limit => arrays;
    dictionaries_below_the_limit, dictionaries_above_the_limit => dictionaries;
    tuples_below_the_limit, tuples_above_the_limit => tuples;
    if_in_condition_below_the_limit, if_in_condition_above_the_limit => if_in_condition;
    if_in_branch_below_the_limit, if_in_branch_above_the_limit => if_in_branch;
    else_if_chain_below_the_limit, else_if_chain_above_the_limit => else_if_chain;
    matches_below_the_limit, matches_above_the_limit => matches;
    blocks_below_the_limit, blocks_above_the_limit => blocks;
    closures_below_the_limit, closures_above_the_limit => closures;
    nested_calls_below_the_limit, nested_calls_above_the_limit => nested_calls;
    field_chain_below_the_limit, field_chain_above_the_limit => field_chain;
    method_chain_below_the_limit, method_chain_above_the_limit => method_chain;
    array_type_below_the_limit, array_type_above_the_limit => array_type_nesting;
    optional_type_below_the_limit, optional_type_above_the_limit => optional_type_nesting;
    generic_type_below_the_limit, generic_type_above_the_limit => generic_type_nesting;
    function_type_below_the_limit, function_type_above_the_limit => function_type_nesting;
    nested_modules_below_the_limit, nested_modules_above_the_limit => nested_modules;
    nested_for_loops_below_the_limit, nested_for_loops_above_the_limit => nested_for_loops;
    block_comments_below_the_limit, block_comments_above_the_limit => nested_block_comments;
    sequential_lets_below_the_limit, sequential_lets_above_the_limit => sequential_lets;
    match_arm_count_below_the_limit, match_arm_count_above_the_limit => match_arm_count;
}

/// The `compile` fuzz target wrote a 1616-byte slow unit. It reads the
/// longest UTF-8 prefix of it, which is this input:
/// broken `if` blocks nested about sixty deep. `scripts/fuzz.sh` calls
/// an input a defect when it takes more than ten seconds, so the probe
/// must return in that time.
#[test]
fn the_slow_fuzz_input_returns_in_ten_seconds() {
    let source = include_str!("../../fuzz/seeds/text/slow_broken_nested_ifs.fv");
    let start = Instant::now();
    let outcome = run_child(source);
    let took = start.elapsed();
    match outcome {
        Outcome::Returned => assert!(
            took < Duration::from_secs(10),
            "the slow fuzz input took {took:?} to refuse"
        ),
        Outcome::Aborted(why) => panic!("the slow fuzz input aborted: {why}"),
        Outcome::Hung => panic!(
            "the slow fuzz input was still running after {}s",
            TIMEOUT.as_secs()
        ),
    }
}
