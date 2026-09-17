//! Phase-by-phase cost of the compiler frontend.
//!
//! Each group isolates one phase so a regression points at a phase
//! instead of at "compilation". Run with `cargo bench --bench frontend`.
//!
//! The inputs are the checked-in examples, which is what a user
//! actually compiles. `EXAMPLES` is embedded at build time so the
//! benchmark does no I/O in the measured region.

use std::collections::HashMap;
use std::path::PathBuf;

use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use formalang::{
    compile_to_ir, compile_with_analyzer_and_resolver, parse_only, IrPass, Lexer, Pipeline,
};

fn main() {
    divan::main();
}

/// A resolver that serves nothing. Keeps the benchmark off the
/// filesystem, which would otherwise dominate the measurement.
struct NoResolver;

impl ModuleResolver for NoResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        Err(ModuleError::NotFound {
            path: path.to_vec(),
            searched_paths: Vec::new(),
        })
    }
}

/// The example programs this benchmark runs over. Four shapes,
/// picked to span the range: a small generic struct, a trait-dispatch
/// program, the sequence combinators, and a wide `match`.
///
/// The benchmarks take the *name* as the argument and look the source
/// up here, so the result table stays readable.
const EXAMPLES: &[&str] = &[
    "01_generics_box",
    "08_trait_dispatch",
    "09_arrays_for_dict",
    "20_match_advanced",
];

fn source_of(name: &str) -> &'static str {
    match name {
        "08_trait_dispatch" => include_str!("../examples/08_trait_dispatch.fv"),
        "09_arrays_for_dict" => include_str!("../examples/09_arrays_for_dict.fv"),
        "20_match_advanced" => include_str!("../examples/20_match_advanced.fv"),
        _ => include_str!("../examples/01_generics_box.fv"),
    }
}

/// The compiler-shipped prelude. Every entry point parses it once per
/// call, so its cost is the floor under every other measurement here.
const PRELUDE: &str = include_str!("../src/prelude.fv");

// ---------------------------------------------------------------------------
// Phases
// ---------------------------------------------------------------------------

#[divan::bench(args = EXAMPLES)]
fn lex(bencher: divan::Bencher, name: &str) {
    let source = source_of(name);
    bencher.bench(|| Lexer::tokenize_all_with_errors(divan::black_box(source)));
}

#[divan::bench(args = EXAMPLES)]
fn parse(bencher: divan::Bencher, name: &str) {
    let source = source_of(name);
    bencher.bench(|| parse_only(divan::black_box(source)));
}

/// Lex plus parse plus semantic analysis, without IR lowering.
#[divan::bench(args = EXAMPLES)]
fn semantic(bencher: divan::Bencher, name: &str) {
    let source = source_of(name);
    bencher.bench(|| compile_with_analyzer_and_resolver(divan::black_box(source), NoResolver));
}

/// The whole frontend, which is what `compile_to_ir` costs a caller.
#[divan::bench(args = EXAMPLES)]
fn compile(bencher: divan::Bencher, name: &str) {
    let source = source_of(name);
    bencher.bench(|| compile_to_ir(divan::black_box(source)));
}

/// The prelude alone. `compile_to_ir` re-lexes and re-parses this on
/// every call; comparing it against `compile` shows how much of a
/// small compile is fixed overhead.
#[divan::bench]
fn prelude_parse(bencher: divan::Bencher) {
    bencher.bench(|| parse_only(divan::black_box(PRELUDE)));
}

/// The floor: the smallest program the compiler accepts. Anything
/// this costs is paid by every caller, whatever the input.
#[divan::bench]
fn compile_minimal(bencher: divan::Bencher) {
    bencher.bench(|| compile_to_ir(divan::black_box("pub struct A { a: I32 }")));
}

// ---------------------------------------------------------------------------
// IR passes
//
// Each pass is measured on an already-lowered module, so the number is
// the pass's own cost and not the frontend's.
// ---------------------------------------------------------------------------

fn lowered(source: &str) -> IrModule {
    compile_to_ir(source).unwrap_or_default()
}

macro_rules! bench_pass {
    ($name:ident, $pass:expr) => {
        #[divan::bench(args = EXAMPLES)]
        fn $name(bencher: divan::Bencher, name: &str) {
            let module = lowered(source_of(name));
            bencher
                .with_inputs(|| module.clone())
                .bench_values(|m| $pass.run(m));
        }
    };
}

bench_pass!(pass_constant_folding, ConstantFoldingPass::new());
bench_pass!(pass_dead_code, DeadCodeEliminationPass::new());
bench_pass!(pass_resolve_refs, ResolveReferencesPass::new());
bench_pass!(pass_closure_conv, ClosureConversionPass::new());
bench_pass!(pass_defunctionalise, DefunctionalisePass::new());
bench_pass!(
    pass_monomorphise,
    MonomorphisePass::default().with_imports(HashMap::new())
);

/// The pipeline a backend actually runs.
#[divan::bench(args = EXAMPLES)]
fn pipeline_for_codegen(bencher: divan::Bencher, name: &str) {
    let module = lowered(source_of(name));
    bencher
        .with_inputs(|| module.clone())
        .bench_values(|m| Pipeline::for_codegen().run(m));
}

// ---------------------------------------------------------------------------
// Serialisation
//
// The IR JSON is the published artefact; backends in other languages
// read it, so its encode cost is part of the product.
// ---------------------------------------------------------------------------

#[divan::bench(args = EXAMPLES)]
fn ir_to_json(bencher: divan::Bencher, name: &str) {
    let module = lowered(source_of(name));
    bencher.bench(|| serde_json::to_string(divan::black_box(&module)));
}

#[divan::bench(args = EXAMPLES)]
fn ir_from_json(bencher: divan::Bencher, name: &str) {
    let module = lowered(source_of(name));
    let json = serde_json::to_string(&module).unwrap_or_default();
    bencher.bench(|| serde_json::from_str::<IrModule>(divan::black_box(&json)));
}
