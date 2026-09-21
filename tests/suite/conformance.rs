//! The conformance corpus.
//!
//! `tests/conformance/` holds small `FormaLang` programs, one rule to
//! a file. Each states what should happen in a header line, and this
//! runner makes it happen:
//!
//! ```text
//! // expect: run
//! ```
//!
//! The program must compile, and its `run_checks()` must complete with
//! every `assert` holding. This is the shape that tests what a program
//! *means*, which no amount of inspecting the IR can.
//!
//! ```text
//! // expect: reject TypeMismatch
//! ```
//!
//! The program must fail to compile, reporting that `CompilerError`
//! variant. `reject` with no variant accepts any error.
//!
//! ```text
//! // expect: compile
//! ```
//!
//! The program must compile. Use it where there is nothing to run —
//! a declaration-only file, say.
//!
//! Adding a case is adding a file. That is the point: the corpus is
//! meant to grow to thousands of cases, and it should never need a
//! line of Rust to do so.
//!
//! The corpus is organised the way other languages organise theirs —
//! one directory per feature, one file per rule. The taxonomy borrows
//! from `TypeScript`'s `conformance` tree, Swift's optionals and
//! named-argument tests, and Rust's ownership tests; the cases are
//! written against `docs/user/`, which is this language's reference.

#![expect(
    clippy::panic,
    clippy::option_if_let_else,
    clippy::if_not_else,
    reason = "a conformance case reports its own failure by failing loudly; the \
              match over expectation and outcome reads better in one place"
)]

use crate::common::interpreter::{Fault, Interpreter};
use crate::common::Checked;

use std::path::{Path, PathBuf};

/// What a case file says should happen.
#[derive(Debug, PartialEq, Eq)]
enum Expectation {
    /// Compile and run `run_checks()`.
    Run,
    /// Compile, and stop there.
    Compile,
    /// Fail to compile. `Some(variant)` names the error that must
    /// appear among the reported ones.
    Reject(Option<String>),
}

/// One case: where it came from, what it says, and its source.
struct Case {
    name: String,
    expectation: Expectation,
    source: String,
}

/// Read the `// expect:` header. A file without one is an error, not a
/// silent skip.
fn parse_expectation(name: &str, source: &str) -> Expectation {
    let Some(line) = source
        .lines()
        .find(|l| l.trim_start().starts_with("// expect:"))
    else {
        panic!(
            "{name}: no `// expect:` header. Every conformance case must say \
             what should happen."
        );
    };
    let rest = line.trim_start().trim_start_matches("// expect:").trim();
    let mut parts = rest.split_whitespace();
    match parts.next() {
        Some("run") => Expectation::Run,
        Some("compile") => Expectation::Compile,
        Some("reject") => Expectation::Reject(parts.next().map(str::to_string)),
        other => panic!(
            "{name}: unknown expectation {other:?}; use `run`, `compile` or \
             `reject [Variant]`"
        ),
    }
}

/// Load every `.fv` under `tests/conformance/`, recursively.
fn cases() -> Vec<Case> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("conformance");
    let mut out = Vec::new();
    collect(&root, &root, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(
        out.len() >= 100,
        "the conformance corpus holds {} cases, expected at least 100 — check \
         tests/conformance/",
        out.len()
    );
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Case>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("fv") {
            continue;
        }
        let name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let expectation = parse_expectation(&name, &source);
        out.push(Case {
            name,
            expectation,
            source,
        });
    }
}

