#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use formalang::ast::ParamConvention;
/// Tests for `mut`/`sink` parameter conventions (Mutable Value Semantics).
use formalang::compile_to_ir;
use formalang::error::CompilerError;

// ---------------------------------------------------------------------------
// Parser: convention is captured on FnParam
// ---------------------------------------------------------------------------

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

fn parse_ok(src: &str) -> formalang::ast::File {
    compile(src).expect("expected parse success")
}

fn first_fn_params(src: &str) -> Vec<formalang::ast::FnParam> {
    use formalang::ast::{Definition, Statement};
    let file = parse_ok(src);
    for stmt in &file.statements {
        if let Statement::Definition(def) = stmt {
            if let Definition::Function(f) = &**def {
                return f.params.clone();
            }
        }
    }
    panic!("no standalone function found in source");
}

fn first_impl_fn_params(src: &str) -> Vec<formalang::ast::FnParam> {
    use formalang::ast::{Definition, Statement};
    let file = parse_ok(src);
    for stmt in &file.statements {
        if let Statement::Definition(def) = stmt {
            if let Definition::Impl(imp) = &**def {
                if let Some(method) = imp.functions.first() {
                    return method.params.clone();
                }
            }
        }
    }
    panic!("no impl method found in source");
}

#[test]
fn test_default_param_convention_is_let() {
    let params = first_fn_params("pub fn read(x: Number) -> Number { x }");
    assert_eq!(params[0].convention, ParamConvention::Let);
}

#[test]
fn test_mut_param_parsed() {
    let params = first_fn_params("pub fn bump(mut x: Number) -> Number { x }");
    assert_eq!(params[0].name.name, "x");
    assert_eq!(params[0].convention, ParamConvention::Mut);
}

#[test]
fn test_sink_param_parsed() {
    let params = first_fn_params("pub fn consume(sink x: Number) -> Number { x }");
    assert_eq!(params[0].name.name, "x");
    assert_eq!(params[0].convention, ParamConvention::Sink);
}

#[test]
fn test_mixed_conventions_in_one_fn() {
    let params =
        first_fn_params("pub fn mixed(a: Number, mut b: Number, sink c: Number) -> Number { a }");
    assert_eq!(params[0].convention, ParamConvention::Let);
    assert_eq!(params[1].convention, ParamConvention::Mut);
    assert_eq!(params[2].convention, ParamConvention::Sink);
}

#[test]
fn test_mut_self_parsed() {
    let params = first_impl_fn_params(
        "pub struct Counter { val: Number }
         impl Counter { fn inc(mut self) -> Number { self.val } }",
    );
    assert_eq!(params[0].name.name, "self");
    assert_eq!(params[0].convention, ParamConvention::Mut);
}

#[test]
fn test_sink_self_parsed() {
    let params = first_impl_fn_params(
        "pub struct Task { id: Number }
         impl Task { fn consume(sink self) -> Number { self.id } }",
    );
    assert_eq!(params[0].name.name, "self");
    assert_eq!(params[0].convention, ParamConvention::Sink);
}

#[test]
fn test_labeled_mut_param_parsed() {
    // `fn foo(external_label internal_name: Type)` — labeled param with mut
    let params = first_fn_params("pub fn foo(val x: Number) -> Number { x }");
    assert_eq!(params[0].convention, ParamConvention::Let);
    assert!(params[0].external_label.is_some());
}

// ---------------------------------------------------------------------------
// IR lowering: convention threads through to IrFunctionParam
// ---------------------------------------------------------------------------

fn ir_ok(src: &str) -> formalang::ir::IrModule {
    compile_to_ir(src).expect("expected IR lowering success")
}

#[test]
fn test_ir_mut_param_convention() {
    let module = ir_ok("pub fn bump(mut x: Number) -> Number { x }");
    let func = module.functions.first().expect("no function");
    assert_eq!(func.params[0].convention, ParamConvention::Mut);
}

#[test]
fn test_ir_sink_param_convention() {
    let module = ir_ok("pub fn consume(sink x: Number) -> Number { x }");
    let func = module.functions.first().expect("no function");
    assert_eq!(func.params[0].convention, ParamConvention::Sink);
}

