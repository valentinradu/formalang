//! Inner doc comments, one statement per line at every level, and `_`
//! in a variant pattern. See `docs/user/core.md` and
//! `docs/user/control-flow.md`.

use formalang::ast::{Definition, Expr, Pattern, Statement};
use formalang::{parse_only, File};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn parse(source: &str) -> Result<File, String> {
    parse_only(source).map_err(|e| format!("{e:?}"))
}

/// Fail with `what` when `found` differs from `expected`.
fn expect_eq<T: PartialEq + std::fmt::Debug>(what: &str, found: &T, expected: &T) -> TestResult {
    if found == expected {
        Ok(())
    } else {
        Err(format!("{what}: expected {expected:?}, found {found:?}").into())
    }
}

fn definitions(file: &File) -> impl Iterator<Item = &Definition> {
    file.statements.iter().filter_map(|statement| {
        if let Statement::Definition(def) = statement {
            Some(&**def)
        } else {
            None
        }
    })
}

fn first_module_doc(file: &File) -> Option<String> {
    definitions(file).find_map(|def| {
        if let Definition::Module(m) = def {
            m.doc.clone()
        } else {
            None
        }
    })
}

#[test]
fn an_inner_doc_comment_documents_the_file() -> TestResult {
    let file = parse("//! First line.\n//! Second line.\n\npub let a: I32 = 1\n")?;
    expect_eq(
        "the file doc",
        &file.doc.as_deref(),
        &Some("First line.\nSecond line."),
    )?;
    expect_eq("the statement count", &file.statements.len(), &1)
}

#[test]
fn a_file_without_an_inner_doc_comment_has_no_doc() -> TestResult {
    let file = parse("/// Item doc.\npub let a: I32 = 1\n")?;
    expect_eq("the file doc", &file.doc, &None)
}

#[test]
fn an_inner_doc_comment_documents_the_module() -> TestResult {
    let file = parse("pub mod m {\n    //! Inner.\n    pub struct A { x: I32 }\n}\n")?;
    expect_eq(
        "the module doc",
        &first_module_doc(&file).as_deref(),
        &Some("Inner."),
    )
}

#[test]
fn outer_and_inner_module_docs_join() -> TestResult {
    let file = parse("/// Outer.\npub mod m {\n    //! Inner.\n    pub struct A { x: I32 }\n}\n")?;
    expect_eq(
        "the module doc",
        &first_module_doc(&file).as_deref(),
        &Some("Outer.\nInner."),
    )
}

#[test]
fn an_inner_doc_comment_after_an_item_is_refused() {
    assert!(parse("pub let a: I32 = 1\n//! Late.\npub let b: I32 = 2\n").is_err());
    assert!(parse("pub fn f() -> I32 {\n    //! Late.\n    1\n}\n").is_err());
}

#[test]
fn two_definitions_on_one_line_are_refused() {
    for source in [
        "pub let a: I32 = 1 pub let b: I32 = 2\n",
        "pub struct A { x: I32 } pub struct B { y: I32 }\n",
        "pub mod m {\n    pub struct A { x: I32 } pub struct B { y: I32 }\n}\n",
        "pub struct A { x: I32 }\nimpl A {\n    fn one(self) -> I32 { 1 } fn two(self) -> I32 { 2 }\n}\n",
    ] {
        assert!(parse(source).is_err(), "parsed: {source}");
    }
}

#[test]
fn a_definition_that_ends_in_a_generic_type_ends_at_the_line_break() -> TestResult {
    let file = parse("extern fn make() -> Box<I32>\npub let a: I32 = 1\n")?;
    expect_eq("the statement count", &file.statements.len(), &2)
}

#[test]
fn a_glob_import_ends_at_the_line_break() -> TestResult {
    let file = parse("use m::*\npub let a: I32 = 1\n")?;
    expect_eq("the statement count", &file.statements.len(), &2)
}

#[test]
fn a_trailing_operator_still_continues_the_line() -> TestResult {
    let file = parse("pub let a: Boolean = 2 >\n    1\npub let b: I32 = 2 *\n    3\n")?;
    expect_eq("the statement count", &file.statements.len(), &2)
}

#[test]
fn an_underscore_in_a_variant_pattern_parses() -> TestResult {
    let file = parse(
        "pub enum M { image(url: String, size: I32) }\n\
         pub fn f(m: M) -> I32 {\n    match m {\n        .image(_, s): s\n    }\n}\n",
    )?;
    let body = definitions(&file).find_map(|def| {
        if let Definition::Function(f) = def {
            f.body.clone()
        } else {
            None
        }
    });
    let Some(Expr::MatchExpr { arms, .. }) = body else {
        return Err(format!("expected a match body, got {body:?}").into());
    };
    let Some(Pattern::Variant { bindings, .. }) = arms.first().map(|arm| &arm.pattern) else {
        return Err("expected a variant pattern".into());
    };
    let names: Vec<&str> = bindings.iter().map(|b| b.name.as_str()).collect();
    expect_eq("the bindings", &names, &vec!["_", "s"])
}
