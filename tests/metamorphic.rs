//! Metamorphic tests over the checked-in examples.
//!
//! A metamorphic test does not need to know the right answer. It
//! changes the input in a way that must not change the output, then
//! checks that the output did not change. That catches a class of bug
//! that example-based tests cannot: a result that depends on hash
//! order, on where a definition sits in the file, or on how many times
//! a pass has run.
//!
//! Every property here is a contract a backend relies on.

#![expect(
    clippy::panic,
    clippy::print_stdout,
    clippy::wildcard_enum_match_arm,
    reason = "tests assert their fixtures hold and walk untyped serde_json trees; \
              the cross-process check talks to its child over stdout"
)]

#[path = "common/mod.rs"]
mod common;

use common::Checked;

use std::collections::HashMap;
use std::path::PathBuf;

use formalang::ir::{
    ClosureConversionPass, ConstantFoldingPass, DeadCodeEliminationPass, DefunctionalisePass,
    IrModule, MonomorphisePass, ResolveReferencesPass,
};
use formalang::{compile_to_ir, IrPass, Pipeline};

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// Load every `examples/*.fv` file, sorted by name.
fn examples() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("fv") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if let Ok(source) = std::fs::read_to_string(&path) {
            out.push((name, source));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    // Every test in this file walks this corpus and skips what does not
    // apply. An empty or truncated corpus would make all of them pass
    // without asserting anything, so refuse to return one.
    assert!(
        out.len() >= 20,
        "the example corpus holds {} files, expected at least 20 — check \
         examples/ and CARGO_MANIFEST_DIR",
        out.len()
    );
    out
}