#[test]
fn test_ir_default_param_convention_is_let() {
    let module = ir_ok("pub fn read(x: Number) -> Number { x }");
    let func = module.functions.first().expect("no function");
    assert_eq!(func.params[0].convention, ParamConvention::Let);
}

// ---------------------------------------------------------------------------
// Semantic enforcement: Let params are immutable
// ---------------------------------------------------------------------------

fn errors(src: &str) -> Vec<CompilerError> {
    compile(src).expect_err("expected semantic errors")
}

fn has_error<F: Fn(&CompilerError) -> bool>(src: &str, pred: F) -> bool {
    errors(src).iter().any(pred)
}

#[test]
fn test_let_param_immutable_in_body() {
    // Assigning to a `let` (default) param inside the body must fail.
    assert!(has_error(
        "pub fn bad(x: Number) -> Number {
            x = 5
            x
        }",
        |e| matches!(e, CompilerError::AssignmentToImmutable { .. }),
    ));
}

// ---------------------------------------------------------------------------
// Semantic enforcement: mut params require a mutable argument at call site
// ---------------------------------------------------------------------------

#[test]
fn test_mut_param_accepts_mutable_arg() {
    // let mut y — argument is mutable, should pass.
    compile(
        "pub fn bump(mut n: Number) -> Number { n }
         let mut y: Number = 5
         let result: Number = bump(y)",
    )
    .expect("mutable arg to mut param should compile");
}

#[test]
fn test_mut_param_rejects_immutable_arg() {
    // let y (immutable) passed to mut param → MutabilityMismatch.
    assert!(has_error(
        "pub fn bump(mut n: Number) -> Number { n }
         let y: Number = 5
         let result: Number = bump(y)",
        |e| matches!(e, CompilerError::MutabilityMismatch { .. }),
    ));
}

#[test]
fn test_let_param_accepts_any_arg() {
    // Immutable arg to a let param is fine.
    compile(
        "pub fn read(n: Number) -> Number { n }
         let y: Number = 5
         let result: Number = read(y)",
    )
    .expect("immutable arg to let param should compile");
}

#[test]
fn test_sink_param_does_not_reject_immutable_arg() {
    // Sink transfers ownership — the caller gives the value; immutability
    // of the source binding is irrelevant (the value is moved).
    compile(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let result: Number = consume(y)",
    )
    .expect("sink param should accept any value");
}

#[test]
fn test_use_after_sink_rejected() {
    // After passing `y` to a sink param it is consumed; a second use is an error.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let _a: Number = consume(y)
         let _b: Number = y",
        |e| matches!(e, CompilerError::UseAfterSink { .. }),
    ));
}

#[test]
fn test_use_after_sink_second_call_rejected() {
    // Passing the same binding twice to sink params is also an error.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let _a: Number = consume(y)
         let _b: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { .. }),
    ));
}

// ---------------------------------------------------------------------------
// Method call convention enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_method_mut_self_rejects_immutable_receiver() {
    assert!(has_error(
        "pub struct Counter { count: Number }
         impl Counter {
             fn bump(mut self) -> Number { self.count }
         }
         let c: Counter = Counter(count: 0)
         let _r: Number = c.bump()",
        |e| matches!(e, CompilerError::MutabilityMismatch { .. }),
    ));
}

#[test]
fn test_method_mut_self_accepts_mutable_receiver() {
    compile(
        "pub struct Counter { count: Number }
         impl Counter {
             fn bump(mut self) -> Number { self.count }
         }
         let mut c: Counter = Counter(count: 0)
         let _r: Number = c.bump()",
    )
    .expect("mutable receiver should satisfy mut self");
}

#[test]
fn test_method_sink_self_consumes_receiver() {
    assert!(has_error(
        "pub struct Box { value: Number }
         impl Box {
             fn unwrap(sink self) -> Number { self.value }
         }
         let b: Box = Box(value: 1)
         let _a: Number = b.unwrap()
         let _again: Number = b.unwrap()",
        |e| matches!(e, CompilerError::UseAfterSink { .. }),
    ));
}

#[test]
fn test_method_mut_param_rejects_immutable_arg() {
    assert!(has_error(
        "pub struct Calc { base: Number }
         impl Calc {
             fn add(self, mut n: Number) -> Number { self.base }
         }
         let c: Calc = Calc(base: 0)
         let x: Number = 5
         let _r: Number = c.add(x)",
        |e| matches!(e, CompilerError::MutabilityMismatch { .. }),
    ));
}

