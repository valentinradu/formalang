//! How the frontend scales with program size.
//!
//! Each generator grows one dimension of a program and holds the rest
//! fixed. Read the results as ratios, not as absolute times: when the
//! input grows 4x, a linear phase grows about 4x. A phase that grows
//! 16x is quadratic in that dimension, and that is the finding.
//!
//! Run with `cargo bench --bench scaling`.

#![expect(
    clippy::format_push_string,
    clippy::arithmetic_side_effects,
    reason = "the input generators build source text with format! and index \
              arithmetic; they are not library code"
)]

use formalang::location::offset_to_location;
use formalang::{compile_to_ir, parse_only, Lexer, Pipeline};

fn main() {
    divan::main();
}

/// Input sizes. Each step is 4x the previous one, so a quadratic
/// phase shows up as a 16x jump between neighbours.
const SIZES: [usize; 4] = [8, 32, 128, 512];

/// `N` structs, each with two fields.
fn many_structs(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!(
            "pub struct S{i} {{\n    a: I32,\n    b: String\n}}\n\n"
        ));
    }
    s
}

/// `N` functions, each calling the one before it. Exercises the
/// symbol table and the call-resolution path.
fn many_functions(n: usize) -> String {
    let mut s = String::from("pub fn f0(x: I32) -> I32 {\n    x + 1\n}\n\n");
    for i in 1..n {
        let prev = i - 1;
        s.push_str(&format!(
            "pub fn f{i}(x: I32) -> I32 {{\n    f{prev}(x: x) + 1\n}}\n\n"
        ));
    }
    s
}

/// One enum with `N` variants, and one function matching all of them.
fn wide_match(n: usize) -> String {
    let mut s = String::from("pub enum E {\n");
    for i in 0..n {
        if i > 0 {
            s.push_str(",\n");
        }
        s.push_str(&format!("    v{i}"));
    }
    s.push_str("\n}\n\npub fn pick(e: E) -> I32 {\n    match e {\n");
    for i in 0..n {
        if i > 0 {
            s.push_str(",\n");
        }
        s.push_str(&format!("        .v{i}: {i}"));
    }
    s.push_str("\n    }\n}\n");
    s
}

/// One function whose body is a left-nested chain of `N` additions.
/// This is the parser's precedence climb and the folder's walk.
fn long_expression(n: usize) -> String {
    let mut body = String::from("0");
    for i in 0..n {
        body.push_str(&format!(" + {i}"));
    }
    format!("pub fn chain() -> I32 {{\n    {body}\n}}\n")
}

/// One function whose body is `N` nested `if` expressions. Grows the
/// recursion depth of every recursive walk in the compiler.
fn deep_nesting(n: usize) -> String {
    let mut body = String::from("0");
    for i in 0..n {
        body = format!("if x > {i} {{ {body} }} else {{ {i} }}");
    }
    format!("pub fn nested(x: I32) -> I32 {{\n    {body}\n}}\n")
}

/// One function with `N` sequential `let` bindings, each reading the
/// one before it. Grows the scope that every name lookup searches.
fn many_locals(n: usize) -> String {
    let mut s = String::from("pub fn locals() -> I32 {\n    let v0 = 1\n");
    for i in 1..n {
        let prev = i - 1;
        s.push_str(&format!("    let v{i} = v{prev} + 1\n"));
    }
    s.push_str(&format!("    v{}\n}}\n", n - 1));
    s
}

/// `N` closures in one function, each capturing a distinct local.
/// This is the closure-conversion pass's input.
fn many_closures(n: usize) -> String {
    let mut s = String::from("pub fn closures(base: I32) -> I32 {\n    let acc0 = base\n");
    for i in 0..n {
        s.push_str(&format!("    let c{i} = (v: I32) -> v + base + {i}\n"));
    }
    let mut tail = String::from("acc0");
    for i in 0..n {
        tail = format!("{tail} + c{i}(1)");
    }
    s.push_str(&format!("    {tail}\n}}\n"));
    s
}

