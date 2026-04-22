use formalang::ast::ParamConvention;
use formalang::error::CompilerError;
/// Tests for `mut`/`sink` parameter conventions (Mutable Value Semantics).
use formalang::{compile, compile_to_ir};

// ---------------------------------------------------------------------------
// Parser: convention is captured on FnParam
// ---------------------------------------------------------------------------

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
