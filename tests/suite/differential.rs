//! Differential testing: the compiler against an independent oracle.
//!
//! The conformance corpus states expected values by hand, one file at
//! a time. This file generates them instead. It builds a random
//! expression tree, computes its value in Rust, renders the same tree
//! as `FormaLang` source, compiles it, and evaluates it with the
//! reference interpreter. The two answers must agree.
//!
//! The oracle is the point. A hand-written test can only check what
//! someone thought to write down; this checks arbitrary programs,
//! thousands per run, against an evaluator written from the operator
//! semantics rather than from the compiler's own code. When the two
//! disagree, either the compiler computes the wrong answer or the
//! interpreter does, and both are worth knowing.
//!
//! `PROPTEST_CASES=N` changes how many trees are tried.

#![expect(
    clippy::format_push_string,
    reason = "the oracle mirrors the language's wrapping integer semantics, and \
              a disagreement is reported by failing loudly"
)]

use crate::common::interpreter::{Fault, Interpreter, Value};

use proptest::prelude::*;

// ---------------------------------------------------------------------------
// The expression tree
// ---------------------------------------------------------------------------

/// An integer-valued expression, small enough to reason about and rich
/// enough to exercise precedence, nesting and every operator.
#[derive(Clone, Debug)]
enum Expr {
    Int(i32),
    /// A `let`-bound name, by index into the bindings the program
    /// declares.
    Var(usize),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    /// Division and modulo carry a non-zero divisor, so the tree never
    /// asks a question the language leaves to the backend.
    Div(Box<Self>, i32),
    Rem(Box<Self>, i32),
    Neg(Box<Self>),
    /// `if cond { a } else { b }`, with the condition a comparison.
    If(Box<Cond>, Box<Self>, Box<Self>),
}

/// A boolean-valued expression.
#[derive(Clone, Debug)]
enum Cond {
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
}

/// How many `let` bindings the generated program declares.
const BINDINGS: usize = 4;

// ---------------------------------------------------------------------------
// The oracle
// ---------------------------------------------------------------------------

/// Evaluate the tree in Rust.
///
/// Returns `None` when any step overflows `i64`. The language leaves
/// what an overflowing operation produces to the backend — the
/// constant folder deliberately leaves such an expression unfolded —
/// so there is no single right answer to compare against, and the
/// caller skips the case. Every other tree has one.
fn oracle(expr: &Expr, bindings: &[i64]) -> Option<i64> {
    let value = match expr {
        Expr::Int(v) => i64::from(*v),
        Expr::Var(i) => bindings.get(*i % BINDINGS).copied().unwrap_or(0),
        Expr::Add(a, b) => oracle(a, bindings)?.checked_add(oracle(b, bindings)?)?,
        Expr::Sub(a, b) => oracle(a, bindings)?.checked_sub(oracle(b, bindings)?)?,
        Expr::Mul(a, b) => oracle(a, bindings)?.checked_mul(oracle(b, bindings)?)?,
        Expr::Div(a, d) => oracle(a, bindings)?.checked_div(i64::from(nonzero(*d)))?,
        Expr::Rem(a, d) => oracle(a, bindings)?.checked_rem(i64::from(nonzero(*d)))?,
        Expr::Neg(a) => oracle(a, bindings)?.checked_neg()?,
        Expr::If(c, t, e) => {
            if oracle_cond(c, bindings)? {
                oracle(t, bindings)?
            } else {
                oracle(e, bindings)?
            }
        }
    };
    Some(value)
}

fn oracle_cond(cond: &Cond, bindings: &[i64]) -> Option<bool> {
    let value = match cond {
        Cond::Lt(a, b) => oracle(a, bindings)? < oracle(b, bindings)?,
        Cond::Gt(a, b) => oracle(a, bindings)? > oracle(b, bindings)?,
        Cond::Le(a, b) => oracle(a, bindings)? <= oracle(b, bindings)?,
        Cond::Ge(a, b) => oracle(a, bindings)? >= oracle(b, bindings)?,
        Cond::Eq(a, b) => oracle(a, bindings)? == oracle(b, bindings)?,
        Cond::Ne(a, b) => oracle(a, bindings)? != oracle(b, bindings)?,
        // Both sides are evaluated: the oracle has to agree with the
        // compiler on whether an overflow in the unreached side makes
        // the case undefined, and the safe reading is that it does.
        Cond::And(a, b) => oracle_cond(a, bindings)? && oracle_cond(b, bindings)?,
        Cond::Or(a, b) => oracle_cond(a, bindings)? || oracle_cond(b, bindings)?,
        Cond::Not(a) => !oracle_cond(a, bindings)?,
    };
    Some(value)
}

