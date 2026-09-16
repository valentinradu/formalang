//! Defunctionalisation removes every closure value and every indirect
//! call from the module.
//!
//! `ClosureConversionPass` lifts each closure body to a top-level
//! function and collects the captures into an env struct. It leaves
//! open what the closure *value* is. This pass answers that with a
//! tag: an enum with one variant per lifted function, dispatched by a
//! `match`.
//!
//! The security argument is the reason to prefer a tag over a function
//! address. With an address, a code address lives inside a data value,
//! and corrupting those bytes becomes a jump to anywhere. With a tag,
//! the same corruption at worst selects the wrong arm — a wrong
//! answer, not a takeover.

#![expect(
    clippy::expect_used,
    clippy::unreachable,
    reason = "tests assert the pass succeeds; panicking on failure is the desired shape"
)]

use formalang::ir::{
    walk_expr_children, walk_module, DefunctionalisePass, IrExpr, IrModule, IrVisitor, ResolvedType,
};
use formalang::{compile_to_ir, IrPass, Pipeline};

fn defunctionalised(source: &str) -> IrModule {
    let module = compile_to_ir(source).expect("source compiles");
    Pipeline::new()
        .pass(formalang::ir::MonomorphisePass::default())
        .pass(formalang::ir::ResolveReferencesPass::new())
        .pass(DefunctionalisePass::new())
        .run(module)
        .expect("defunctionalisation succeeds")
}

#[derive(Default)]
struct Survey {
    closure_refs: usize,
    indirect_calls: usize,
    closure_types: usize,
    enum_instantiations: usize,
}

impl IrVisitor for Survey {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match expr {
            IrExpr::ClosureRef { .. } => self.closure_refs = self.closure_refs.saturating_add(1),
            IrExpr::CallClosure { .. } => {
                self.indirect_calls = self.indirect_calls.saturating_add(1);
            }
            IrExpr::EnumInst { .. } => {
                self.enum_instantiations = self.enum_instantiations.saturating_add(1);
            }
            IrExpr::Literal { .. }
            | IrExpr::StructInst { .. }
            | IrExpr::Array { .. }
            | IrExpr::Tuple { .. }
            | IrExpr::Reference { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::FieldAccess { .. }
            | IrExpr::LetRef { .. }
            | IrExpr::BinaryOp { .. }
            | IrExpr::UnaryOp { .. }
            | IrExpr::If { .. }
            | IrExpr::For { .. }
            | IrExpr::Match { .. }
            | IrExpr::FunctionCall { .. }
            | IrExpr::MethodCall { .. }
            | IrExpr::Closure { .. }
            | IrExpr::DictLiteral { .. }
            | IrExpr::DictAccess { .. }
            | IrExpr::Block { .. } => {}
        }
        if matches!(expr.ty(), ResolvedType::Closure { .. }) {
            self.closure_types = self.closure_types.saturating_add(1);
        }
        walk_expr_children(self, expr);
    }
}

fn survey(module: &IrModule) -> Survey {
    let mut s = Survey::default();
    walk_module(&mut s, module);
    s
}

const EVENT_CLOSURES: &str = "
pub enum Event { pressed, changed(value: I32) }

struct Button { on_press: () -> Event = () -> .pressed }

pub fn dispatch(flag: Boolean) -> Event {
  let f: () -> Event = if flag { () -> .pressed } else { () -> .changed(value: 1) }
  f()
}
";

#[test]
fn no_closure_value_survives() {
    let s = survey(&defunctionalised(EVENT_CLOSURES));
    assert_eq!(s.closure_refs, 0, "a closure value must become a tag");
}

/// The property the whole pass exists for: nothing in the output
/// jumps through an address held in data.
#[test]
fn no_indirect_call_survives() {
    let s = survey(&defunctionalised(EVENT_CLOSURES));
    assert_eq!(
        s.indirect_calls, 0,
        "an indirect call must become a direct one"
    );
}