// ---------------------------------------------------------------------------
// Branch isolation: consuming in one branch does not bleed into the other
// ---------------------------------------------------------------------------

#[test]
fn test_sink_in_then_branch_does_not_block_else_branch() {
    // y is only consumed in the then branch; the else branch should see it as live.
    // The compiler conservatively marks y consumed after the if/else union, but within
    // each branch the binding is independent.
    compile(
        "pub fn consume(sink n: Number) -> Number { n }
         pub fn id(n: Number) -> Number { n }
         let y: Number = 5
         let flag: Boolean = true
         let _r: Number = if flag { consume(y) } else { id(y) }",
    )
    .expect("using y in each branch independently should compile");
}

// ---------------------------------------------------------------------------
// Trait method convention mismatch
// ---------------------------------------------------------------------------

#[test]
fn test_trait_method_convention_mismatch_rejected() {
    assert!(has_error(
        "pub trait Mover {
             fn move_it(mut self) -> Number
         }
         pub struct Thing { val: Number }
         impl Mover for Thing {
             fn move_it(self) -> Number { self.val }
         }",
        |e| matches!(e, CompilerError::TraitMethodSignatureMismatch { .. }),
    ));
}

#[test]
fn test_trait_method_convention_match_accepted() {
    compile(
        "pub trait Mover {
             fn move_it(mut self) -> Number
         }
         pub struct Thing { val: Number }
         impl Mover for Thing {
             fn move_it(mut self) -> Number { self.val }
         }",
    )
    .expect("matching convention should satisfy trait requirement");
}

// ---------------------------------------------------------------------------
// Closure parameter conventions: parsing
// ---------------------------------------------------------------------------

fn parse_closure_param_conventions(src: &str) -> Vec<formalang::ast::ParamConvention> {
    use formalang::ast::{Expr, Statement};
    let file = parse_ok(src);
    for stmt in &file.statements {
        if let Statement::Let(binding) = stmt {
            if let Expr::ClosureExpr { params, .. } = &binding.value {
                return params.iter().map(|p| p.convention).collect();
            }
        }
    }
    panic!("no top-level let closure found");
}

#[test]
fn test_closure_param_default_convention_is_let() {
    let convs = parse_closure_param_conventions("let f: Number -> Number = x -> x");
    assert_eq!(convs[0], ParamConvention::Let);
}

#[test]
fn test_closure_param_mut_parsed() {
    let convs = parse_closure_param_conventions("let f: mut Number -> Number = mut x -> x");
    assert_eq!(convs[0], ParamConvention::Mut);
}

#[test]
fn test_closure_param_sink_parsed() {
    let convs = parse_closure_param_conventions("let f: sink Number -> Number = sink x -> x");
    assert_eq!(convs[0], ParamConvention::Sink);
}

#[test]
fn test_closure_mixed_conventions_parsed() {
    let convs = parse_closure_param_conventions(
        "let f: Number, mut Number, sink Number -> Number = |a, mut b, sink c| a",
    );
    assert_eq!(convs[0], ParamConvention::Let);
    assert_eq!(convs[1], ParamConvention::Mut);
    assert_eq!(convs[2], ParamConvention::Sink);
}

// ---------------------------------------------------------------------------
// Closure parameter conventions: type annotation parsing
// ---------------------------------------------------------------------------

fn parse_closure_type_param_conventions(src: &str) -> Vec<formalang::ast::ParamConvention> {
    use formalang::ast::{Statement, Type};
    let file = parse_ok(src);
    for stmt in &file.statements {
        if let Statement::Let(binding) = stmt {
            if let Some(Type::Closure { params, .. }) = &binding.type_annotation {
                return params.iter().map(|(c, _)| *c).collect();
            }
        }
    }
    panic!("no top-level let with closure type annotation found");
}

#[test]
fn test_closure_type_annotation_default_let() {
    let convs = parse_closure_type_param_conventions("let f: Number -> Number = x -> x");
    assert_eq!(convs[0], ParamConvention::Let);
}

