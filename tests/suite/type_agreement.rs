//! The semantic pass and the IR must give an expression one type.
//!
//! The semantic pass decides what a program means and whether it is
//! legal. The lowering then writes a type on every IR node, and a
//! backend trusts that type. When the two phases disagree, the checked
//! program and the emitted program are different programs. Two of the
//! defects that formajit found (`FOUND_DEFECTS.md`, items 1 and 2) are
//! such a disagreement: the semantic pass did not type an expression,
//! and the lowering typed it alone.
//!
//! Each test here binds one expression to `pub let probe`, with no
//! annotation. It reads the type that the semantic pass inferred (the
//! symbol table, as an editor hover does) and the type on the lowered
//! `IrLet`, and requires the same type. It then writes that type as
//! an annotation, and requires that the semantic pass accepts it.

#![expect(
    clippy::panic,
    reason = "a disagreement fails the test with both types in the message"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "the type of the inferred let is not public, so its method cannot be named"
)]

use formalang::{compile_to_ir, compile_with_analyzer};

const SUPPORT: &str = "\
pub struct Point { x: I32, y: I32 }
pub struct Wide { n: I64 }
pub struct Box<T> { value: T }
pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }
pub enum Maybe<T> { some(v: T), none }
pub fn id<T>(x: T) -> T { x }
pub fn wide() -> I64 { 5I64 }
";

/// The two types of `pub let probe = <expr>`, as each phase writes them.
fn types_of(expr: &str) -> (String, String) {
    let source = format!("{SUPPORT}pub let probe = {expr}\n");
    let (_, analyzer) = match compile_with_analyzer(&source) {
        Ok(ok) => ok,
        Err(e) => panic!("`{expr}` must pass the semantic pass: {e:?}"),
    };
    let semantic = analyzer
        .symbols()
        .lets
        .get("probe")
        .and_then(|l| l.inferred_type.as_ref())
        // `SemType` is not public, so the method cannot be named.
        .map_or_else(|| "<no type>".to_string(), |t| t.display());
    let module = match compile_to_ir(&source) {
        Ok(m) => m,
        Err(e) => panic!("`{expr}` passed the semantic pass but did not lower: {e:?}"),
    };
    let ir = module
        .get_let("probe")
        .map_or_else(|| "<no let>".to_string(), |l| l.ty.display_name(&module));
    (semantic, ir)
}

/// One spelling for one type. The two phases print a range
/// differently: `Range<I32>` and `I32..I32` are the same type.
fn normal(ty: &str) -> String {
    let t = ty.replace(' ', "");
    t.strip_prefix("Range<")
        .and_then(|r| r.strip_suffix('>'))
        .map_or_else(|| t.clone(), |inner| format!("{inner}..{inner}"))
}

fn agree(expr: &str) {
    let (semantic, ir) = types_of(expr);
    assert_eq!(
        normal(&semantic),
        normal(&ir),
        "`{expr}`: the semantic pass says {semantic}, the IR says {ir}"
    );
    // The semantic spelling is the source syntax; the IR spelling of a
    // range (`I32..I32`) is not.
    let annotated = format!("{SUPPORT}pub let probe: {semantic} = {expr}\n");
    if let Err(e) = compile_to_ir(&annotated) {
        panic!(
            "`{expr}`: both phases give it type {semantic}, but the compiler \
             rejects `let probe: {semantic} = {expr}`: {:?}",
            e.iter().map(ToString::to_string).collect::<Vec<_>>()
        );
    }
}

macro_rules! agreement {
    ($($name:ident: $expr:expr;)*) => {
        $(
            #[test]
            fn $name() {
                agree($expr);
            }
        )*
    };
}