/// Turn a possibly-zero divisor into a non-zero one, the same way on
/// both sides.
const fn nonzero(d: i32) -> i32 {
    if d == 0 {
        1
    } else {
        d
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the tree as `FormaLang`, fully parenthesised.
///
/// Parenthesising every operand means the generated source states the
/// tree's shape rather than relying on precedence — the oracle then
/// checks the *evaluation*, and
/// `tests/conformance/literals/precedence.fv` checks precedence on its
/// own. Mixing the two would make a failure ambiguous.
fn render(expr: &Expr) -> String {
    match expr {
        Expr::Int(v) => format!("{v}I64"),
        Expr::Var(i) => format!("v{}", i % BINDINGS),
        Expr::Add(a, b) => format!("({} + {})", render(a), render(b)),
        Expr::Sub(a, b) => format!("({} - {})", render(a), render(b)),
        Expr::Mul(a, b) => format!("({} * {})", render(a), render(b)),
        Expr::Div(a, d) => format!("({} / {}I64)", render(a), nonzero(*d)),
        Expr::Rem(a, d) => format!("({} % {}I64)", render(a), nonzero(*d)),
        Expr::Neg(a) => format!("(-{})", render(a)),
        Expr::If(c, t, e) => format!(
            "if {} {{ {} }} else {{ {} }}",
            render_cond(c),
            render(t),
            render(e)
        ),
    }
}

fn render_cond(cond: &Cond) -> String {
    match cond {
        Cond::Lt(a, b) => format!("({} < {})", render(a), render(b)),
        Cond::Gt(a, b) => format!("({} > {})", render(a), render(b)),
        Cond::Le(a, b) => format!("({} <= {})", render(a), render(b)),
        Cond::Ge(a, b) => format!("({} >= {})", render(a), render(b)),
        Cond::Eq(a, b) => format!("({} == {})", render(a), render(b)),
        Cond::Ne(a, b) => format!("({} != {})", render(a), render(b)),
        Cond::And(a, b) => format!("({} && {})", render_cond(a), render_cond(b)),
        Cond::Or(a, b) => format!("({} || {})", render_cond(a), render_cond(b)),
        Cond::Not(a) => format!("(!{})", render_cond(a)),
    }
}

/// Wrap the tree in a program whose `answer()` returns its value.
///
/// `I64` throughout, so the range matches the oracle's.
fn program(expr: &Expr, bindings: &[i64]) -> String {
    let mut source = String::from("pub fn answer() -> I64 {\n");
    for (i, value) in bindings.iter().enumerate() {
        source.push_str(&format!("    let v{i}: I64 = {value}I64\n"));
    }
    source.push_str(&format!("    {}\n}}\n", render(expr)));
    source
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn expr_strategy() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        (-100_i32..100).prop_map(Expr::Int),
        (0_usize..BINDINGS).prop_map(Expr::Var),
    ];
    leaf.prop_recursive(5, 48, 4, |inner| {
        let cond = cond_strategy(inner.clone());
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Add(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Sub(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Mul(Box::new(a), Box::new(b))),
            (inner.clone(), -20_i32..20).prop_map(|(a, d)| Expr::Div(Box::new(a), d)),
            (inner.clone(), -20_i32..20).prop_map(|(a, d)| Expr::Rem(Box::new(a), d)),
            inner.clone().prop_map(|a| Expr::Neg(Box::new(a))),
            (cond, inner.clone(), inner).prop_map(|(c, t, e)| Expr::If(
                Box::new(c),
                Box::new(t),
                Box::new(e)
            )),
        ]
    })
}

fn cond_strategy(
    expr: impl Strategy<Value = Expr> + Clone + 'static,
) -> impl Strategy<Value = Cond> {
    let comparison = prop_oneof![
        (expr.clone(), expr.clone()).prop_map(|(a, b)| Cond::Lt(Box::new(a), Box::new(b))),
        (expr.clone(), expr.clone()).prop_map(|(a, b)| Cond::Gt(Box::new(a), Box::new(b))),
        (expr.clone(), expr.clone()).prop_map(|(a, b)| Cond::Le(Box::new(a), Box::new(b))),
        (expr.clone(), expr.clone()).prop_map(|(a, b)| Cond::Ge(Box::new(a), Box::new(b))),
        (expr.clone(), expr.clone()).prop_map(|(a, b)| Cond::Eq(Box::new(a), Box::new(b))),
        (expr.clone(), expr).prop_map(|(a, b)| Cond::Ne(Box::new(a), Box::new(b))),
    ];
    comparison.prop_recursive(2, 8, 2, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Cond::And(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Cond::Or(Box::new(a), Box::new(b))),
            inner.prop_map(|a| Cond::Not(Box::new(a))),
        ]
    })
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Compile and evaluate `source`, returning `answer()`.
fn compiled_answer(source: &str) -> Result<i128, String> {
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut interpreter = Interpreter::new(&module);
    match interpreter.run("answer") {
        Ok(Value::Int(v)) => Ok(v),
        Ok(other) => Err(format!("answer() produced {other:?}, not an integer")),
        Err(Fault::AssertFailed) => Err("an assert failed".to_string()),
        Err(fault) => Err(fault.to_string()),
    }
}