#[test]
fn test_closure_type_annotation_mut_convention() {
    let convs = parse_closure_type_param_conventions("let f: mut Number -> Number = mut x -> x");
    assert_eq!(convs[0], ParamConvention::Mut);
}

#[test]
fn test_closure_type_annotation_sink_convention() {
    let convs = parse_closure_type_param_conventions("let f: sink Number -> Number = sink x -> x");
    assert_eq!(convs[0], ParamConvention::Sink);
}

// ---------------------------------------------------------------------------
// Closure parameter conventions: semantic enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_closure_mut_param_rejects_immutable_arg() {
    assert!(has_error(
        "let f: mut Number -> Number = mut x -> x
         let y: Number = 5
         let _r: Number = f(y)",
        |e| matches!(e, CompilerError::MutabilityMismatch { .. }),
    ));
}

#[test]
fn test_closure_mut_param_accepts_mutable_arg() {
    compile(
        "let f: mut Number -> Number = mut x -> x
         let mut y: Number = 5
         let _r: Number = f(y)",
    )
    .expect("mutable arg to closure mut param should compile");
}

#[test]
fn test_closure_let_param_accepts_immutable_arg() {
    compile(
        "let f: Number -> Number = x -> x
         let y: Number = 5
         let _r: Number = f(y)",
    )
    .expect("immutable arg to closure let param should compile");
}

#[test]
fn test_closure_sink_param_consumes_binding() {
    assert!(has_error(
        "let f: sink Number -> Number = sink x -> x
         let y: Number = 5
         let _a: Number = f(y)
         let _b: Number = y",
        |e| matches!(e, CompilerError::UseAfterSink { .. }),
    ));
}

// ---------------------------------------------------------------------------
// Closure capture tracking: detect UseAfterSink when a closure captures a
// binding that is later consumed by a sink parameter, then the closure is
// invoked.
// ---------------------------------------------------------------------------