#[test]
fn no_expression_keeps_a_closure_type() {
    let s = survey(&defunctionalised(EVENT_CLOSURES));
    assert_eq!(
        s.closure_types, 0,
        "a closure type must become its tag enum"
    );
}

#[test]
fn closure_values_become_enum_instantiations() {
    let s = survey(&defunctionalised(EVENT_CLOSURES));
    assert!(
        s.enum_instantiations >= 3,
        "each of the three closures becomes a tag, got {}",
        s.enum_instantiations
    );
}

/// Closures of one shape share one enum, so the tag stays small and
/// every arm of the dispatch is a direct call.
#[test]
fn one_enum_per_closure_shape() {
    let module = defunctionalised(EVENT_CLOSURES);
    let tags: Vec<_> = module
        .enums
        .iter()
        .filter(|e| e.name.starts_with("__Fn"))
        .collect();
    assert_eq!(tags.len(), 1, "every closure here is `() -> Event`");
    let Some(tag) = tags.first() else {
        unreachable!("length checked above")
    };
    assert_eq!(tag.variants.len(), 3, "one variant per lifted closure");
}

#[test]
fn two_shapes_get_two_enums() {
    let module = defunctionalised(
        "pub enum Event { pressed }
struct Button { on_press: () -> Event = () -> .pressed }
struct Slider { on_move: (I32) -> Event = (n: I32) -> .pressed }
pub let n: I32 = 0",
    );
    let tags = module
        .enums
        .iter()
        .filter(|e| e.name.starts_with("__Fn"))
        .count();
    assert_eq!(tags, 2, "`() -> Event` and `(I32) -> Event` are two shapes");
}

#[test]
fn the_dispatch_function_matches_on_the_tag() {
    let module = defunctionalised(EVENT_CLOSURES);
    let dispatch = module
        .functions
        .iter()
        .find(|f| f.name.starts_with("__call_Fn"))
        .expect("a dispatch function exists");
    let body = dispatch.body.as_ref().expect("the dispatch has a body");
    let IrExpr::Match { arms, .. } = body else {
        unreachable!("the dispatch body is a match, got {body:?}")
    };
    assert_eq!(arms.len(), 3, "one arm per lifted closure");
    for arm in arms {
        assert!(
            matches!(&arm.body, IrExpr::FunctionCall { .. }),
            "every arm is a direct call, got {:?}",
            arm.body
        );
    }
}

/// A capture has to reach the lifted function, so the variant carries
/// the env struct closure conversion built.
#[test]
fn captures_travel_in_the_variant() {
    let module = defunctionalised(
        "pub fn make(sink n: I32) -> I32 {
  let f: (I32) -> I32 = (x: I32) -> x + n
  f(1)
}",
    );
    let tag = module
        .enums
        .iter()
        .find(|e| e.name.starts_with("__Fn"))
        .expect("a tag enum exists");
    let variant = tag.variants.first().expect("a variant exists");
    assert_eq!(variant.fields.len(), 1, "the variant carries the env");
    let Some(env) = variant.fields.first() else {
        unreachable!("length checked above")
    };
    assert_eq!(env.name, "env");
}

#[test]
fn a_module_with_no_closure_is_left_alone() {
    let module = defunctionalised("pub fn id(x: I32) -> I32 { x }");
    assert!(
        !module.enums.iter().any(|e| e.name.starts_with("__Fn")),
        "nothing to do, so nothing synthesised"
    );
    assert!(
        !module
            .functions
            .iter()
            .any(|f| f.name.starts_with("__call_Fn")),
        "no dispatch function without a closure"
    );
}

#[test]
fn running_twice_changes_nothing() {
    let once = defunctionalised(EVENT_CLOSURES);
    let enums = once.enums.len();
    let functions = once.functions.len();
    let twice = DefunctionalisePass::new()
        .run(once)
        .expect("a second run succeeds");
    assert_eq!(twice.enums.len(), enums, "no new enum on a second run");
    assert_eq!(
        twice.functions.len(),
        functions,
        "no new dispatch on a second run"
    );
}
