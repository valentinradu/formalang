//! Run every example program.
//!
//! Each `examples/*.fv` ends in a `run_checks()` full of
//! `assert(condition: ...)` calls stating what the program should
//! compute — `area(s: Shape.circle(radius: 4)) == 48`. Ninety-three of
//! them, and until now nothing executed a single one: the compiler is
//! a frontend, so the test suite only ever checked that the examples
//! *compile*.
//!
//! This file runs them, against the reference interpreter in
//! `tests/common/interpreter.rs`. An assertion that fails means the
//! compiler produced an IR that computes the wrong answer — which no
//! amount of checking the IR's *shape* would catch.

#![expect(
    clippy::print_stdout,
    reason = "a failing example should fail the test loudly, and the count of \
              executed assertions is worth seeing"
)]

#[path = "common/mod.rs"]
mod common;

use common::interpreter::{Fault, Interpreter};
use common::Checked;

use std::path::PathBuf;

/// Every `examples/*.fv`, by name and source.
fn examples() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("fv") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if let Ok(source) = std::fs::read_to_string(&path) {
            out.push((name, source));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        out.len() >= 20,
        "the example corpus holds {} files, expected at least 20",
        out.len()
    );
    out
}

/// Every example's `run_checks()` runs, and every assertion in it
/// holds.
#[test]
fn every_example_passes_its_own_checks() {
    let mut checked = Checked::new("examples executed", 20);
    let mut failures = Vec::new();
    let mut total_asserts = 0;

    for (name, source) in examples() {
        let module = match formalang::compile_to_ir(&source) {
            Ok(m) => m,
            Err(errors) => {
                failures.push(format!("{name}: did not compile: {errors:?}"));
                continue;
            }
        };

        let mut interpreter = Interpreter::new(&module);
        if !interpreter.has_function("run_checks") {
            failures.push(format!("{name}: declares no run_checks()"));
            continue;
        }

        match interpreter.run("run_checks") {
            Ok(_) => {
                if interpreter.asserts_passed == 0 {
                    failures.push(format!("{name}: run_checks() asserted nothing"));
                } else {
                    total_asserts += interpreter.asserts_passed;
                    checked.hit();
                }
            }
            Err(Fault::AssertFailed) => {
                failures.push(format!(
                    "{name}: an assert failed after {} passed — the compiler \
                     produced an IR that computes the wrong answer",
                    interpreter.asserts_passed
                ));
            }
            Err(fault) => {
                failures.push(format!(
                    "{name}: {fault} (after {} assert(s) passed)",
                    interpreter.asserts_passed
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} example(s) failed to run:\n\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        total_asserts >= 90,
        "only {total_asserts} assertion(s) ran across the examples; the corpus \
         holds about 93"
    );
    println!(
        "{total_asserts} assertions executed across {} examples",
        checked.count()
    );
}
