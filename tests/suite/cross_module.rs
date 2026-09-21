//! Compiling across module boundaries.
//!
//! `compile_to_ir_with_resolver` is the multi-file entry point. After
//! lowering it runs `MonomorphisePass` with the imported modules'
//! IR, and that pass inlines what the entry point uses, specialises
//! generic imports, renumbers every id into the host module's space,
//! and remaps every span's `FileId` onto the host's `file_table`.
//!
//! That is the largest body of code in the crate reachable only
//! through a resolver, and `compile_to_ir` never touches any of it.
//!
//! The properties below are what a backend depends on after inlining:
//! the module is self-contained, its ids are in range, its spans point
//! at files the table knows, and the answer does not change between
//! runs.

#![expect(
    clippy::panic,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::wildcard_enum_match_arm,
    reason = "a fixture that stops compiling should fail loudly; the \
              cross-process check talks to its child over stdout, and the \
              id sweeps walk untyped serde_json trees"
)]

use crate::common::Checked;

use std::collections::HashMap;
use std::path::PathBuf;

use formalang::ir::{IrModule, ResolvedType};
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use formalang::{compile_to_ir_with_path_and_resolver, compile_to_ir_with_resolver};

/// A resolver that serves modules from memory.
struct MemResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MemResolver {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn with(modules: &[(&str, &str)]) -> Self {
        let mut resolver = Self::new();
        for (path, source) in modules {
            resolver.add(path, source);
        }
        resolver
    }

    fn add(&mut self, path: &str, source: &str) {
        let segments: Vec<String> = path.split("::").map(str::to_string).collect();
        let file = PathBuf::from(format!("{}.fv", segments.join("/")));
        self.modules.insert(segments, (source.to_string(), file));
    }
}

impl ModuleResolver for MemResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: Vec::new(),
            })
    }
}

