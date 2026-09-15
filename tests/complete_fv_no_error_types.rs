//! Regression coverage for the IR-shape invariant on `complete.fv`:
//! after each stage of the canonical codegen pipeline, no expression
//! should carry `ResolvedType::Error`.
//!
//! Previously flagged as a `ResolvedType::Error` issue in
//! `MonomorphisePass` that blocked the full pipeline on `complete.fv`.
//! The fix landed before this test; this guard pins the
//! absence-of-regression rather than driving new behaviour.

#![expect(
    clippy::expect_used,
    reason = "test asserts pipeline success; expect() is the desired panic-on-failure shape"
)]

use formalang::compile_to_ir;
use formalang::ir::{
    walk_expr_children, walk_module, IrExpr, IrModule, IrVisitor, ReferenceTarget, ResolvedType,
};
use formalang::Pipeline;

struct ErrorCounter(usize);
impl IrVisitor for ErrorCounter {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if matches!(expr.ty(), ResolvedType::Error) {
            self.0 = self.0.saturating_add(1);
        }
        walk_expr_children(self, expr);
    }
}

fn count_error_types(m: &IrModule) -> usize {
    let mut c = ErrorCounter(0);
    walk_module(&mut c, m);
    c.0
}

struct UnresolvedTargetCounter(usize);
impl IrVisitor for UnresolvedTargetCounter {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExpr::Reference { target, .. } = expr {
            if matches!(target, ReferenceTarget::Unresolved) {
                self.0 = self.0.saturating_add(1);
            }
        }
        walk_expr_children(self, expr);
    }
}

fn count_unresolved_targets(m: &IrModule) -> usize {
    let mut c = UnresolvedTargetCounter(0);
    walk_module(&mut c, m);
    c.0
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the inline FormaLang fixture covers a full surface in one program; splitting it would lose the integration coverage"
)]
fn complete_fv_has_no_error_types_through_the_full_pipeline() {
    // Inlined comprehensive program covering enums (with and without
    // data), traits, structs (optional / default fields), impls, free
    // functions, parameter conventions (`mut`, `sink`), module-level
    // `let` bindings (including `pub let mut`), instantiation,
    // destructuring, tuples, dictionaries, arithmetic / comparison
    // operators, `if`, `for`, `match`, inferred enums, and closures
    // (default, `mut`, `sink`, no-arg). Mirrors the same surface area
    // the previous `fixtures/complete.fv` exercised, updated for the
    // current closure-syntax requirements (parens around params, no
    // pipe form, no field-level `mut`).
    let source = r#"
pub enum Priority {
    low,
    medium,
    high(urgency: I32)
}

pub enum Status {
    pending,
    done(score: I32)
}

pub trait Labeled {
    label: String
}

pub trait Tracked: Labeled {
    label: String,
    created_at: I32
}

struct Logger {}
extern impl Logger { fn log(self, message: String) }
extern fn get_logger() -> Logger

pub struct Task {
    id: I32,
    label: String,
    created_at: I32,
    priority: Priority,
    status: Status,
    tags: [String],
    notes: String? = "none",
    retry_count: I32 = 0
}

pub struct Config {
    max_retries: I32 = 3,
    debug: Boolean = false
}

impl Task {
    fn is_done(self) -> Boolean {
        match self.status {
            .pending: false,
            .done(score): true
        }
    }

    fn priority_score(self) -> I32 {
        match self.priority {
            .low: 1,
            .medium: 5,
            .high(urgency): urgency
        }
    }

    fn describe(self) -> String {
        if self.is_done() {
            self.label + " (done)"
        } else {
            self.label + " (pending)"
        }
    }

    fn next_retry(self) -> I32 {
        self.retry_count + 1
    }
}

pub fn clamp(value: I32, min: I32, max: I32) -> I32 {
    if value < min {
        min
    } else {
        if value > max { max } else { value }
    }
}

pub fn score_label(score: I32) -> String {
    if score > 7 { "high" } else { if score > 3 { "medium" } else { "low" } }
}

pub fn apply_bonus(mut score: I32, bonus: I32) -> I32 {
    score
}

pub fn consume_label(sink text: String) -> String {
    text
}

pub fn describe_priority(priority: Priority) -> String {
    match priority {
        .low: "low",
        .medium: "medium",
        .high(urgency): "high"
    }
}

pub let max_retries: I32 = 3
pub let mut task_count: I32 = 0
let default_label: String = "untitled"

pub let sample = Task(
    id: 1,
    label: "Write integration test",
    created_at: 1000000,
    priority: Priority.high(urgency: 9),
    status: Status.pending,
    tags: ["testing", "compiler"]
)

pub let cfg = Config()

let [first_tag, second_tag] = sample.tags

let { label, id } = sample

let coords: (x: I32, y: I32) = (x: 10, y: 20)
let (x, y) = coords

let metadata: [String: String] = ["version": "1.0", "env": "test"]
let version: String? = metadata["version"]

let raw_score: I32 = sample.priority_score()
let clamped: I32 = clamp(raw_score, 0, 10)
let is_urgent: Boolean = raw_score > 7
let is_valid: Boolean = clamped >= 0 && clamped <= 10

let status_text: String = if sample.is_done() { "done" } else { "pending" }

let upper_tags: [String] = for tag in sample.tags { tag }.collect()

let priority_name: String = match sample.priority {
    .low: "low",
    .medium: "medium",
    .high(urgency): "high"
}

let default_status: Status = .pending

let format_tag: (String) -> String = (t: String) -> "[" + t + "]"
let make_label: () -> String = () -> default_label

// mut convention: callee receives an exclusive mutable view
let bump_score: (mut I32) -> I32 = (mut n) -> n

// sink convention: callee takes ownership; caller cannot use arg after
let consume_label_c: (sink String) -> String = (sink s) -> s

let final_score: I32 = (
    let base: I32 = sample.priority_score()
    in let bonus: I32 = if is_urgent { 5 } else { 0 }
    in base + bonus
)
"#;
    let module = compile_to_ir(source).expect("compile");
    assert_eq!(count_error_types(&module), 0, "before any pass");

    let m = Pipeline::for_codegen().run(module).expect("full pipeline");
    assert_eq!(count_error_types(&m), 0, "after Pipeline::for_codegen()");
    // After the full pipeline, every `IrExpr::Reference` (including
    // the synthesised `__env` refs inside lifted closure bodies)
    // must carry a resolved `ReferenceTarget`. `Unresolved` here is
    // a regression: backends keying on `target` would emit broken
    // code or fail their own `UndefinedReference` validation.
    assert_eq!(
        count_unresolved_targets(&m),
        0,
        "after Pipeline::for_codegen()"
    );
}