/// Serialise a module to JSON. The JSON is the comparison key: it is
/// also the format external consumers read, so two modules that
/// serialise identically are interchangeable to a backend.
fn json(module: &IrModule) -> String {
    serde_json::to_string(module).unwrap_or_else(|e| format!("<unserialisable: {e}>"))
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

/// Compiling the same source twice must produce the same IR.
///
/// The compiler uses `HashMap` throughout. `HashMap` iteration order
/// is randomised per process but stable within one, so a single-process
/// repeat is a weak check — [`compile_is_deterministic_across_processes`]
/// is the strong one. This test still catches order that leaks from a
/// counter or an accumulator that is not reset.
#[test]
fn compile_is_deterministic_within_a_process() {
    let mut checked = Checked::new("examples compiled twice", 20);
    for (name, source) in examples() {
        let Ok(first) = compile_to_ir(&source) else {
            continue;
        };
        let Ok(second) = compile_to_ir(&source) else {
            panic!("{name}: compiled once but not twice");
        };
        assert_eq!(
            json(&first),
            json(&second),
            "{name}: two compiles of the same source produced different IR"
        );
        checked.hit();
    }
}

/// The same check, across processes.
///
/// Rust randomises `HashMap` seeds per process. If any iteration order
/// reaches the IR — the order of impl methods, of monomorphised
/// clones, of synthesised closure-environment fields — the IR differs
/// between two runs of the same binary over the same input. A backend
/// would then emit a different artefact on every build.
///
/// The child is this same test binary, re-entered through an
/// environment variable.
#[test]
fn compile_is_deterministic_across_processes() {
    const VAR: &str = "FORMALANG_TEST_EMIT_IR";

    // Child mode: print the IR of the requested example and exit.
    if let Ok(target) = std::env::var(VAR) {
        for (name, source) in examples() {
            if name == target {
                if let Ok(module) = compile_to_ir(&source) {
                    print!("{}", json(&module));
                }
                return;
            }
        }
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => panic!("cannot locate the test binary: {e}"),
    };

    for (name, source) in examples() {
        let Ok(parent) = compile_to_ir(&source) else {
            continue;
        };
        let expected = json(&parent);

        let output = std::process::Command::new(&exe)
            .env(VAR, &name)
            .arg("compile_is_deterministic_across_processes")
            .arg("--exact")
            .arg("--nocapture")
            .output();
        let Ok(output) = output else {
            panic!("{name}: could not spawn the child process");
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        // The harness prints its own lines around ours; find the JSON.
        let Some(start) = stdout.find('{') else {
            panic!("{name}: the child produced no IR:\n{stdout}");
        };
        let Some(end) = stdout.rfind('}') else {
            panic!("{name}: the child produced no IR:\n{stdout}");
        };
        let actual = &stdout[start..=end];

        assert_eq!(
            expected, actual,
            "{name}: the IR differs between two processes, so it depends on \
             hash iteration order"
        );
    }
}

// ---------------------------------------------------------------------------
// Idempotence
// ---------------------------------------------------------------------------

/// Running the codegen pipeline over its own output must change
/// nothing.
///
/// A backend is free to run a pass twice — the `IrPass` docs say a
/// pass may be reused. A pass that is not idempotent quietly doubles
/// its own output on the second run.
#[test]
fn codegen_pipeline_is_idempotent() {
    let mut checked = Checked::new("examples through the pipeline twice", 20);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        let Ok(once) = Pipeline::for_codegen().run(module) else {
            continue;
        };
        let once_json = json(&once);
        let twice = match Pipeline::for_codegen().run(once) {
            Ok(m) => m,
            Err(errors) => panic!(
                "{name}: the pipeline accepted a module and then rejected its own \
                 output: {errors:?}"
            ),
        };
        assert_eq!(
            once_json,
            json(&twice),
            "{name}: the codegen pipeline is not idempotent"
        );
        checked.hit();
    }
}

/// Each built-in pass, run twice on its own output.
#[test]
fn each_pass_is_idempotent() {
    let mut checked = Checked::new("examples through every pass twice", 20);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };

        check_pass_idempotent(&name, "constant-folding", &module, || {
            Box::new(ConstantFoldingPass::new())
        });
        check_pass_idempotent(&name, "dead-code", &module, || {
            Box::new(DeadCodeEliminationPass::new())
        });
        check_pass_idempotent(&name, "resolve-refs", &module, || {
            Box::new(ResolveReferencesPass::new())
        });
        check_pass_idempotent(&name, "closure-conversion", &module, || {
            Box::new(ClosureConversionPass::new())
        });
        check_pass_idempotent(&name, "defunctionalise", &module, || {
            Box::new(DefunctionalisePass::new())
        });
        check_pass_idempotent(&name, "monomorphise", &module, || {
            Box::new(MonomorphisePass::default().with_imports(HashMap::new()))
        });
        checked.hit();
    }
}

fn check_pass_idempotent(
    example: &str,
    pass_name: &str,
    module: &IrModule,
    make: impl Fn() -> Box<dyn IrPass>,
) {
    let Ok(once) = make().run(module.clone()) else {
        return;
    };
    let once_json = json(&once);
    let twice = match make().run(once) {
        Ok(m) => m,
        Err(errors) => panic!(
            "{example}: pass {pass_name} accepted a module and then rejected its \
             own output: {errors:?}"
        ),
    };
    assert_eq!(
        once_json,
        json(&twice),
        "{example}: pass {pass_name} is not idempotent"
    );
}

/// `rebuild_indices` must be a fixpoint: calling it on a module that
/// already has its indices does not change the module.
#[test]
fn rebuild_indices_is_a_fixpoint() {
    let mut checked = Checked::new("examples reindexed", 20);
    for (name, source) in examples() {
        let Ok(mut module) = compile_to_ir(&source) else {
            continue;
        };
        let before = json(&module);
        module.rebuild_indices();
        let after_one = json(&module);
        module.rebuild_indices();
        let after_two = json(&module);

        assert_eq!(
            before, after_one,
            "{name}: rebuild_indices changed a module that was already indexed"
        );
        assert_eq!(
            after_one, after_two,
            "{name}: rebuild_indices is not a fixpoint"
        );
        checked.hit();
    }
}

