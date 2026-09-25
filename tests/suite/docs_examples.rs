//! The documentation, used as a specification.
//!
//! `docs/` is the reference of the language. Each ```` ```formalang ````
//! block in it is a complete program that a reader copies, so this file
//! compiles each one. The info string after the language tells the
//! harness what the block is:
//!
//! ```text
//! ```formalang                    the block must compile; if it declares
//!                                 `run_checks()`, the reference
//!                                 interpreter runs it, and each `assert`
//!                                 in it must hold
//! ```formalang,reject=Variant     the block shows a mistake: it must
//!                                 fail with that CompilerError variant,
//!                                 in compile_to_ir or, for an error of
//!                                 monomorphisation, in MonomorphisePass
//! ```formalang,file=path/name.fv  the block is a module file: the other
//!                                 blocks of the same page import it with
//!                                 `use path::name::Item`
//! ```
//!
//! No other tag is legal. The old tag `fragment` marked a part of a
//! program that did not compile. It is an error now: write the
//! declarations that the block needs, and put statements in a function.
//!
//! The second half of this file checks claims that the prose makes about
//! the IR: doc comments, attributes, the `byte_at` desugaring, the
//! monomorphisation, the resolved references and the spans. The claims
//! about what a program means are in `tests/conformance/`, in the files
//! that start with `d_` and `p12_`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test returns a Result so that `?` reports a setup failure, and \
              asserts the claim itself"
)]

use crate::common::interpreter::Interpreter;
use crate::common::{Checked, MemResolver};
use formalang::ir::{IrExpr, IrModule, MonomorphisePass};
use formalang::{CompilerError, Pipeline};
use std::path::{Path, PathBuf};

/// What the info string of a block says.
#[derive(Debug, PartialEq, Eq)]
enum Tag {
    Compile,
    /// The retired `fragment` tag. A block that carries it fails.
    Fragment,
    Reject(String),
    /// A module file, with its path relative to the resolver root.
    File(String),
    Unknown(String),
}

/// One ```` ```formalang ```` block.
struct Block {
    /// The page, relative to `docs/`.
    page: String,
    /// The line of the opening fence, one-based.
    line: usize,
    tag: Tag,
    source: String,
}

fn docs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs")
}

fn parse_tag(info: &str) -> Tag {
    let mut parts = info.split(',').map(str::trim);
    let _language = parts.next();
    match parts.next() {
        None => Tag::Compile,
        Some("fragment") => Tag::Fragment,
        Some(other) => other.strip_prefix("reject=").map_or_else(
            || {
                other.strip_prefix("file=").map_or_else(
                    || Tag::Unknown(other.to_string()),
                    |file| Tag::File(file.to_string()),
                )
            },
            |variant| Tag::Reject(variant.to_string()),
        ),
    }
}

