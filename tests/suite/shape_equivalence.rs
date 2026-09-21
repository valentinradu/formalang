//! The same computation, written in different shapes, must give the
//! same answer.
//!
//! A metamorphic test, and the answer to a gap the rest of the suite
//! has. Counting the generated programs in this repository:
//!
//! | suite | programs | asserts |
//! | --- | --- | --- |
//! | `type_matrix` | 3584 | accepted or rejected |
//! | `operator_matrix` | 3584 | accepted or rejected |
//! | `no_internal_errors` | 2260 | not an internal error |
//! | `method_matrix` | 255 | accepted or rejected |
//!
//! Nearly ten thousand generated programs, and **not one of them is
//! executed**. Every one asks the compiler for a verdict and stops
//! there. A defect that makes a program compile and then compute the
//! wrong answer is invisible to all of them.
//!
//! One did exactly that. Newline handling counted `(` and `[` but
//! never reset the count at `{`, so a block written inside a call
//! argument lost its statement breaks: `a` on one line and `-1` on the
//! next became `a - 1`. It compiled. It type-checked. It answered 9
//! where the same body outside a call answered -1.
//!
//! So this file does not check verdicts. It writes one computation
//! several ways — in a function body, inside a call argument, inside a
//! block, through a closure, behind a `let` — and asserts that every
//! spelling produces the same value. There is no oracle to get wrong:
//! the shapes are each other's oracle.

use crate::common::interpreter::Interpreter;
use crate::common::Checked;

use formalang::compile_to_ir;

/// One computation, and the ways it can be written.
///
/// `body` is a sequence of statements ending in a value, written
/// against a parameter called `n`. Each spelling places that same body
/// somewhere different.
struct Computation {
    what: &'static str,
    body: &'static str,
    /// The argument the shapes are run with.
    input: i64,
}

const COMPUTATIONS: &[Computation] = &[
    Computation {
        what: "a binding then a negative literal",
        body: "let a = n\n        a\n        -1",
        input: 10,
    },
    Computation {
        what: "two bindings then a sum",
        body: "let a = n\n        let b = 2\n        a + b",
        input: 5,
    },
    Computation {
        what: "a binding used twice",
        body: "let a = n + 1\n        a * a",
        input: 3,
    },
    Computation {
        what: "an if with both branches",
        body: "if n > 0 { n * 2 } else { 0 - n }",
        input: 7,
    },
    Computation {
        what: "a nested block",
        body: "let a = {\n            let b = n\n            b + 1\n        }\n        a * 2",
        input: 4,
    },
    Computation {
        what: "a match over a range check",
        body: "let a = n\n        if a > 100 { 1 } else { a }",
        input: 6,
    },
    Computation {
        what: "a loop folded to a value",
        body: "for i in 0..n { i }.fold(initial: 0, f: (a, b) -> a + b)",
        input: 5,
    },
    Computation {
        what: "a statement then a unary minus",
        body: "let a = n * 2\n        a\n        -a",
        input: 8,
    },
];

/// One way of writing a body. Each returns `I32` from a function
/// called `shaped`, taking `n: I32`.
struct Shape {
    what: &'static str,
    render: fn(&str) -> String,
}