// ---------------------------------------------------------------------------
// Source transformations that must not change the IR
// ---------------------------------------------------------------------------

/// Adding comments must not change the compiled IR, except for spans.
///
/// A comment shifts every byte offset after it. If a comment changes
/// anything beyond spans, some phase is reading the source text where
/// it should be reading the AST.
#[test]
fn comments_do_not_change_the_ir() {
    let mut checked = Checked::new("examples compiled with comments added", 20);
    for (name, source) in examples() {
        let Ok(plain) = compile_to_ir(&source) else {
            continue;
        };

        // A comment line before every line that starts a definition.
        let mut commented = String::new();
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("pub ") || trimmed.starts_with("fn ") {
                commented.push_str("// inserted by the metamorphic test\n");
            }
            commented.push_str(line);
            commented.push('\n');
        }

        let annotated = match compile_to_ir(&commented) {
            Ok(m) => m,
            Err(errors) => panic!("{name}: adding comments broke the compile: {errors:?}"),
        };

        assert_eq!(
            canonical(&plain),
            canonical(&annotated),
            "{name}: adding comments changed the IR beyond its spans and ids"
        );
        checked.hit();
    }
}

/// Trailing whitespace on every line must not change the IR.
#[test]
fn trailing_whitespace_does_not_change_the_ir() {
    let mut checked = Checked::new("examples compiled with padding", 20);
    for (name, source) in examples() {
        let Ok(plain) = compile_to_ir(&source) else {
            continue;
        };
        let padded: String = source.lines().fold(String::new(), |mut acc, l| {
            acc.push_str(l);
            acc.push_str("   \n");
            acc
        });
        let padded_ir = match compile_to_ir(&padded) {
            Ok(m) => m,
            Err(errors) => panic!("{name}: trailing whitespace broke the compile: {errors:?}"),
        };
        assert_eq!(
            canonical(&plain),
            canonical(&padded_ir),
            "{name}: trailing whitespace changed the IR beyond its spans and ids"
        );
        checked.hit();
    }
}