/// `N` instantiations of one generic struct. This is the
/// monomorphiser's input.
fn many_generic_uses(n: usize) -> String {
    let mut s = String::from("pub struct Box<T> {\n    value: T\n}\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "pub fn take{i}() -> I32 {{\n    let b = Box<I32>(value: {i})\n    b.value\n}}\n\n"
        ));
    }
    s
}

type Generator = fn(usize) -> String;

const SHAPES: &[(&str, Generator)] = &[
    ("many_structs", many_structs),
    ("many_functions", many_functions),
    ("wide_match", wide_match),
    ("long_expression", long_expression),
    ("many_locals", many_locals),
    ("many_closures", many_closures),
    ("many_generic_uses", many_generic_uses),
];

/// Lexing alone, over one shape, at four sizes.
///
/// This is the clearest quadratic detector in the suite: the lexer
/// touches every byte once, so its cost must be linear in the source
/// length. A 16x jump between neighbouring sizes says the lexer does
/// per-token work that is proportional to the offset.
#[divan::bench(args = SIZES)]
fn lex_scaling(bencher: divan::Bencher, n: usize) {
    let source = many_structs(n);
    bencher.bench(|| Lexer::tokenize_all_with_errors(divan::black_box(&source)));
}

/// Parsing alone, over the same shape and sizes, so parsing can be
/// compared against lexing at every size.
#[divan::bench(args = SIZES)]
fn parse_scaling(bencher: divan::Bencher, n: usize) {
    let source = many_structs(n);
    bencher.bench(|| parse_only(divan::black_box(&source)));
}

/// Byte offset to line/column, the conversion the lexer runs twice per
/// token. Measured at the *end* of the source, which is the worst case
/// for a scan that starts at the beginning.
#[divan::bench(args = SIZES)]
fn offset_to_location_at_end(bencher: divan::Bencher, n: usize) {
    let source = many_structs(n);
    let offset = source.len();
    bencher.bench(|| offset_to_location(divan::black_box(offset), divan::black_box(&source)));
}

/// Parsing alone, per shape and size.
#[divan::bench(args = SIZES)]
fn parse_shapes(bencher: divan::Bencher, n: usize) {
    let sources: Vec<String> = SHAPES.iter().map(|(_, gen)| gen(n)).collect();
    bencher.bench(|| {
        for source in &sources {
            let _ = parse_only(divan::black_box(source));
        }
    });
}

/// The whole frontend, one benchmark per shape so a regression names
/// the shape that caused it.
macro_rules! bench_shape {
    ($name:ident, $gen:expr) => {
        #[divan::bench(args = SIZES)]
        fn $name(bencher: divan::Bencher, n: usize) {
            let source = $gen(n);
            bencher.bench(|| compile_to_ir(divan::black_box(&source)));
        }
    };
}

bench_shape!(compile_many_structs, many_structs);
bench_shape!(compile_many_functions, many_functions);
bench_shape!(compile_wide_match, wide_match);
bench_shape!(compile_long_expression, long_expression);
bench_shape!(compile_many_locals, many_locals);
bench_shape!(compile_many_closures, many_closures);
bench_shape!(compile_many_generic_uses, many_generic_uses);

/// Nesting depth is capped lower than the other dimensions: the
/// recursive walks use the native stack, so a deep input either runs
/// or overflows it.
#[divan::bench(args = [4, 16, 64])]
fn compile_deep_nesting(bencher: divan::Bencher, n: usize) {
    let source = deep_nesting(n);
    bencher.bench(|| compile_to_ir(divan::black_box(&source)));
}

/// The codegen pipeline over each shape, separated from lowering.
#[divan::bench(args = SIZES)]
fn pipeline_shapes(bencher: divan::Bencher, n: usize) {
    let modules: Vec<_> = SHAPES
        .iter()
        .filter_map(|(_, gen)| compile_to_ir(&gen(n)).ok())
        .collect();
    bencher.with_inputs(|| modules.clone()).bench_values(|ms| {
        for m in ms {
            let _ = Pipeline::for_codegen().run(m);
        }
    });
}