const SHAPES: &[Shape] = &[
    Shape {
        what: "a plain function body",
        render: |body| format!("pub fn shaped(n: I32) -> I32 {{\n        {body}\n}}\n"),
    },
    Shape {
        what: "a closure inside a call argument",
        render: |body| {
            format!(
                "fn apply(op: (I32) -> I32, x: I32) -> I32 {{\n    op(x)\n}}\n\n\
                 pub fn shaped(n: I32) -> I32 {{\n    apply(op: (n: I32) -> {{\n        {body}\n    }}, x: n)\n}}\n"
            )
        },
    },
    Shape {
        what: "a block bound by a let",
        render: |body| {
            format!(
                "pub fn shaped(n: I32) -> I32 {{\n    let out = {{\n        {body}\n    }}\n    out\n}}\n"
            )
        },
    },
    Shape {
        what: "a closure called immediately",
        render: |body| {
            format!(
                "pub fn shaped(n: I32) -> I32 {{\n    let op = (n: I32) -> {{\n        {body}\n    }}\n    op(n)\n}}\n"
            )
        },
    },
    Shape {
        what: "a closure passed through a second function",
        render: |body| {
            format!(
                "fn twice(op: (I32) -> I32, x: I32) -> I32 {{\n    op(x)\n}}\n\n\
                 fn once(op: (I32) -> I32, x: I32) -> I32 {{\n    twice(op: op, x: x)\n}}\n\n\
                 pub fn shaped(n: I32) -> I32 {{\n    once(op: (n: I32) -> {{\n        {body}\n    }}, x: n)\n}}\n"
            )
        },
    },
    Shape {
        what: "a block inside a nested call argument",
        render: |body| {
            format!(
                "fn apply(op: (I32) -> I32, x: I32) -> I32 {{\n    op(x)\n}}\n\n\
                 pub fn shaped(n: I32) -> I32 {{\n    apply(op: (n: I32) -> {{\n        \
                 let inner = apply(op: (m: I32) -> {{\n            {body}\n        }}, x: n)\n        \
                 inner\n    }}, x: n)\n}}\n"
            )
        },
    },
    Shape {
        what: "a body behind a second call",
        render: |body| {
            format!(
                "fn inner(n: I32) -> I32 {{\n        {body}\n}}\n\n\
                 pub fn shaped(n: I32) -> I32 {{\n    inner(n: n)\n}}\n"
            )
        },
    },
];

/// Compile a program and run `shaped(n: input)`, returning its value.
///
/// `None` when the program does not compile or the shape cannot be
/// evaluated — the caller reports that separately from a disagreement.
fn evaluate(source: &str, input: i64) -> Result<i128, String> {
    // The interpreter runs a function that takes nothing, so the
    // argument is baked into a probe the shape does not know about.
    let with_probe = format!("{source}\npub fn probe() -> I32 {{\n    shaped(n: {input})\n}}\n");
    let module = compile_to_ir(&with_probe).map_err(|e| format!("did not compile: {e:?}"))?;
    let mut interpreter = Interpreter::new(&module);
    let value = interpreter
        .run("probe")
        .map_err(|e| format!("did not run: {e:?}"))?;
    value
        .as_int()
        .map_err(|e| format!("did not produce an integer: {e:?}"))
}

#[test]
fn every_shape_of_one_computation_agrees() {
    let mut checked = Checked::new("shape comparisons", 40);
    let mut disagreements = Vec::new();
    let mut failures = Vec::new();

    for computation in COMPUTATIONS {
        let mut answers: Vec<(&str, i128)> = Vec::new();

        for shape in SHAPES {
            let source = (shape.render)(computation.body);
            match evaluate(&source, computation.input) {
                Ok(value) => answers.push((shape.what, value)),
                Err(why) => failures.push(format!(
                    "{} as {}: {why}\n--- source ---\n{source}",
                    computation.what, shape.what
                )),
            }
            checked.hit();
        }

        let Some((first_shape, first)) = answers.first().copied() else {
            continue;
        };
        for (shape, value) in answers.iter().skip(1) {
            if *value != first {
                disagreements.push(format!(
                    "{}: as {first_shape} it is {first}, as {shape} it is {value}",
                    computation.what
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} shape(s) did not compile or run:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        disagreements.is_empty(),
        "{} computation(s) gave different answers depending on how they were \
         written. The shapes are each other's oracle, so one of them is a \
         defect:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
}

/// The generator must keep comparing more than one shape.
///
/// A refactor that emptied `SHAPES` would leave the test above passing
/// on nothing to compare.
#[test]
fn the_generator_compares_several_shapes() {
    assert!(
        SHAPES.len() >= 4,
        "only {} shape(s) to compare; the test needs several",
        SHAPES.len()
    );
    assert!(
        COMPUTATIONS.len() >= 5,
        "only {} computation(s); the test needs several",
        COMPUTATIONS.len()
    );
}