/// A canonical view of a module: no spans, no numeric ids, and every
/// top-level vector sorted by name.
///
/// Dropping spans lets two compiles of the same program at different
/// byte offsets compare equal. Dropping ids and sorting is what makes
/// the comparison independent of the order the lowerer happened to
/// register definitions in — see FL-4 in `tests/known_issues.rs`.
/// Everything that survives — names, visibility, types, field names,
/// operators, literals, the shape of every body — is what a source
/// transformation must not touch.
fn canonical(module: &IrModule) -> String {
    let mut value: serde_json::Value = match serde_json::to_value(module) {
        Ok(v) => v,
        Err(e) => return format!("<unserialisable: {e}>"),
    };
    canonicalise(&mut value);
    if let serde_json::Value::Object(map) = &mut value {
        for vector in ["structs", "traits", "enums", "functions", "impls", "lets"] {
            if let Some(serde_json::Value::Array(items)) = map.get_mut(vector) {
                items.sort_by_key(|item| {
                    item.get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string()
                });
            }
        }
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Keys whose value is an index into one of the module's vectors.
const ID_KEYS: &[&str] = &[
    "struct_id",
    "enum_id",
    "trait_id",
    "impl_id",
    "function_id",
    "variant_idx",
    "field_idx",
    "binding_id",
    "id",
    "Struct",
    "Trait",
    "Enum",
];

fn canonicalise(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("span");
            for key in ID_KEYS {
                if let Some(entry) = map.get_mut(*key) {
                    if entry.is_number() {
                        *entry = serde_json::Value::Null;
                    }
                }
            }
            // `IrModuleNode` holds bare id lists, e.g.
            // `"structs": [0, 3]`. Null those too: they index the same
            // vectors and carry the same order dependence.
            for entry in map.values_mut() {
                if let serde_json::Value::Array(items) = entry {
                    if !items.is_empty() && items.iter().all(serde_json::Value::is_number) {
                        for item in items {
                            *item = serde_json::Value::Null;
                        }
                    }
                }
            }
            for v in map.values_mut() {
                canonicalise(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                canonicalise(v);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Structural invariants of a lowered module
// ---------------------------------------------------------------------------

/// Every id inside a module must point at something that exists.
///
/// A dangling id is the worst kind of IR bug: the frontend reports
/// success and the backend indexes out of bounds, or worse, emits code
/// for the wrong definition.
#[test]
fn every_id_in_a_lowered_module_is_in_range() {
    let mut checked = Checked::new("examples id-checked", 20);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        check_ids(&name, "after lowering", &module);

        let Ok(after) = Pipeline::for_codegen().run(module) else {
            continue;
        };
        check_ids(&name, "after the codegen pipeline", &after);
        checked.hit();
    }
}

fn check_ids(example: &str, stage: &str, module: &IrModule) {
    let value = match serde_json::to_value(module) {
        Ok(v) => v,
        Err(e) => panic!("{example}: module will not serialise: {e}"),
    };

    let counts = [
        ("Struct", module.structs.len()),
        ("Trait", module.traits.len()),
        ("Enum", module.enums.len()),
        ("Function", module.functions.len()),
    ];

    // `ResolvedType` and `ReferenceTarget` serialise their ids under
    // the variant name, e.g. `{"Struct": {"id": 3, ...}}`. Walk the
    // tree looking for those shapes.
    let mut violations = Vec::new();
    walk_ids(&value, &counts, &mut violations);
    assert!(
        violations.is_empty(),
        "{example} {stage}: {} id(s) point past the end of their table: {:?}",
        violations.len(),
        violations
    );

    // The file id on every span must index the file table, except id 0
    // which is reserved for synthetic nodes.
    let table = module.file_table.len();
    let mut bad_files = Vec::new();
    walk_file_ids(&value, table, &mut bad_files);
    assert!(
        bad_files.is_empty(),
        "{example} {stage}: {} span(s) name a file id outside the file table \
         (table has {table} entries): {:?}",
        bad_files.len(),
        bad_files
    );
}

fn walk_ids(value: &serde_json::Value, counts: &[(&str, usize)], out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if let Some((_, limit)) = counts.iter().find(|(k, _)| k == key) {
                    if let Some(id) = child.get("id").and_then(serde_json::Value::as_u64) {
                        if usize::try_from(id).unwrap_or(usize::MAX) >= *limit {
                            out.push(format!("{key} id {id} >= {limit}"));
                        }
                    }
                }
                walk_ids(child, counts, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                walk_ids(v, counts, out);
            }
        }
        _ => {}
    }
}

fn walk_file_ids(value: &serde_json::Value, table: usize, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(file) = map.get("file").and_then(serde_json::Value::as_u64) {
                let file = usize::try_from(file).unwrap_or(usize::MAX);
                if file != 0 && file > table {
                    out.push(format!("file id {file} > {table}"));
                }
            }
            for child in map.values() {
                walk_file_ids(child, table, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                walk_file_ids(v, table, out);
            }
        }
        _ => {}
    }
}

/// Dead-code elimination keeps every public type, whether or not
/// anything inside the module names it.
///
/// A `pub` definition is the module's contract with whatever links
/// against it. The pass never removes functions, public or not, but it
/// used to remove an unreferenced `pub struct` — so a library came out
/// of the pipeline with its API stripped and the backend emitted
/// nothing a consumer could use.
#[test]
fn dead_code_keeps_every_public_type() {
    let cases: &[(&str, &str, &str)] = &[
        ("struct", "pub struct Config {\n    a: I32\n}\n", "Config"),
        ("enum", "pub enum E {\n    one,\n    two\n}\n", "E"),
        ("trait", "pub trait Named {\n    name: String\n}\n", "Named"),
    ];

    for (kind, source, name) in cases {
        let Ok(module) = compile_to_ir(source) else {
            panic!("{kind}: the fixture must compile");
        };
        let Ok(after) = DeadCodeEliminationPass::new().run(module) else {
            panic!("{kind}: the pass must accept the fixture");
        };
        let present = after.structs.iter().any(|s| s.name == *name)
            || after.enums.iter().any(|e| e.name == *name)
            || after.traits.iter().any(|t| t.name == *name);
        assert!(
            present,
            "the public {kind} {name} was removed although nothing may remove a \
             public definition"
        );
    }
}

/// A private definition nothing references is still removed.
#[test]
fn dead_code_removes_a_private_definition_nothing_names() {
    let source = "struct Hidden {\n    a: I32\n}\n";
    let Ok(module) = compile_to_ir(source) else {
        panic!("the fixture must compile");
    };
    let Ok(after) = DeadCodeEliminationPass::new().run(module) else {
        panic!("the pass must accept the fixture");
    };
    assert!(
        !after.structs.iter().any(|s| s.name == "Hidden"),
        "a private struct nothing references should have been removed"
    );
}

/// A private definition a public function's body uses survives.
///
/// A public signature cannot name a private type — that is rejected as
/// `PrivateTypeInPublic`, because a caller could not write the type
/// down. So the way a private type stays reachable is from inside a
/// public function, and dead-code elimination has to follow it there.
#[test]
fn dead_code_keeps_a_private_type_a_public_body_uses() {
    let source =
        "struct Hidden {\n    a: I32\n}\n\npub fn take() -> I32 {\n    Hidden(a: 1).a\n}\n";
    let Ok(module) = compile_to_ir(source) else {
        panic!("the fixture must compile");
    };
    let Ok(after) = DeadCodeEliminationPass::new().run(module) else {
        panic!("the pass must accept the fixture");
    };
    assert!(
        after.structs.iter().any(|s| s.name == "Hidden"),
        "a private struct named by a public signature should survive"
    );
}

/// Dead-code elimination must never remove a public definition.
///
/// A public definition is the module's contract with its callers;
/// nothing inside the module references it, so a reachability pass
/// that does not seed itself from the public surface deletes the whole
/// library.
#[test]
fn dead_code_keeps_the_public_surface() {
    let mut checked = Checked::new("examples through dead-code elimination", 20);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        let public_before: Vec<String> = module
            .functions
            .iter()
            .filter(|f| f.visibility == formalang::ast::Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        let Ok(after) = DeadCodeEliminationPass::new().run(module) else {
            continue;
        };
        let public_after: Vec<String> = after
            .functions
            .iter()
            .filter(|f| f.visibility == formalang::ast::Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        for f in &public_before {
            assert!(
                public_after.contains(f),
                "{name}: dead-code elimination removed the public function {f}"
            );
        }
        checked.hit();
    }
}

// ---------------------------------------------------------------------------
// Source positions survive the passes
// ---------------------------------------------------------------------------

/// A constructor for one of the built-in passes.
type MakePass = fn() -> Box<dyn IrPass>;

/// Count the IR nodes whose span still points at real source.
fn nodes_with_a_real_span(module: &IrModule) -> usize {
    let value = serde_json::to_value(module).unwrap_or_default();
    let mut count = 0;
    count_real_spans(&value, &mut count);
    count
}

fn count_real_spans(value: &serde_json::Value, count: &mut usize) {
    match value {
        serde_json::Value::Object(map) => {
            let end = map
                .get("span")
                .and_then(|s| s.get("span"))
                .and_then(|s| s.get("end"))
                .and_then(|e| e.get("offset"))
                .and_then(serde_json::Value::as_u64);
            if end.is_some_and(|offset| offset > 0) {
                *count = count.saturating_add(1);
            }
            for child in map.values() {
                count_real_spans(child, count);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                count_real_spans(v, count);
            }
        }
        _ => {}
    }
}

/// No pass may throw away the source positions of the nodes it walks
/// past.
///
/// `IrModule.file_table` and `IrSpan` exist so a backend can emit
/// DWARF, a source map or a line-number table. A pass that rebuilds an
/// expression with a default span silently deletes that data, and the
/// frontend still reports success.
///
/// `ClosureConversionPass` used to do exactly this, in every function
/// body, including bodies with no closure in them. On
/// `20_match_advanced.fv` it took 107 real spans down to zero.
#[test]
fn no_pass_throws_away_source_positions() {
    let mut checked = Checked::new("examples span-checked per pass", 15);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        let before = nodes_with_a_real_span(&module);
        if before == 0 {
            continue;
        }

        let passes: Vec<(&str, MakePass)> = vec![
            ("constant-folding", || Box::new(ConstantFoldingPass::new())),
            ("resolve-refs", || Box::new(ResolveReferencesPass::new())),
            ("closure-conversion", || {
                Box::new(ClosureConversionPass::new())
            }),
            ("monomorphise", || Box::new(MonomorphisePass::default())),
            ("dead-code", || Box::new(DeadCodeEliminationPass::new())),
            ("defunctionalise", || Box::new(DefunctionalisePass::new())),
        ];

        for (pass_name, make) in passes {
            let Ok(after_module) = make().run(module.clone()) else {
                continue;
            };
            let after = nodes_with_a_real_span(&after_module);
            // A pass may add synthetic nodes and dead-code elimination
            // may remove real ones, so the count moves either way. What
            // it must not do is collapse: compare doubled counts rather
            // than halving one, so no division is involved.
            assert!(
                after.saturating_mul(2) >= before,
                "{name}: pass {pass_name} took the node count with a real span \
                 from {before} to {after}"
            );
            checked.hit();
        }
    }
}

/// The codegen pipeline as a whole must keep most positions.
#[test]
fn the_codegen_pipeline_keeps_source_positions() {
    let mut checked = Checked::new("examples span-checked after the pipeline", 15);
    for (name, source) in examples() {
        let Ok(module) = compile_to_ir(&source) else {
            continue;
        };
        let before = nodes_with_a_real_span(&module);
        if before == 0 {
            continue;
        }
        let Ok(after_module) = Pipeline::for_codegen().run(module) else {
            continue;
        };
        let after = nodes_with_a_real_span(&after_module);
        assert!(
            after > 0,
            "{name}: the codegen pipeline left no node with a real span, so a \
             backend can emit no debug information at all"
        );
        checked.hit();
    }
}

/// Dead-code elimination reaches a fixpoint in one run.
///
/// Removing a definition can make another unreachable: a trait whose
/// method signature mentions `Dictionary` is what keeps `Dictionary`
/// alive, so dropping the trait has to be followed by another look at
/// `Dictionary`. A single round left that second definition behind,
/// and a second `run` then removed it — which is how the fuzzer found
/// it, as a pipeline that was not idempotent.
#[test]
fn dead_code_elimination_reaches_a_fixpoint() {
    let sources = [
        // The shape the fuzzer shrank to: the only reference to
        // `Dictionary` is the return type of a trait method, and the
        // trait itself is unreachable.
        "pub trait Named {\n    fn f(self) -> [I32: I32]\n}\n",
        "pub trait A {\n    fn f(self) -> [String]\n}\n",
        "pub struct Unused {\n    a: I32\n}\n\npub trait T {\n    fn f(self) -> Unused\n}\n",
    ];

    for source in sources {
        let Ok(module) = compile_to_ir(source) else {
            continue;
        };
        let Ok(once) = DeadCodeEliminationPass::new().run(module) else {
            continue;
        };
        let once_json = json(&once);
        let Ok(twice) = DeadCodeEliminationPass::new().run(once) else {
            panic!("the pass must accept its own output for:\n{source}");
        };
        assert_eq!(
            once_json,
            json(&twice),
            "a second run removed more than the first for:\n{source}"
        );
    }
}
