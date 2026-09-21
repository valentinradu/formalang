//! Every context must hand a value back unchanged.
//!
//! The other matrices in this repository generate programs and ask the
//! compiler for a verdict. This one generates programs and **runs**
//! them.
//!
//! The gap that motivates it: `type_matrix`, `operator_matrix`,
//! `method_matrix` and `no_internal_errors` together compile close to
//! ten thousand programs, and none of them executes one. A defect that
//! lets a program compile and then compute the wrong answer is
//! invisible to all four. One did exactly that — a block inside a call
//! argument lost its statement breaks, so `a` and `-1` on separate
//! lines became `a - 1`. It compiled, it type-checked, and it answered
//! 9 instead of -1.
//!
//! No oracle is needed here, and none is written down. A value put into
//! a context and read back out must equal itself, and the comparison is
//! made by the program under test:
//!
//! ```text
//! pub fn probe() -> I32 {
//!     let got: I32 = 7          // the context
//!     assert(condition: got == 7)
//!     0
//! }
//! ```
//!
//! So the test asserts three things about each cell: it compiles, it
//! runs without faulting, and the assertion inside it passed. A cell
//! that does not compile is reported separately — every pairing here is
//! meant to be legal.

use crate::common::interpreter::{Fault, Interpreter};
use crate::common::matrix::{program, Context, Value, CONTEXTS, VALUES};
use crate::common::Checked;

use formalang::compile_to_ir;

/// What happened to one cell.
enum Outcome {
    Passed,
    DidNotCompile(String),
    Faulted(String),
    AssertedNothing,
}

fn run_cell(value: &Value, context: &Context) -> Outcome {
    let source = program(value, context);
    let module = match compile_to_ir(&source) {
        Ok(m) => m,
        Err(errors) => return Outcome::DidNotCompile(format!("{errors:?}")),
    };
    let mut interpreter = Interpreter::new(&module);
    match interpreter.run("probe") {
        Err(Fault::AssertFailed) => Outcome::Faulted("the value came back changed".to_string()),
        Err(other) => Outcome::Faulted(format!("{other:?}")),
        Ok(_) if interpreter.asserts_passed == 0 => Outcome::AssertedNothing,
        Ok(_) => Outcome::Passed,
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Every value survives every context unchanged.
#[test]
fn a_value_comes_back_out_of_every_context() {
    let mut checked = Checked::new("executed cells", 200);
    let mut changed = Vec::new();
    let mut unrunnable = Vec::new();
    let mut passed = 0_usize;

    for value in VALUES {
        for context in CONTEXTS {
            if value.skip.contains(&context.what) {
                continue;
            }
            match run_cell(value, context) {
                Outcome::Passed => passed = passed.saturating_add(1),
                Outcome::Faulted(why) => {
                    changed.push(format!("{} through {}: {why}", value.name, context.what));
                }
                Outcome::AssertedNothing => {
                    changed.push(format!(
                        "{} through {}: the program ran but its assert never executed",
                        value.name, context.what
                    ));
                }
                Outcome::DidNotCompile(errors) => {
                    unrunnable.push(format!("{} through {}: {errors}", value.name, context.what));
                }
            }
            checked.hit();
        }
    }

    assert!(
        changed.is_empty(),
        "{} cell(s) did not hand the value back unchanged:\n  {}",
        changed.len(),
        changed.join("\n  ")
    );
    assert!(
        unrunnable.is_empty(),
        "{} cell(s) did not compile. Every pairing here is meant to be \
         legal, so a failure is either a defect or a wrong expectation:\n  {}",
        unrunnable.len(),
        unrunnable.join("\n  ")
    );
    assert!(
        passed > 200,
        "only {passed} cell(s) actually ran and checked something"
    );
}

/// The matrix keeps covering a wide surface.
#[test]
fn the_matrix_runs_more_than_it_compiles() {
    assert!(
        VALUES.len() >= 10,
        "only {} value(s); the matrix needs several",
        VALUES.len()
    );
    assert!(
        CONTEXTS.len() >= 12,
        "only {} context(s); the matrix needs several",
        CONTEXTS.len()
    );
}
