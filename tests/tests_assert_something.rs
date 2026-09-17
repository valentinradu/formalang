//! A test that compiles a program and asks nothing of the result.
//!
//! `test_overload_in_impl_block` declared two methods of one name,
//! compiled them, and stopped. It passed for as long as the feature
//! was broken, because it never called either method — the compiler
//! could not perform method overloading at all, and the test covering
//! that feature said it worked.
//!
//! `test_if_without_else_branch` asserted that an `if` with no `else`
//! satisfied a non-optional type, contradicting the sentence in
//! `docs/user/control-flow.md` that says it returns nil.
//! `test_inferred_enum_in_let` asserted that a `.variant` with nothing
//! to resolve against was fine.
//!
//! Three tests that passed while testing nothing, or the wrong thing.
//! This file finds the shape mechanically.
//!
//! # What it can and cannot see
//!
//! It reads the test sources and asks whether each test function, after
//! compiling something, looks at the result: an assertion, an expected
//! error, or a run through the reference interpreter. That is a text
//! search, so it sees the shape and not the meaning. A test may assert
//! something worthless and still pass here.
//!
//! What it does guarantee is that the number of tests asserting
//! *nothing* cannot grow. A compile-only test is not worthless — on a
//! valid program it asserts "this is not falsely rejected", which is a
//! real property of a compiler, and the parser suites are built almost
//! entirely on it. But it cannot catch a wrong answer, and it cannot
//! tell a working feature from a missing one. So the existing ones are
//! recorded here, and a new one has to displace an old one or assert
//! something.
//!
//! The thorough version of this question is mutation testing:
//! perturb the compiler and see whether any test notices. That runs
//! separately — see `TESTING.md` — because it takes hours. This is the
//! cheap smoke alarm that runs every time.

#![expect(
    clippy::expect_used,
    reason = "a tests/ directory that cannot be read is a broken harness, not \
              a test failure"
)]

use std::collections::BTreeMap;
use std::path::Path;

/// How many compile-only tests each file is allowed.
///
/// A ratchet, not a target. Lower a number when a test starts
/// asserting something; never raise one.
const ALLOWED: &[(&str, usize)] = &[
    ("cross_module.rs", 1),
    ("default_parameters.rs", 1),
    ("destructuring.rs", 9),
    ("error_paths.rs", 21),
    ("integration.rs", 64),
    ("ir_spans.rs", 2),
    ("lexer_correctness.rs", 3),
    ("metamorphic.rs", 1),
    ("parser_edge_cases.rs", 70),
    ("semantic.rs", 36),
    ("semantic_analysis.rs", 46),
    ("semantic_edge_cases.rs", 26),
    ("semantic_validation.rs", 50),
    ("test_ast_and_token_helpers.rs", 85),
    ("test_dce_visitor.rs", 2),
    ("test_extern.rs", 7),
    ("test_gaps.rs", 17),
    ("test_impl_trait.rs", 5),
    ("test_no_ui.rs", 4),
    ("test_overloading.rs", 4),
    ("test_param_conventions.rs", 16),
    ("test_semantic_coverage.rs", 57),
    ("test_semantic_coverage2.rs", 41),
    ("test_semantic_coverage3.rs", 33),
    ("test_semantic_coverage4.rs", 6),
    ("test_trait_methods.rs", 8),
];

/// Text that means the test looked at what it compiled.
const LOOKS_AT_THE_RESULT: &[&str] = &[
    "assert",
    "is_err",
    "Err(",
    "expect_err",
    ".run(",
    "interpreter",
    "Interpreter",
];

/// The body of every `#[test]` function in `source`, by name.
fn test_bodies(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = source;

    while let Some(at) = rest.find("#[test]") {
        let after = rest.get(at..).unwrap_or_default();
        let Some(fn_at) = after.find("\nfn ") else {
            break;
        };
        let Some(open) = after.get(fn_at..).and_then(|s| s.find('{')) else {
            break;
        };
        let header = after.get(fn_at..fn_at.saturating_add(open)).unwrap_or("");
        let name = header
            .trim_start_matches("\nfn ")
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("")
            .to_string();

        let body_start = fn_at.saturating_add(open).saturating_add(1);
        let mut depth = 1_usize;
        let mut end = body_start;
        for (offset, c) in after.get(body_start..).unwrap_or("").char_indices() {
            match c {
                '{' => depth = depth.saturating_add(1),
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = body_start.saturating_add(offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push((name, after.get(body_start..end).unwrap_or("").to_string()));
        rest = after.get(end..).unwrap_or("");
    }

    out
}

/// Whether this body compiles something and never looks at the result.
fn compiles_without_looking(body: &str) -> bool {
    let compiles = body.contains("compile(")
        || body.contains("compile_to_ir(")
        || body.contains("compile_to_ir_with_resolver(");
    compiles && !LOOKS_AT_THE_RESULT.iter().any(|m| body.contains(m))
}

#[test]
fn no_file_grows_its_count_of_tests_that_assert_nothing() {
    let allowed: BTreeMap<&str, usize> = ALLOWED.iter().copied().collect();
    let mut counted: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let entries = std::fs::read_dir(Path::new("tests")).expect("tests/ must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // This file holds the search text itself, so scanning it finds
        // its own helpers rather than tests.
        if name == "tests_assert_something.rs" {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (test, body) in test_bodies(&source) {
            if compiles_without_looking(&body) {
                counted.entry(name.to_string()).or_default().push(test);
            }
        }
    }

    let mut grew = Vec::new();
    for (file, tests) in &counted {
        let budget = allowed.get(file.as_str()).copied().unwrap_or(0);
        if tests.len() > budget {
            grew.push(format!(
                "{file}: {} test(s) compile something and assert nothing, {budget} allowed. \
                 New: {:?}",
                tests.len(),
                tests.iter().rev().take(3).collect::<Vec<_>>()
            ));
        }
    }

    assert!(
        grew.is_empty(),
        "{} file(s) gained a test that compiles a program and asks nothing of \
         the result. Such a test passes whether or not the feature works — it \
         is how method overloading stayed 'covered' while being impossible. \
         Assert what the program does, or lower another entry in ALLOWED to \
         make room:\n  {}",
        grew.len(),
        grew.join("\n  ")
    );
}

/// The budget must not drift above what the tree actually holds.
///
/// Without this, a test that starts asserting something leaves its
/// budget behind, and the next compile-only test slips in under it.
#[test]
fn the_budget_is_not_stale() {
    let mut counted: BTreeMap<String, usize> = BTreeMap::new();
    let entries = std::fs::read_dir(Path::new("tests")).expect("tests/ must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == "tests_assert_something.rs" {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let n = test_bodies(&source)
            .iter()
            .filter(|(_, body)| compiles_without_looking(body))
            .count();
        if n > 0 {
            counted.insert(name.to_string(), n);
        }
    }

    let mut stale = Vec::new();
    for (file, budget) in ALLOWED {
        let actual = counted.get(*file).copied().unwrap_or(0);
        if actual < *budget {
            stale.push(format!(
                "{file}: allows {budget}, holds {actual} — lower it"
            ));
        }
    }

    assert!(
        stale.is_empty(),
        "{} budget entr(ies) are above what the tree holds. Lowering them keeps \
         the ratchet tight:\n  {}",
        stale.len(),
        stale.join("\n  ")
    );
}