agreement! {
    an_integer_literal: "1";
    an_i64_literal: "1I64";
    a_float_literal: "1.5";
    an_f32_literal: "1.5F32";
    a_string_literal: "\"s\"";
    a_boolean_literal: "true";
    an_i32_sum: "1 + 2";
    an_i64_sum: "1I64 + 2I64";
    an_i64_product_with_a_call: "wide() * 2I64";
    a_float_product: "2.0 * 3.0";
    a_negated_i64: "-1I64";
    a_negated_call: "-wide()";
    a_not: "!true";
    a_comparison_of_i64: "1I64 < 2I64";
    an_i32_array: "[1, 2]";
    an_i64_array: "[1I64, 2I64]";
    a_dictionary: "[\"a\": 1]";
    a_tuple: "(a: 1, b: \"x\")";
    a_tuple_field: "(a: 1, b: \"x\").b";
    a_range: "0..3";
    an_if_with_i64_branches: "if true { 1I64 } else { 2I64 }";
    an_if_without_else: "if true { 1 }";
    a_block_of_i64: "{\n    let a = 1I64\n    a + 1I64\n}";
    a_struct_field: "Point(x: 1, y: 2).x";
    an_i64_struct_field: "Wide(n: 3I64).n";
    a_generic_struct: "Box(value: 1I64)";
    a_generic_struct_field: "Box(value: 1I64).value";
    a_nested_generic_struct_field: "Box(value: Box(value: \"s\")).value.value";
    a_generic_call: "id(x: 1I64)";
    a_generic_call_on_a_string: "id(x: \"s\")";
    an_enum_instance: "Shape.circle(r: 1)";
    a_generic_enum_instance: "Maybe.some(v: 1I64)";
    an_array_index: "[1I64, 2I64][0]";
    a_dictionary_read: "[\"a\": 1I64][\"a\"]";
    a_string_length: "\"abc\".len()";
    an_array_length: "[1, 2].len()";
    a_closure: "(x: I64) -> x + 1I64";
    // A sequence cannot be a module-level `let`
    // (sequences/d_cf_a_module_sequence_is_rejected.fv), so the loop is
    // collected.
    a_loop: "(for i in 0..3 { i * 2 }).collect()";
    a_match_on_the_i32_field: "match Shape.rect(w: 1, h: 2I64) { .rect(w, h): w, _: 0 }";
    a_match_on_the_i64_field: "match Shape.rect(w: 1, h: 2I64) { .rect(w, h): h, _: 0I64 }";
    a_match_on_a_generic_payload: "match Maybe.some(v: 1I64) { .some(v): v, .none: 0I64 }";
    a_match_on_a_generic_string_payload: "match Maybe.some(v: \"s\") { .some(v): v, .none: \"\" }";
    a_collected_map_to_i64: "for i in 0..3 { i }.map(f: (x) -> 1I64).collect()";
    a_collected_map_to_string: "for i in 0..3 { i }.map(f: (x) -> \"s\").collect()";
    a_fold_to_i64: "for i in 0..3 { i }.fold(initial: 0I64, f: (a, x) -> a + 1I64)";
    a_fold_to_a_boolean: "for i in 0..3 { i }.fold(initial: false, f: (a, x) -> a || x == 2)";
    a_first_element: "for i in 0..3 { i * 2 }.first()";
    a_collected_dictionary: "for i in 0..3 { i }.collect(key: (x) -> x, value: (x) -> \"v\")";
    a_count: "for i in 0..3 { i }.filter(f: (x) -> x > 0).count()";
    an_optional_array_element: "[1I64][5]";
    an_if_let: "if let n = [1I64][0] { n } else { 0I64 }";
    a_string_slice: "\"abc\".slice(start: 0, end: 1)";
    a_byte: "\"abc\".byte_at(i: 0)";
    a_closure_call: "{\n    let f = (x: I64) -> x * 2I64\n    f(3I64)\n}";
    a_generic_identity_of_an_array: "id(x: [1I64])";
    a_generic_identity_of_a_closure: "id(x: (a: I64) -> a)";
    a_block_that_ends_in_an_if: "{\n    let a = 1I64\n    if a > 0I64 { a } else { 0I64 }\n}";
    a_generic_enum_of_a_struct: "Maybe.some(v: Point(x: 1, y: 2))";
    a_generic_struct_of_an_enum: "Box(value: Shape.empty)";
    a_generic_struct_of_an_optional: "Box(value: [1I64][0])";
    a_tuple_of_i64: "(a: 1I64, b: wide())";
    a_dictionary_of_i64_keys: "[1I64: \"a\"]";
}

// ---------------------------------------------------------------------------
// The two defects that formajit found, as verdicts
// ---------------------------------------------------------------------------

/// The IR types the arm `h` as `I64`; the function answers `I32`. The
/// same mistake without a `match` is `FunctionReturnTypeMismatch`.
#[test]
fn a_match_binding_of_i64_cannot_answer_an_i32_function() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn b(s: Shape) -> I32 { match s { .rect(w, h): h, _: 0 } }\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts an I64 match binding as the I32 answer of a function"
    );
}

/// `r` is an `I32`, and `I32 + Boolean` is refused everywhere else.
#[test]
fn a_match_binding_cannot_be_added_to_a_boolean() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn a(s: Shape) -> I32 { match s { .circle(r): r + true, _: 0 } }\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts `r + true` where `r` is an I32 match binding"
    );
}

/// A match binding bound to a `let` must get the type of its field, so
/// an annotation of another type is refused.
#[test]
fn a_match_binding_cannot_be_annotated_with_another_type() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn a(s: Shape) -> Boolean {\n\
                  match s { .circle(r): { let b: Boolean = r\n b }, _: false }\n}\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts `let b: Boolean = r` where `r` is an I32 match binding"
    );
}

/// The leading-dot form must name a variant that exists.
#[test]
fn a_dot_variant_that_does_not_exist_is_rejected() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn a() -> Shape { .nope }\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts `.nope` where `Shape` has no such variant"
    );
}

/// The leading-dot form in a `match` arm must name a variant too.
#[test]
fn a_match_arm_that_names_no_variant_is_rejected() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn a(s: Shape) -> I32 { match s { .nope: 1, _: 0 } }\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts the arm `.nope` where `Shape` has no such variant"
    );
}

/// A match arm may not bind more names than its variant has fields.
#[test]
fn a_match_arm_that_binds_too_many_names_is_rejected() {
    let source = "pub enum Shape { circle(r: I32), rect(w: I32, h: I64), empty }\n\
                  pub fn a(s: Shape) -> I32 { match s { .circle(r, q): r, _: 0 } }\n";
    assert!(
        compile_to_ir(source).is_err(),
        "the compiler accepts two bindings for the one field of `circle`"
    );
}