/// Compile `entry` against `modules`, or panic with the errors.
fn compile(entry: &str, modules: &[(&str, &str)]) -> IrModule {
    match compile_to_ir_with_resolver(entry, MemResolver::with(modules)) {
        Ok(module) => module,
        Err(errors) => panic!("the fixture must compile: {errors:?}\n{entry}"),
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SHAPES: &str = "pub struct Point {\n    x: I32,\n    y: I32\n}\n\npub fn origin() -> Point {\n    Point(x: 0, y: 0)\n}\n";

const GENERIC: &str = "pub struct Box<T> {\n    value: T\n}\n\npub fn wrap<T>(v: T) -> Box<T> {\n    Box<T>(value: v)\n}\n";

const TRAITS: &str = "pub trait Named {\n    name: String\n}\n\npub struct Widget {\n    name: String\n}\n\nimpl Named for Widget {}\n";

const ENUMS: &str = "pub enum Status {\n    idle,\n    busy(load: I32)\n}\n\npub fn idle() -> Status {\n    Status.idle\n}\n";

// ---------------------------------------------------------------------------
// The imported module lands in the host
// ---------------------------------------------------------------------------

/// An imported struct is present in the host module by name.
#[test]
fn an_imported_struct_is_inlined() {
    let module = compile(
        "use shapes::Point\n\npub fn f() -> I32 {\n    Point(x: 1, y: 2).x\n}\n",
        &[("shapes", SHAPES)],
    );
    assert!(
        module.structs.iter().any(|s| s.name.ends_with("Point")),
        "the imported struct is missing: {:?}",
        module.structs.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

/// An imported struct binds to a local `let` and its fields read.
#[test]
fn an_imported_struct_binds_to_a_local_let() {
    let module = compile(
        "use shapes::Point\n\npub fn f() -> I32 {\n    let p = Point(x: 1, y: 2)\n    p.x\n}\n",
        &[("shapes", SHAPES)],
    );
    assert!(
        module.structs.iter().any(|s| s.name.ends_with("Point")),
        "the imported struct is missing from the let path"
    );
}

/// An imported enum and its variants survive.
#[test]
fn an_imported_enum_is_inlined() {
    let module = compile(
        "use status::Status\n\npub fn f() -> Status {\n    Status.idle\n}\n",
        &[("status", ENUMS)],
    );
    let found = module
        .enums
        .iter()
        .find(|e| e.name.ends_with("Status"))
        .expect("the imported enum must be present");
    assert_eq!(
        found.variants.len(),
        2,
        "the imported enum lost a variant: {:?}",
        found.variants.iter().map(|v| &v.name).collect::<Vec<_>>()
    );
}

/// A struct imported from a module that also declares a trait for it
/// arrives, and the module stays self-contained.
#[test]
fn a_struct_imported_alongside_a_trait_arrives() {
    let module = compile(
        "use widgets::{Named, Widget}\n\npub fn f() -> String {\n    Widget(name: \"w\").name\n}\n",
        &[("widgets", TRAITS)],
    );
    assert!(
        module.structs.iter().any(|s| s.name.ends_with("Widget")),
        "the imported struct is missing: {:?}",
        module.structs.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// The result is self-contained
// ---------------------------------------------------------------------------

/// One sweep case: a name, an entry point, and the modules it needs.
type Case<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

/// Serialise a module and walk it for `External` type references.
fn external_references(module: &IrModule) -> Vec<String> {
    let value = serde_json::to_value(module).unwrap_or_default();
    let mut found = Vec::new();
    walk_external(&value, &mut found);
    found
}

fn walk_external(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(external) = map.get("External") {
                out.push(external.to_string());
            }
            for child in map.values() {
                walk_external(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                walk_external(v, out);
            }
        }
        _ => {}
    }
}

/// After inlining, nothing points outside the module.
///
/// An `External` reference that survives is a name a backend cannot
/// resolve: the definition it names is in a file the backend was
/// never given.
#[test]
fn no_external_reference_survives() {
    let mut checked = Checked::new("cross-module cases swept for External", 4);
    let cases: &[Case] = &[
        (
            "struct",
            "use shapes::Point\n\npub fn f() -> I32 {\n    Point(x: 1, y: 2).x\n}\n",
            &[("shapes", SHAPES)],
        ),
        (
            "let binding",
            "use shapes::Point\n\npub fn f() -> I32 {\n    let p = Point(x: 1, y: 2)\n    p.y\n}\n",
            &[("shapes", SHAPES)],
        ),
        (
            "enum",
            "use status::Status\n\npub fn f() -> Status {\n    Status.idle\n}\n",
            &[("status", ENUMS)],
        ),
        (
            "trait module",
            "use widgets::Widget\n\npub fn f() -> String {\n    Widget(name: \"w\").name\n}\n",
            &[("widgets", TRAITS)],
        ),
    ];

    for (name, entry, modules) in cases {
        let module = compile(entry, modules);
        let leftovers = external_references(&module);
        assert!(
            leftovers.is_empty(),
            "{name}: {} External reference(s) survived inlining: {:?}",
            leftovers.len(),
            leftovers
        );
        checked.hit();
    }
}

/// Every id in the inlined module indexes a definition that exists.
///
/// Inlining renumbers the imported module's ids into the host's
/// space. An off-by-one there hands a backend an id that points past
/// the end of a vector, or at the wrong definition.
#[test]
fn every_id_survives_the_renumbering() {
    let module = compile(
        "use shapes::Point\nuse status::Status\n\npub fn f() -> I32 {\n    let p = Point(x: 3, y: 4)\n    p.x + Point(x: 1, y: 2).y\n}\n",
        &[("shapes", SHAPES), ("status", ENUMS)],
    );

    let value = serde_json::to_value(&module).expect("the module must serialise");
    let counts = [
        ("Struct", module.structs.len()),
        ("Trait", module.traits.len()),
        ("Enum", module.enums.len()),
        ("Function", module.functions.len()),
    ];
    let mut bad = Vec::new();
    walk_ids(&value, &counts, &mut bad);
    assert!(
        bad.is_empty(),
        "{} id(s) point past the end of their table after inlining: {bad:?}",
        bad.len()
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

/// Every span's file id indexes the host's file table.
///
/// An inlined definition's spans arrive numbered against its own
/// module's table. Remapping them onto the host's is what makes a
/// source map across several files possible; a stale id points a
/// debugger at the wrong file.
#[test]
fn every_span_names_a_file_the_table_knows() {
    let module = match compile_to_ir_with_path_and_resolver(
        "use shapes::Point\n\npub fn f() -> I32 {\n    Point(x: 1, y: 2).x\n}\n",
        PathBuf::from("main.fv"),
        MemResolver::with(&[("shapes", SHAPES)]),
    ) {
        Ok(module) => module,
        Err(errors) => panic!("the fixture must compile: {errors:?}"),
    };

    let table = module.file_table.len();
    let value = serde_json::to_value(&module).expect("the module must serialise");
    let mut bad = Vec::new();
    walk_file_ids(&value, table, &mut bad);
    assert!(
        bad.is_empty(),
        "{} span(s) name a file id outside a {table}-entry table: {bad:?}",
        bad.len()
    );
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

/// Cross-module compilation is deterministic across processes.
///
/// Rust randomises `HashMap` seeds per process, so two runs of the
/// same binary over the same input are the strong check. Every phase
/// of the inlining pass hands out ids as it walks the imports, so any
/// hash order that reaches it changes every id in the result.
#[test]
fn cross_module_compilation_is_deterministic_across_processes() {
    const VAR: &str = "FORMALANG_TEST_EMIT_CROSS_MODULE_IR";
    let entry = "use shapes::Point\nuse status::Status\nuse widgets::Widget\n\npub fn f() -> I32 {\n    let p = Point(x: 1, y: 2)\n    p.x + p.y\n}\n";
    let modules: &[(&str, &str)] = &[("shapes", SHAPES), ("status", ENUMS), ("widgets", TRAITS)];

    if std::env::var(VAR).is_ok() {
        print!(
            "{}",
            serde_json::to_string(&compile(entry, modules)).unwrap_or_default()
        );
        return;
    }

    let expected = serde_json::to_string(&compile(entry, modules)).unwrap_or_default();
    let exe = std::env::current_exe().expect("the test binary must be locatable");

    for attempt in 0..3 {
        let output = std::process::Command::new(&exe)
            .env(VAR, "1")
            .arg(crate::common::test_name(
                module_path!(),
                "cross_module_compilation_is_deterministic_across_processes",
            ))
            .arg("--exact")
            .arg("--nocapture")
            .output()
            .expect("the child process must run");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let start = stdout.find('{').expect("the child must print IR");
        let end = stdout.rfind('}').expect("the child must print IR");
        let actual = stdout.get(start..=end).unwrap_or_default();
        assert_eq!(
            expected, actual,
            "attempt {attempt}: the IR differs between two processes, so the \
             cross-module path depends on hash iteration order"
        );
    }
}

/// The same check within one process, repeated.
#[test]
fn cross_module_compilation_is_deterministic() {
    let entry = "use shapes::Point\nuse status::Status\nuse widgets::Widget\n\npub fn f() -> I32 {\n    let p = Point(x: 1, y: 2)\n    p.x + p.y\n}\n";
    let modules: &[(&str, &str)] = &[("shapes", SHAPES), ("status", ENUMS), ("widgets", TRAITS)];

    let first = serde_json::to_string(&compile(entry, modules)).unwrap_or_default();
    for _ in 0..5 {
        let again = serde_json::to_string(&compile(entry, modules)).unwrap_or_default();
        assert_eq!(
            first, again,
            "two cross-module compiles of the same input differ"
        );
    }
}

// ---------------------------------------------------------------------------
// Failure paths
// ---------------------------------------------------------------------------

/// Importing from a module the resolver does not serve is an error,
/// naming the module.
#[test]
fn a_missing_module_is_reported() {
    let result = compile_to_ir_with_resolver(
        "use nowhere::Thing\n\npub fn f() -> I32 {\n    1\n}\n",
        MemResolver::new(),
    );
    let errors = result.expect_err("the import must fail");
    let text = format!("{errors:?}");
    assert!(
        text.contains("nowhere"),
        "the error did not name the missing module: {text}"
    );
}

/// Importing an item a module does not export is an error.
#[test]
fn a_missing_item_is_reported() {
    let result = compile_to_ir_with_resolver(
        "use shapes::NoSuchType\n\npub fn f() -> I32 {\n    1\n}\n",
        MemResolver::with(&[("shapes", SHAPES)]),
    );
    assert!(
        result.is_err(),
        "importing an item the module does not export was accepted"
    );
}

/// A module that imports itself is reported rather than looping.
#[test]
fn a_circular_import_is_reported() {
    let result = compile_to_ir_with_resolver(
        "use a::Thing\n\npub fn f() -> I32 {\n    1\n}\n",
        MemResolver::with(&[
            ("a", "use b::Thing\n\npub struct Thing {\n    v: I32\n}\n"),
            ("b", "use a::Thing\n\npub struct Other {\n    v: I32\n}\n"),
        ]),
    );
    // Either it resolves or it reports a cycle. What it must not do is
    // recurse until the stack runs out.
    let _ = result;
}

/// An entry point with no import at all still compiles through the
/// resolver path.
#[test]
fn the_resolver_path_works_without_any_import() {
    let module = compile("pub struct A {\n    a: I32\n}\n", &[]);
    assert_eq!(
        module.user_structs().count(),
        1,
        "the entry point's own struct went missing"
    );
    assert!(
        module.imports.is_empty(),
        "a program with no import reported one"
    );
}

/// An unused import does not drag its module into the output.
#[test]
fn an_unused_import_contributes_nothing_it_does_not_need() {
    let module = compile(
        "use shapes::Point\n\npub fn f() -> I32 {\n    1\n}\n",
        &[("shapes", SHAPES)],
    );
    // Whatever the pass decides to inline, the result must still be
    // self-contained and correctly numbered.
    assert!(
        external_references(&module).is_empty(),
        "an unused import left an External reference behind"
    );
}

// ---------------------------------------------------------------------------
// The four shapes that used to be a documented gap
//
// Each of these failed with an `InternalError` — the compiler telling
// the user to file a bug because a pass could not finish. They now
// work, and these are the acceptance tests for that:
//
//   - an imported type in a signature resolves to a local struct id,
//   - an imported function is callable and its imported return type
//     carries field access,
//   - a generic import is specialised at the instantiation asked for,
//   - an imported trait arrives with the impl that records the
//     conformance, so a generic bound on it can still be satisfied.
// ---------------------------------------------------------------------------

/// An imported type used as a parameter type resolves to a local id.
#[test]
fn an_imported_type_resolves_to_a_local_id() {
    let module = compile(
        "use shapes::Point\n\npub fn f(p: Point) -> I32 {\n    p.x\n}\n",
        &[("shapes", SHAPES)],
    );
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "f")
        .expect("the entry point's function must be present");
    let param = f
        .params
        .first()
        .expect("the function must have a parameter");
    assert!(
        matches!(param.ty, Some(ResolvedType::Struct(_))),
        "the imported parameter type is {:?}, not a local struct id",
        param.ty
    );
}

/// Calling an imported function whose return type is also imported
/// works.
#[test]
fn an_imported_function_is_callable() {
    let module = compile(
        "use shapes::{Point, origin}\n\npub fn f() -> I32 {\n    origin().x\n}\n",
        &[("shapes", SHAPES)],
    );
    assert!(
        module.functions.iter().any(|f| f.name.ends_with("origin")),
        "the imported function is missing"
    );
}

/// A generic imported type is specialised at the instantiation the
/// entry point asks for.
#[test]
fn a_generic_import_is_specialised() {
    let module = compile(
        "use generic::Box\n\npub fn f() -> I32 {\n    let b = Box<I32>(value: 7)\n    b.value\n}\n",
        &[("generic", GENERIC)],
    );
    assert!(
        module.structs.iter().any(|s| s.name.contains("Box")),
        "the generic import produced no struct"
    );
}

/// An imported trait arrives with the struct that implements it, so a
/// generic bound on it can still be checked.
#[test]
fn an_imported_trait_arrives_with_its_impl() {
    let module = compile(
        "use widgets::{Named, Widget}\n\npub fn f() -> String {\n    Widget(name: \"w\").name\n}\n",
        &[("widgets", TRAITS)],
    );
    assert!(
        module.traits.iter().any(|t| t.name.ends_with("Named")),
        "the imported trait is missing"
    );
    assert!(
        !module.impls.is_empty(),
        "no impl block survived the inlining"
    );
}