#[test]
fn test_closure_captures_binding_consumed_after_creation() {
    // Closure `c` captures `y`. After `consume(y)` marks `y` as consumed,
    // invoking `c()` should trigger UseAfterSink because `c`'s body references
    // the consumed `y`.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _a: Number = consume(y)
         let _b: Number = c()",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

#[test]
fn test_closure_captures_binding_not_consumed_compiles() {
    // Positive control: if `y` is never consumed, calling `c()` is fine.
    let result = compile(
        "let y: Number = 5
         let c: () -> Number = () -> y
         let _b: Number = c()",
    );
    assert!(
        result.is_ok(),
        "expected clean compile, got {:?}",
        result.err()
    );
}

#[test]
fn test_closure_captures_consumed_but_not_invoked_compiles() {
    // Negative control: if the closure is never called, capturing a consumed
    // binding is not an error (the closure body is just dormant code).
    let result = compile(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _a: Number = consume(y)",
    );
    assert!(
        result.is_ok(),
        "expected clean compile, got {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Closure escape analysis: when a closure escapes (sink-pass, struct field,
// array/dict entry), its captures are consumed at the escape site. Subsequent
// sink-consumption of a captured binding must be flagged.
// ---------------------------------------------------------------------------

#[test]
fn test_closure_escape_into_struct_field_consumes_captures() {
    // Closure stored in a struct field → captures escape with the struct.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         pub struct Handler { callback: () -> Number }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _h: Handler = Handler(callback: c)
         let _gone: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

#[test]
fn test_closure_sink_pass_consumes_captures() {
    // Closure sink-passed to a function → captures escape.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         pub fn store(sink cb: () -> Number) -> Number { cb() }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _r: Number = store(c)
         let _gone: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

#[test]
fn test_closure_let_pass_does_not_consume_captures() {
    // Positive control: let-pass is a borrow, not escape — the captured `y`
    // remains live after a let-pass of `c`, even when the host function calls
    // the closure parameter inside its body.
    let result = compile(
        "pub fn run(cb: () -> Number) -> Number { cb() }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _r: Number = run(c)
         let _still_live: Number = y",
    );
    assert!(
        result.is_ok(),
        "let-pass should not consume captures; got {:?}",
        result.err()
    );
}

#[test]
fn test_nested_closure_escape_consumes_transitive_captures() {
    // `outer` captures `inner`; when `outer` escapes into a struct, `inner`'s
    // captures (y) are consumed transitively.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         pub struct Outer { cb: () -> Number }
         let y: Number = 5
         let inner: () -> Number = () -> y
         let outer: () -> Number = () -> inner()
         let _o: Outer = Outer(cb: outer)
         let _gone: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

#[test]
fn test_closure_in_array_consumes_captures() {
    // Closure stored as an array element → captures escape with the array.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         let y: Number = 5
         let c: () -> Number = () -> y
         let _arr: [() -> Number] = [c]
         let _gone: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

#[test]
fn test_closure_conditional_escape_consumes_captures() {
    // Closure escapes in one branch but not the other: branch union means
    // the capture is conservatively considered consumed after the merge.
    assert!(has_error(
        "pub fn consume(sink n: Number) -> Number { n }
         pub struct Handler { cb: () -> Number }
         pub fn run(cb: () -> Number) -> Number { 0 }
         let y: Number = 5
         let c: () -> Number = () -> y
         let flag: Boolean = true
         let _r: Number = if flag {
             run(c)
         } else {
             let _h: Handler = Handler(cb: c)
             0
         }
         let _gone: Number = consume(y)",
        |e| matches!(e, CompilerError::UseAfterSink { name, .. } if name == "y"),
    ));
}

// ---------------------------------------------------------------------------
// Closure-typed parameters are invokable inside the host function body
// ---------------------------------------------------------------------------

#[test]
fn test_closure_typed_param_callable() {
    compile(
        "pub fn run(cb: () -> Number) -> Number { cb() }
         let r: Number = run(cb: () -> 5)",
    )
    .expect("closure-typed param should be invokable");
}

#[test]
fn test_closure_typed_param_with_args_callable() {
    compile(
        "pub fn apply(f: Number -> Number, v: Number) -> Number { f(v) }
         let r: Number = apply(f: |n: Number| n + 1, v: 5)",
    )
    .expect("closure param with arg should be invokable");
}

// ---------------------------------------------------------------------------
// Function-return escape check for captured bindings
// ---------------------------------------------------------------------------

#[test]
fn test_fn_returns_closure_capturing_let_param_rejected() {
    // Let param is a view — can't escape.
    assert!(has_error(
        "pub fn make(y: Number) -> () -> Number { () -> y }",
        |e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }),
    ));
}

#[test]
fn test_fn_returns_closure_capturing_sink_param_allowed() {
    // Sink transfers ownership into the closure.
    compile("pub fn make(sink y: Number) -> () -> Number { () -> y }")
        .expect("sink param should be allowed to escape via returned closure");
}

#[test]
fn test_fn_returns_closure_capturing_local_let_rejected() {
    assert!(has_error(
        "pub fn make() -> () -> Number {
             let y: Number = 5
             () -> y
         }",
        |e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }),
    ));
}

#[test]
fn test_fn_returns_closure_capturing_module_let_allowed() {
    compile(
        "let top: Number = 5
         pub fn make() -> () -> Number { () -> top }",
    )
    .expect("module-level let capture should outlive the function");
}

#[test]
fn test_fn_returns_closure_no_capture_allowed() {
    compile("pub fn make() -> () -> Number { () -> 42 }")
        .expect("closure with no captures can always escape");
}

#[test]
fn test_fn_returns_named_closure_capturing_local_let_rejected() {
    // Named closure binding returned; its captures must still be checked.
    assert!(has_error(
        "pub fn make() -> () -> Number {
             let y: Number = 5
             let c: () -> Number = () -> y
             c
         }",
        |e| matches!(e, CompilerError::ClosureCaptureEscapesLocalBinding { .. }),
    ));
}

#[test]
fn test_fn_returns_named_closure_capturing_module_let_allowed() {
    // Module-level capture outlives the function.
    compile(
        "let top: Number = 5
         pub fn make() -> () -> Number {
             let c: () -> Number = () -> top
             c
         }",
    )
    .expect("module capture via named closure binding should be allowed");
}

#[test]
fn test_fn_returns_named_closure_capturing_sink_param_allowed() {
    // Sink param ownership transfers via named binding too.
    compile(
        "pub fn make(sink y: Number) -> () -> Number {
             let c: () -> Number = () -> y
             c
         }",
    )
    .expect("sink-param capture via named closure binding should be allowed");
}