/// The name of an error's variant, from its `Debug` form.
fn variant_name(error: &formalang::CompilerError) -> String {
    format!("{error:?}")
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Every case does what its header says.
#[test]
fn every_case_meets_its_expectation() {
    let all = cases();
    let mut checked = Checked::new("conformance cases", 100);
    let mut failures = Vec::new();
    let mut executed_asserts = 0;

    for case in &all {
        let compiled = formalang::compile_to_ir(&case.source);

        match (&case.expectation, compiled) {
            (Expectation::Reject(want), Ok(_)) => {
                failures.push(match want {
                    Some(v) => format!("{}: compiled, but should report {v}", case.name),
                    None => format!("{}: compiled, but should be rejected", case.name),
                });
            }
            (Expectation::Reject(want), Err(errors)) => {
                let found: Vec<String> = errors.iter().map(variant_name).collect();
                // An internal error is never a correct answer to user
                // input. It tells the user that the compiler broke and
                // asks them to file a bug, for a mistake in their own
                // program. Every rejection must name what is wrong.
                if found.iter().any(|f| f == "InternalError") {
                    failures.push(format!(
                        "{}: rejected with an internal compiler error: {errors:?}",
                        case.name
                    ));
                } else if let Some(want) = want {
                    if !found.iter().any(|f| f == want) {
                        failures.push(format!("{}: expected {want}, got {found:?}", case.name));
                    }
                }
            }
            (Expectation::Run | Expectation::Compile, Err(errors)) => {
                failures.push(format!("{}: did not compile: {errors:?}", case.name));
            }
            (Expectation::Compile, Ok(_)) => {}
            (Expectation::Run, Ok(module)) => {
                let mut interpreter = Interpreter::new(&module);
                if !interpreter.has_function("run_checks") {
                    failures.push(format!(
                        "{}: expects `run` but declares no run_checks()",
                        case.name
                    ));
                } else {
                    match interpreter.run("run_checks") {
                        Ok(_) if interpreter.asserts_passed == 0 => {
                            failures.push(format!("{}: run_checks() asserted nothing", case.name));
                        }
                        Ok(_) => executed_asserts += interpreter.asserts_passed,
                        Err(Fault::AssertFailed) => failures.push(format!(
                            "{}: an assert failed after {} passed",
                            case.name, interpreter.asserts_passed
                        )),
                        Err(fault) => failures.push(format!(
                            "{}: {fault} (after {} assert(s))",
                            case.name, interpreter.asserts_passed
                        )),
                    }
                }
            }
        }
        checked.hit();
    }

    assert!(
        failures.is_empty(),
        "{} of {} conformance case(s) failed:\n\n{}",
        failures.len(),
        all.len(),
        failures.join("\n")
    );

    let runnable = all
        .iter()
        .filter(|c| c.expectation == Expectation::Run)
        .count();
    assert!(
        executed_asserts >= runnable,
        "only {executed_asserts} assertion(s) ran across {runnable} runnable \
         case(s); every one should assert at least once"
    );
}

/// The corpus covers both halves: programs that must work, and
/// programs that must be refused.
///
/// A suite of nothing but accepting cases says nothing about what the
/// compiler rejects, which is where the type checker's holes were.
#[test]
fn the_corpus_covers_acceptance_and_rejection() {
    let all = cases();
    let rejects = all
        .iter()
        .filter(|c| matches!(c.expectation, Expectation::Reject(_)))
        .count();
    let runs = all
        .iter()
        .filter(|c| c.expectation == Expectation::Run)
        .count();

    assert!(
        rejects >= 40,
        "only {rejects} rejection case(s); the corpus needs a real negative half"
    );
    assert!(
        runs >= 40,
        "only {runs} runnable case(s); the corpus needs a real positive half"
    );
}

/// Every rejection case names the variant it expects.
///
/// A bare `reject` passes on any error, including one from a typo in
/// the fixture, so it is a weaker test than it looks.
#[test]
fn most_rejection_cases_name_their_variant() {
    let all = cases();
    let unnamed: Vec<&str> = all
        .iter()
        .filter(|c| matches!(c.expectation, Expectation::Reject(None)))
        .map(|c| c.name.as_str())
        .collect();

    assert!(
        unnamed.len() <= 5,
        "{} rejection case(s) do not name the error they expect: {unnamed:?}",
        unnamed.len()
    );
}