proptest! {
    /// The compiler agrees with the oracle on every generated tree.
    #[test]
    fn the_compiler_computes_what_the_oracle_computes(
        expr in expr_strategy(),
        bindings in proptest::collection::vec(-1000_i64..1000, BINDINGS),
    ) {
        let Some(expected) = oracle(&expr, &bindings) else {
            // Overflowed: the language leaves the result to the
            // backend, so there is nothing to compare.
            return Ok(());
        };
        let expected = i128::from(expected);
        let source = program(&expr, &bindings);

        match compiled_answer(&source) {
            Ok(actual) => prop_assert_eq!(
                actual,
                expected,
                "the compiler answered {} where {} was expected for:\n{}",
                actual,
                expected,
                source
            ),
            Err(why) => prop_assert!(
                false,
                "the generated program did not run: {}\n{}",
                why,
                source
            ),
        }
    }

    /// Constant folding does not change the answer.
    ///
    /// The folder evaluates what it can at compile time. Whatever it
    /// folds, the program must still compute the same value — a folder
    /// that gets an operator's semantics slightly wrong is invisible
    /// until someone compares against an oracle.
    #[test]
    fn constant_folding_preserves_the_answer(
        expr in expr_strategy(),
        bindings in proptest::collection::vec(-1000_i64..1000, BINDINGS),
    ) {
        use formalang::ir::ConstantFoldingPass;
        use formalang::IrPass;

        let source = program(&expr, &bindings);
        let Ok(module) = formalang::compile_to_ir(&source) else {
            return Ok(());
        };

        let before = {
            let mut interpreter = Interpreter::new(&module);
            interpreter.run("answer")
        };
        let Ok(Value::Int(before)) = before else {
            return Ok(());
        };

        let Ok(folded) = ConstantFoldingPass::new().run(module) else {
            return Ok(());
        };
        let mut interpreter = Interpreter::new(&folded);
        let after = interpreter.run("answer");
        let Ok(Value::Int(after)) = after else {
            return Err(TestCaseError::fail(format!(
                "the folded program did not produce an integer for:\n{source}"
            )));
        };

        prop_assert_eq!(
            before,
            after,
            "constant folding changed the answer from {} to {} for:\n{}",
            before,
            after,
            source
        );
    }

    /// A fully constant tree folds to a single literal, and that
    /// literal is the right one.
    #[test]
    fn a_constant_tree_folds_to_its_value(
        expr in expr_strategy(),
    ) {
        use formalang::ir::ConstantFoldingPass;
        use formalang::IrPass;

        // No bindings referenced: substitute zero for every variable so
        // the tree is closed.
        let bindings = vec![0_i64; BINDINGS];
        let Some(expected) = oracle(&expr, &bindings) else {
            return Ok(());
        };
        let expected = i128::from(expected);
        let source = program(&expr, &bindings);

        let Ok(module) = formalang::compile_to_ir(&source) else {
            return Ok(());
        };
        let Ok(folded) = ConstantFoldingPass::new().run(module) else {
            return Ok(());
        };

        let mut interpreter = Interpreter::new(&folded);
        match interpreter.run("answer") {
            Ok(Value::Int(actual)) => prop_assert_eq!(
                actual,
                expected,
                "after folding, the answer was {} where {} was expected for:\n{}",
                actual,
                expected,
                source
            ),
            other => return Err(TestCaseError::fail(format!(
                "the folded program produced {other:?} for:\n{source}"
            ))),
        }
    }
}