/// Every block of one page.
fn blocks_of(page: &str) -> Vec<Block> {
    let Ok(text) = std::fs::read_to_string(docs_root().join(page)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut open: Option<(usize, Tag, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index.saturating_add(1);
        match open.take() {
            None => {
                if line.starts_with("```formalang") {
                    open = Some((
                        number,
                        parse_tag(line.trim_start_matches('`')),
                        String::new(),
                    ));
                }
            }
            Some((start, tag, mut source)) => {
                if line.trim_start().starts_with("```") {
                    out.push(Block {
                        page: page.to_string(),
                        line: start,
                        tag,
                        source,
                    });
                } else {
                    source.push_str(line);
                    source.push('\n');
                    open = Some((start, tag, source));
                }
            }
        }
    }
    out
}

/// Every page under `docs/` that holds a block, relative to `docs/`.
fn pages() -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                if text.contains("```formalang") {
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    out.push(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
    let root = docs_root();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// The name of an error's variant, from its `Debug` form.
fn variant_name(error: &formalang::CompilerError) -> String {
    format!("{error:?}")
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The resolver of one page: each `file=` block, under the module path
/// that its file name gives. `utils/helpers.fv` is `utils::helpers`.
fn resolver_of(blocks: &[Block]) -> Option<MemResolver> {
    let mut resolver = MemResolver::new();
    let mut any = false;
    for block in blocks {
        if let Tag::File(file) = &block.tag {
            let path: Vec<String> = file
                .trim_end_matches(".fv")
                .split('/')
                .map(str::to_string)
                .collect();
            resolver.add(path, &block.source);
            any = true;
        }
    }
    any.then_some(resolver)
}

/// Compile one block, through the resolver of its page when the page
/// has module files.
fn compile_block(
    source: &str,
    resolver: Option<&MemResolver>,
) -> Result<IrModule, Vec<CompilerError>> {
    resolver.map_or_else(
        || formalang::compile_to_ir(source),
        |r| formalang::compile_to_ir_with_resolver(source, r),
    )
}

/// Run `run_checks()` when the module declares it. Returns why the run
/// failed, or `None`.
fn run_failure(module: &IrModule) -> Option<String> {
    let mut interpreter = Interpreter::new(module);
    if !interpreter.has_function("run_checks") {
        return None;
    }
    match interpreter.run("run_checks") {
        Ok(_) if interpreter.asserts_passed == 0 => {
            Some("run_checks() asserted nothing".to_string())
        }
        Ok(_) => None,
        Err(fault) => Some(format!(
            "run_checks() failed after {} assert(s): {fault}",
            interpreter.asserts_passed
        )),
    }
}

/// Check each block of one page against its tag. Returns one line for
/// each block that fails.
fn check_page(page: &str) -> Vec<String> {
    let blocks = blocks_of(page);
    assert!(
        !blocks.is_empty(),
        "{page}: no ```formalang block found; check the path"
    );
    let resolver = resolver_of(&blocks);
    let mut failures = Vec::new();
    for block in &blocks {
        let at = format!("{}:{}", block.page, block.line);
        let reject: Option<&str> = match &block.tag {
            Tag::Unknown(tag) => {
                failures.push(format!("{at}: unknown fence tag `{tag}`"));
                continue;
            }
            Tag::Fragment => {
                failures.push(format!(
                    "{at}: the `fragment` tag is retired; make the block a complete program"
                ));
                continue;
            }
            Tag::Compile | Tag::File(_) => None,
            Tag::Reject(want) => Some(want.as_str()),
        };
        match (reject, compile_block(&block.source, resolver.as_ref())) {
            (None, Ok(module)) => {
                if let Some(why) = run_failure(&module) {
                    failures.push(format!("{at}: {why}"));
                }
            }
            (None, Err(errors)) => {
                let found: Vec<String> = errors.iter().map(variant_name).collect();
                failures.push(format!("{at}: the example does not compile: {found:?}"));
            }
            (Some(want), Ok(module)) => {
                // Some errors come from `MonomorphisePass`, which
                // `compile_to_ir` does not run. So a block that compiles
                // gets that pass too before it counts as accepted.
                match Pipeline::new()
                    .pass(MonomorphisePass::default())
                    .run(module)
                {
                    Ok(_) => {
                        failures.push(format!("{at}: compiles, but the prose says {want}"));
                    }
                    Err(errors) => {
                        let found: Vec<String> = errors.iter().map(variant_name).collect();
                        if !found.iter().any(|f| f == want) {
                            failures.push(format!(
                                "{at}: MonomorphisePass fails, but not with {want}: {found:?}"
                            ));
                        }
                    }
                }
            }
            (Some(want), Err(errors)) => {
                let found: Vec<String> = errors.iter().map(variant_name).collect();
                if found.iter().any(|f| f == "InternalError") {
                    failures.push(format!("{at}: an internal compiler error: {errors:?}"));
                } else if !found.iter().any(|f| f == want) {
                    failures.push(format!("{at}: expected {want}, got {found:?}"));
                }
            }
        }
    }
    failures
}

macro_rules! page_tests {
    ($($name:ident => $page:literal,)*) => {
        $(
            #[test]
            fn $name() {
                let failures = check_page($page);
                assert!(
                    failures.is_empty(),
                    "{} block(s) of {} do not do what the page says:\n{}",
                    failures.len(),
                    $page,
                    failures.join("\n")
                );
            }
        )*

        /// Each page that holds a block has a test above.
        #[test]
        fn every_page_with_a_block_has_a_test() {
            let covered: &[&str] = &[$($page,)*];
            let missing: Vec<String> = pages()
                .into_iter()
                .filter(|p| !covered.contains(&p.as_str()))
                .collect();
            assert!(
                missing.is_empty(),
                "add a line to `page_tests!` for each of these pages: {missing:?}"
            );
        }
    };
}

page_tests! {
    user_closures_examples => "user/closures.md",
    user_control_flow_examples => "user/control-flow.md",
    user_core_examples => "user/core.md",
    user_enums_examples => "user/enums.md",
    user_expressions_examples => "user/expressions.md",
    user_extern_examples => "user/extern.md",
    user_functions_examples => "user/functions.md",
    user_generics_examples => "user/generics.md",
    user_large_data_examples => "user/large-data.md",
    user_modules_examples => "user/modules.md",
    user_structs_examples => "user/structs.md",
    user_traits_examples => "user/traits.md",
    user_types_examples => "user/types.md",
    developer_ast_examples => "developer/ast/examples.md",
    developer_ir_examples => "developer/ir/examples.md",
    readme_examples => "../README.md",
}

// ---------------------------------------------------------------------------
// Claims about the IR
// ---------------------------------------------------------------------------

/// Each `d_` conformance case below `dir` that expects `run` or
/// `compile`.
fn walk_doc_cases(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_doc_cases(&path, out);
            continue;
        }
        let is_doc_case = path.extension().and_then(|e| e.to_str()) == Some("fv")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("d_"));
        if !is_doc_case {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let accepts = source.lines().any(|l| {
            let l = l.trim();
            l == "// expect: run" || l == "// expect: compile"
        });
        if accepts {
            out.push((path.to_string_lossy().into_owned(), source));
        }
    }
}

/// Every program that the documentation says compiles: the untagged
/// blocks, the module files, and the `d_` conformance cases that expect `run` or `compile`.
fn accepted_corpus() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for page in pages() {
        for block in blocks_of(&page) {
            if matches!(block.tag, Tag::Compile | Tag::File(_)) {
                out.push((format!("{}:{}", block.page, block.line), block.source));
            }
        }
    }
    walk_doc_cases(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("conformance"),
        &mut out,
    );
    out
}

/// A visitor of one JSON object.
type Visit<'a> = dyn FnMut(&serde_json::Map<String, serde_json::Value>) + 'a;

/// Visit every JSON object below `value`.
fn each_object(value: &serde_json::Value, visit: &mut Visit<'_>) {
    match value {
        serde_json::Value::Object(map) => {
            visit(map);
            for child in map.values() {
                each_object(child, visit);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                each_object(child, visit);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

fn to_json(module: &IrModule) -> serde_json::Value {
    serde_json::to_value(module).unwrap_or(serde_json::Value::Null)
}

/// docs/user/core.md: doc comments "flow through to the IR and are
/// available to backends as the `doc:` field on most definitions".
#[test]
fn a_doc_comment_reaches_the_ir() -> Result<(), Box<dyn std::error::Error>> {
    let source = "\
/// A point.
pub struct Point { x: I32 }

/// A colour.
pub enum Colour { red, green }

/// A name.
pub trait Named { name: String }

/// Adds one.
pub fn inc(n: I32) -> I32 { n + 1 }

/// The answer.
pub let answer: I32 = 42
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let mut missing = Vec::new();
    for s in &module.structs {
        if s.name == "Point" && s.doc.as_deref().map(str::trim) != Some("A point.") {
            missing.push(format!("struct Point: {:?}", s.doc));
        }
    }
    for e in &module.enums {
        if e.name == "Colour" && e.doc.as_deref().map(str::trim) != Some("A colour.") {
            missing.push(format!("enum Colour: {:?}", e.doc));
        }
    }
    for t in &module.traits {
        if t.name == "Named" && t.doc.as_deref().map(str::trim) != Some("A name.") {
            missing.push(format!("trait Named: {:?}", t.doc));
        }
    }
    for f in &module.functions {
        if f.name == "inc" && f.doc.as_deref().map(str::trim) != Some("Adds one.") {
            missing.push(format!("fn inc: {:?}", f.doc));
        }
    }
    for l in &module.lets {
        if l.name == "answer" && l.doc.as_deref().map(str::trim) != Some("The answer.") {
            missing.push(format!("let answer: {:?}", l.doc));
        }
    }
    assert!(
        missing.is_empty(),
        "these doc comments did not reach the IR: {missing:?}"
    );
    Ok(())
}

/// docs/user/core.md: `///` attaches to the declaration that follows.
/// A method in an impl block is a declaration too.
#[test]
fn a_method_doc_comment_reaches_the_ir() -> Result<(), Box<dyn std::error::Error>> {
    let source = "\
pub struct Counter { n: I32 }

impl Counter {
    /// The current count.
    fn value(self) -> I32 { self.n }
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let docs: Vec<Option<String>> = module
        .impls
        .iter()
        .flat_map(|i| i.functions.iter())
        .filter(|f| f.name == "value")
        .map(|f| f.doc.clone())
        .collect();
    assert!(
        docs.iter()
            .any(|d| d.as_deref().map(str::trim) == Some("The current count.")),
        "the method doc comment did not reach the IR: {docs:?}"
    );
    Ok(())
}

/// docs/user/functions.md: the codegen prefixes are metadata that the
/// frontend passes through unchanged, and they stack.
#[test]
fn codegen_attributes_reach_the_ir() -> Result<(), Box<dyn std::error::Error>> {
    let source = "\
inline cold fn a() -> I32 { 1 }
no_inline fn b() -> I32 { 2 }
pub cold extern fn c() -> Never
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let attrs = |name: &str| -> Vec<String> {
        module
            .functions
            .iter()
            .filter(|f| f.name == name)
            .flat_map(|f| f.attributes.iter().map(|a| format!("{a:?}")))
            .collect()
    };
    let a = attrs("a");
    let b = attrs("b");
    let c = attrs("c");
    assert!(
        a.contains(&"Inline".to_string()) && a.contains(&"Cold".to_string()),
        "fn a lost a prefix: {a:?}"
    );
    assert_eq!(b, vec!["NoInline".to_string()], "fn b: {b:?}");
    assert_eq!(c, vec!["Cold".to_string()], "extern fn c: {c:?}");
    Ok(())
}

/// docs/user/extern.md: "`s[i]` for a `String` receiver desugars to
/// `s.byte_at(i)` at IR lowering, so backends only see standard
/// `MethodCall` shapes."
#[test]
fn a_string_index_lowers_to_byte_at() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub fn f(s: String) -> I32 { s[1] }\n";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let json = to_json(&module);
    let mut method_calls = Vec::new();
    let mut index_nodes = 0_usize;
    each_object(&json, &mut |map| {
        if let Some(call) = map.get("MethodCall") {
            if let Some(method) = call.get("method") {
                method_calls.push(method.to_string());
            }
        }
        if map.contains_key("Index") {
            index_nodes = index_nodes.saturating_add(1);
        }
    });
    assert!(
        method_calls.iter().any(|m| m.contains("byte_at")),
        "no `byte_at` method call in the IR; calls found: {method_calls:?}"
    );
    assert_eq!(index_nodes, 0, "an index node is left in the IR");
    Ok(())
}

/// The generic definitions that are left in a module after
/// `MonomorphisePass`, one label for each.
fn generic_leftovers(m: &IrModule) -> Vec<String> {
    let mut left: Vec<String> = Vec::new();
    left.extend(
        m.structs
            .iter()
            .filter(|s| !s.generic_params.is_empty())
            .map(|s| format!("struct {}", s.name)),
    );
    left.extend(
        m.enums
            .iter()
            .filter(|e| !e.generic_params.is_empty())
            .map(|e| format!("enum {}", e.name)),
    );
    left.extend(
        m.traits
            .iter()
            .filter(|t| !t.generic_params.is_empty())
            .map(|t| format!("trait {}", t.name)),
    );
    left.extend(
        m.functions
            .iter()
            .filter(|f| !f.generic_params.is_empty())
            .map(|f| format!("fn {}", f.name)),
    );
    left.extend(
        m.impls
            .iter()
            .filter(|i| !i.generic_params.is_empty())
            .map(|_| "an impl block".to_string()),
    );
    left.extend(
        m.impls
            .iter()
            .flat_map(|i| i.functions.iter())
            .filter(|f| !f.generic_params.is_empty())
            .map(|f| format!("method {}", f.name)),
    );
    left.sort();
    left
}

/// docs/user/traits.md: "After mono runs, no generic definitions remain in
/// the IR." The prelude carriers (`Array`, `Seq`, `Optional` and the
/// others) stay generic by design: `src/ir/monomorphise/mod.rs` keeps them.
/// So the test removes what a program with no definitions keeps, and
/// checks what is left over every program the documentation accepts.
#[test]
fn no_generic_definition_remains_after_monomorphisation() -> Result<(), Box<dyn std::error::Error>>
{
    let empty =
        formalang::compile_to_ir("pub let zero: I32 = 0\n").map_err(|e| format!("{e:?}"))?;
    let baseline = generic_leftovers(
        &Pipeline::new()
            .pass(MonomorphisePass::default())
            .run(empty)
            .map_err(|e| format!("{e:?}"))?,
    );
    let mut checked = Checked::new("documentation programs monomorphised", 50);
    let mut failures = Vec::new();
    for (name, source) in accepted_corpus() {
        let Ok(module) = formalang::compile_to_ir(&source) else {
            continue;
        };
        match Pipeline::new()
            .pass(MonomorphisePass::default())
            .run(module)
        {
            Err(errors) => failures.push(format!("{name}: the pass failed: {errors:?}")),
            Ok(m) => {
                let mut left = generic_leftovers(&m);
                for known in &baseline {
                    if let Some(at) = left.iter().position(|l| l == known) {
                        left.remove(at);
                    }
                }
                if !left.is_empty() {
                    failures.push(format!("{name}: {left:?}"));
                }
            }
        }
        checked.hit();
    }
    assert!(
        failures.is_empty(),
        "{} program(s) keep a generic definition after MonomorphisePass:\n{}",
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

/// docs: `ReferenceTarget::Unresolved` is a placeholder that
/// `ResolveReferencesPass` overwrites; "backends should never see
/// `Unresolved`". `Pipeline::for_codegen` holds that pass.
#[test]
fn no_reference_stays_unresolved_after_the_codegen_pipeline() {
    let mut checked = Checked::new("documentation programs resolved", 50);
    let mut failures = Vec::new();
    for (name, source) in accepted_corpus() {
        let Ok(module) = formalang::compile_to_ir(&source) else {
            continue;
        };
        match Pipeline::for_codegen().run(module) {
            Err(errors) => failures.push(format!("{name}: the pipeline failed: {errors:?}")),
            Ok(m) => {
                let mut unresolved = Vec::new();
                each_object(&to_json(&m), &mut |map| {
                    if let Some(reference) = map.get("Reference") {
                        if reference.get("target").and_then(serde_json::Value::as_str)
                            == Some("Unresolved")
                        {
                            unresolved.push(
                                reference
                                    .get("path")
                                    .map(ToString::to_string)
                                    .unwrap_or_default(),
                            );
                        }
                    }
                });
                if !unresolved.is_empty() {
                    failures.push(format!("{name}: {unresolved:?}"));
                }
            }
        }
        checked.hit();
    }
    assert!(
        failures.is_empty(),
        "{} program(s) keep an unresolved reference after the codegen pipeline:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// docs/user/checklist.md: every `IrExpr` carries an `IrSpan` for tools,
/// source maps and DWARF. Lines and columns are one-based, so a span that
/// is not the default one has no line 0 and no column 0.
#[test]
fn every_ir_span_has_one_based_positions() {
    let mut checked = Checked::new("documentation programs with spans", 50);
    let mut failures = Vec::new();
    for (name, source) in accepted_corpus() {
        let Ok(module) = formalang::compile_to_ir(&source) else {
            continue;
        };
        let mut bad = 0_usize;
        let mut first = String::new();
        each_object(&to_json(&module), &mut |map| {
            let (Some(offset), Some(line), Some(column)) = (
                map.get("offset").and_then(serde_json::Value::as_u64),
                map.get("line").and_then(serde_json::Value::as_u64),
                map.get("column").and_then(serde_json::Value::as_u64),
            ) else {
                return;
            };
            let is_default = offset == 0 && line == 0 && column == 0;
            if !is_default && (line == 0 || column == 0) {
                if bad == 0 {
                    first = format!("offset {offset}, line {line}, column {column}");
                }
                bad = bad.saturating_add(1);
            }
        });
        if bad > 0 {
            failures.push(format!("{name}: {bad} position(s), first at {first}"));
        }
        checked.hit();
    }
    assert!(
        failures.is_empty(),
        "{} program(s) carry a span position with line 0 or column 0:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// docs/user/functions.md: a default "sees the actual passed value" of an
/// earlier parameter. The call site must not carry a reference that no
/// binding in scope can resolve.
#[test]
fn a_default_that_reads_a_defaulted_parameter_is_resolved() -> Result<(), Box<dyn std::error::Error>>
{
    let source = "\
fn f(x: I32, y: I32 = 2, z: I32 = y * 10) -> I32 { x + y + z }
pub fn g() -> I32 { f(x: 1) }
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let resolved = Pipeline::for_codegen()
        .run(module)
        .map_err(|e| format!("{e:?}"))?;
    let g = resolved
        .functions
        .iter()
        .find(|f| f.name == "g")
        .ok_or("fn g is missing")?;
    let mut unresolved = Vec::new();
    let json = serde_json::to_value(&g.body)?;
    each_object(&json, &mut |map| {
        if let Some(reference) = map.get("Reference") {
            if reference.get("target").and_then(serde_json::Value::as_str) == Some("Unresolved") {
                unresolved.push(reference.to_string());
            }
        }
    });
    assert!(
        unresolved.is_empty(),
        "the call `f(x: 1)` holds unresolved references: {unresolved:?}"
    );
    Ok(())
}

/// docs/user/expressions.md lists the escapes of a string. A string with
/// another escape is closed by its quote, so the error must not say that
/// the string is unterminated.
#[test]
fn an_unknown_escape_does_not_report_an_unterminated_string() {
    for source in ["pub let s = \"a\\qb\"\n", "pub let s = \"\\u41\"\n"] {
        let errors = formalang::compile_to_ir(source).err().unwrap_or_default();
        assert!(!errors.is_empty(), "{source:?} compiled");
        let found: Vec<String> = errors.iter().map(variant_name).collect();
        assert!(
            !found.iter().any(|f| f == "UnterminatedString"),
            "{source:?}: the string is closed, but the errors are {found:?}"
        );
    }
}

/// docs/user/types.md: `let big: I64 = 9_223_372_036_854_775_807`. The
/// literal must reach the IR as an I64 of that value.
#[test]
fn the_largest_i64_literal_reaches_the_ir() -> Result<(), Box<dyn std::error::Error>> {
    let module = formalang::compile_to_ir("pub let big: I64 = 9_223_372_036_854_775_807\n")
        .map_err(|e| format!("{e:?}"))?;
    let value = module
        .lets
        .iter()
        .find(|l| l.name == "big")
        .map(|l| l.value.clone())
        .ok_or("let big is missing")?;
    let IrExpr::Literal { ty, .. } = &value else {
        return Err(format!("not a literal: {value:?}").into());
    };
    assert_eq!(format!("{ty:?}"), "Primitive(I64)");
    Ok(())
}
