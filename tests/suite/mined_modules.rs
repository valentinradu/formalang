//! Rules for modules on disk, taken from the privacy and import tests of
//! other languages.
//!
//! A conformance case is one file. The rules here need two or more
//! files, so each test writes its modules to a temporary directory and
//! compiles the entry through a `FileSystemResolver`.
//!
//! The sources are Rust `tests/ui/privacy/` and `tests/ui/resolve/`, and
//! Go `test/import*.go`. Each test names the rule it takes, and
//! `docs/user/modules.md` where it states the rule.

#![expect(
    clippy::panic,
    reason = "a test reports its own failure by failing loudly"
)]

use std::path::Path;

use formalang::semantic::module_resolver::FileSystemResolver;
use formalang::{compile_to_ir_with_resolver, CompilerError, IrModule};

use crate::common::interpreter::Interpreter;

/// The name of an error's variant, from its `Debug` form.
fn variant(error: &CompilerError) -> String {
    format!("{error:?}")
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Write each `(path, source)` pair under a new temporary directory.
fn setup(modules: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("no temporary directory: {e}"));
    for (path, source) in modules {
        write(dir.path(), path, source);
    }
    dir
}

/// Compile `entry` with the directory of `modules` as the module root.
fn compile(entry: &str, modules: &[(&str, &str)]) -> Result<IrModule, Vec<CompilerError>> {
    let dir = setup(modules);
    compile_to_ir_with_resolver(entry, FileSystemResolver::new(dir.path().to_path_buf()))
}

fn write(root: &Path, path: &str, source: &str) {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("cannot make {}: {e}", parent.display()));
    }
    std::fs::write(&file, source)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", file.display()));
}

/// The compile must fail. When `want` is given, one of the errors must
/// be that variant. An internal error is never a correct answer.
fn assert_rejects(entry: &str, modules: &[(&str, &str)], want: Option<&str>) {
    match compile(entry, modules) {
        Ok(_) => panic!(
            "compiled, but must be rejected{}",
            want.map_or(String::new(), |w| format!(" with {w}"))
        ),
        Err(errors) => {
            let found: Vec<String> = errors.iter().map(variant).collect();
            assert!(
                !found.iter().any(|f| f == "InternalError"),
                "rejected with an internal compiler error: {errors:?}"
            );
            if let Some(want) = want {
                assert!(
                    found.iter().any(|f| f == want),
                    "expected {want}, got {found:?}: {errors:?}"
                );
            }
        }
    }
}

/// The compile must succeed, and `run_checks()` must pass at least one
/// assertion.
fn assert_runs(entry: &str, modules: &[(&str, &str)]) {
    let module =
        compile(entry, modules).unwrap_or_else(|errors| panic!("did not compile: {errors:?}"));
    let mut interpreter = Interpreter::new(&module);
    match interpreter.run("run_checks") {
        Ok(_) => assert!(
            interpreter.asserts_passed > 0,
            "run_checks() asserted nothing"
        ),
        Err(fault) => panic!("{fault} (after {} assert(s))", interpreter.asserts_passed),
    }
}

fn assert_compiles(entry: &str, modules: &[(&str, &str)]) {
    if let Err(errors) = compile(entry, modules) {
        panic!("did not compile: {errors:?}");
    }
}

// ---------------------------------------------------------------------------
// Private items
// ---------------------------------------------------------------------------

/// docs/user/modules.md: can only import `pub` items. Rust E0603.
#[test]
fn a_private_function_cannot_be_imported() {
    assert_rejects(
        "use lib::hidden\n\npub fn f() -> I32 { hidden() }\n",
        &[("lib.fv", "fn hidden() -> I32 { 1 }\n")],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: can only import `pub` items. Rust E0603.
#[test]
fn a_private_struct_cannot_be_imported() {
    assert_rejects(
        "use lib::Hidden\n\npub fn f() -> I32 { Hidden(x: 1).x }\n",
        &[("lib.fv", "struct Hidden { x: I32 }\n")],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: can only import `pub` items. Rust E0603.
#[test]
fn a_private_let_cannot_be_imported() {
    assert_rejects(
        "use lib::SECRET\n\npub fn f() -> I32 { SECRET }\n",
        &[("lib.fv", "let SECRET: I32 = 1\n")],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: can only import `pub` items. Rust E0603.
#[test]
fn a_private_enum_cannot_be_imported() {
    assert_rejects(
        "use lib::Hidden\n\npub fn f() -> I32 {\n    let h: Hidden = .a\n    0\n}\n",
        &[("lib.fv", "enum Hidden { a, b }\n")],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: can only import `pub` items. Rust E0603.
#[test]
fn a_private_trait_cannot_be_imported() {
    assert_rejects(
        "use lib::Hidden\n\npub struct S { x: I32 }\n\nimpl Hidden for S { }\n",
        &[("lib.fv", "trait Hidden { x: I32 }\n")],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: can only import `pub` items. One private item in
/// a group is enough to refuse the group.
#[test]
fn a_private_item_in_a_group_cannot_be_imported() {
    assert_rejects(
        "use lib::{shown, hidden}\n\npub fn f() -> I32 { shown() + hidden() }\n",
        &[(
            "lib.fv",
            "pub fn shown() -> I32 { 1 }\nfn hidden() -> I32 { 2 }\n",
        )],
        Some("PrivateImport"),
    );
}

/// docs/user/modules.md: the item that a public function calls may stay
/// private. The caller sees the result only.
#[test]
fn a_public_function_may_call_a_private_helper() {
    assert_runs(
        "use lib::api\n\npub fn run_checks() {\n    assert(condition: api() == 42)\n}\n",
        &[(
            "lib.fv",
            "fn helper() -> I32 { 41 }\npub fn api() -> I32 { helper() + 1 }\n",
        )],
    );
}

/// A public function may read a private `let` of its own module.
#[test]
fn a_public_function_may_read_a_private_let() {
    assert_runs(
        "use lib::api\n\npub fn run_checks() {\n    assert(condition: api() == 7)\n}\n",
        &[("lib.fv", "let K: I32 = 7\npub fn api() -> I32 { K }\n")],
    );
}

/// A default value is evaluated in the module that defines the
/// function, so it may read that module's private `let`.
#[test]
fn an_imported_default_reads_a_private_let_of_its_module() {
    assert_runs(
        "use lib::api\n\npub fn run_checks() {\n    assert(condition: api() == 9)\n    assert(condition: api(n: 1) == 1)\n}\n",
        &[("lib.fv", "let K: I32 = 9\npub fn api(n: I32 = K) -> I32 { n }\n")],
    );
}

/// docs/user/modules.md: a public signature names public types. The
/// imported module breaks that rule, so the program that imports it
/// must not compile. Rust `tests/ui/privacy/private-in-public*`.
#[test]
fn an_imported_module_that_leaks_a_private_type_is_rejected() {
    assert_rejects(
        "use lib::make\n\npub fn f() -> I32 { 0 }\n",
        &[(
            "lib.fv",
            "struct Hidden { x: I32 }\npub fn make() -> Hidden { Hidden(x: 1) }\n",
        )],
        Some("PrivateTypeInPublic"),
    );
}

/// An error inside an imported module stops the program that imports
/// it. The importer may not compile against a broken module.
#[test]
fn a_type_error_in_an_imported_module_is_reported() {
    assert_rejects(
        "use lib::api\n\npub fn f() -> I32 { api() }\n",
        &[("lib.fv", "pub fn api() -> I32 { \"not a number\" }\n")],
        None,
    );
}

/// An error in a function the importer does not name still breaks the
/// imported module.
#[test]
fn a_type_error_in_an_unused_function_of_an_imported_module_is_reported() {
    assert_rejects(
        "use lib::good\n\npub fn f() -> I32 { good() }\n",
        &[(
            "lib.fv",
            "pub fn good() -> I32 { 1 }\npub fn bad() -> I32 { \"x\" }\n",
        )],
        None,
    );
}

// ---------------------------------------------------------------------------
// Names that do not resolve
// ---------------------------------------------------------------------------

/// Rust E0432, unresolved import.
#[test]
fn an_unknown_item_of_a_known_module_is_rejected() {
    assert_rejects(
        "use lib::missing\n\npub fn f() -> I32 { 0 }\n",
        &[("lib.fv", "pub fn present() -> I32 { 1 }\n")],
        Some("ImportItemNotFound"),
    );
}

/// Rust E0432, unresolved import in a group.
#[test]
fn an_unknown_item_in_a_group_is_rejected() {
    assert_rejects(
        "use lib::{present, missing}\n\npub fn f() -> I32 { present() }\n",
        &[("lib.fv", "pub fn present() -> I32 { 1 }\n")],
        Some("ImportItemNotFound"),
    );
}

/// Go `test/import*.go`: an import of a missing module is an error even
/// when nothing uses it.
#[test]
fn an_unused_import_of_a_missing_module_is_rejected() {
    assert_rejects(
        "use nowhere::thing\n\npub fn f() -> I32 { 0 }\n",
        &[],
        Some("ModuleNotFound"),
    );
}

/// An item that is not imported is not in scope, even when its module is
/// imported for another item.
#[test]
fn an_item_that_is_not_imported_is_not_in_scope() {
    assert_rejects(
        "use lib::a\n\npub fn f() -> I32 { b() }\n",
        &[(
            "lib.fv",
            "pub fn a() -> I32 { 1 }\npub fn b() -> I32 { 2 }\n",
        )],
        Some("UndefinedReference"),
    );
}

/// An import is not transitive. `mid` imports `leaf::x`, but the entry
/// did not, so `x` is not in the entry's scope.
#[test]
fn an_import_is_not_re_exported() {
    assert_rejects(
        "use mid::y\n\npub fn f() -> I32 { x() }\n",
        &[
            ("leaf.fv", "pub fn x() -> I32 { 1 }\n"),
            ("mid.fv", "use leaf::x\npub fn y() -> I32 { x() }\n"),
        ],
        Some("UndefinedReference"),
    );
}

/// An import is not transitive, so `mid::x` does not name `leaf::x`.
/// Rust E0603 on a private re-export.
#[test]
fn an_imported_name_cannot_be_imported_again_through_the_importer() {
    assert_rejects(
        "use mid::x\n\npub fn f() -> I32 { x() }\n",
        &[
            ("leaf.fv", "pub fn x() -> I32 { 1 }\n"),
            ("mid.fv", "use leaf::x\npub fn y() -> I32 { x() }\n"),
        ],
        None,
    );
}

// ---------------------------------------------------------------------------
// Name clashes
// ---------------------------------------------------------------------------

/// Rust E0252: two imports bind one name.
#[test]
fn two_imports_of_one_name_are_rejected() {
    assert_rejects(
        "use one::a\nuse two::a\n\npub fn f() -> I32 { a() }\n",
        &[
            ("one.fv", "pub fn a() -> I32 { 1 }\n"),
            ("two.fv", "pub fn a() -> I32 { 2 }\n"),
        ],
        None,
    );
}

/// Rust E0252: two imports of two structs of one name.
#[test]
fn two_imported_structs_of_one_name_are_rejected() {
    assert_rejects(
        "use one::P\nuse two::P\n\npub fn f() -> I32 { P(x: 1).x }\n",
        &[
            ("one.fv", "pub struct P { x: I32 }\n"),
            ("two.fv", "pub struct P { x: I32 }\n"),
        ],
        None,
    );
}

/// Rust E0255: an import clashes with a local item of the same name.
#[test]
fn an_import_that_clashes_with_a_local_struct_is_rejected() {
    assert_rejects(
        "use lib::P\n\nstruct P { y: I32 }\n\npub fn f() -> I32 { 0 }\n",
        &[("lib.fv", "pub struct P { x: I32 }\n")],
        None,
    );
}

/// Rust E0255: an import clashes with a local function of the same name.
#[test]
fn an_import_that_clashes_with_a_local_function_is_rejected() {
    assert_rejects(
        "use lib::a\n\nfn a() -> I32 { 2 }\n\npub fn f() -> I32 { a() }\n",
        &[("lib.fv", "pub fn a() -> I32 { 1 }\n")],
        None,
    );
}

/// Rust E0252 also refuses the same import twice.
#[test]
fn the_same_import_twice_is_rejected() {
    assert_rejects(
        "use lib::a\nuse lib::a\n\npub fn f() -> I32 { a() }\n",
        &[("lib.fv", "pub fn a() -> I32 { 1 }\n")],
        None,
    );
}

/// Two modules may each have a private item of one name. Neither leaves
/// its module, so they do not clash, and each public function reads its
/// own.
#[test]
fn two_modules_may_hold_private_items_of_one_name() {
    assert_runs(
        "use one::a\nuse two::b\n\npub fn run_checks() {\n    assert(condition: a() == 1)\n    assert(condition: b() == 2)\n}\n",
        &[
            ("one.fv", "fn k() -> I32 { 1 }\npub fn a() -> I32 { k() }\n"),
            ("two.fv", "fn k() -> I32 { 2 }\npub fn b() -> I32 { k() }\n"),
        ],
    );
}

/// Two modules may each have a private `let` of one name, and each
/// public function reads its own.
#[test]
fn two_modules_may_hold_private_lets_of_one_name() {
    assert_runs(
        "use one::a\nuse two::b\n\npub fn run_checks() {\n    assert(condition: a() == 1)\n    assert(condition: b() == 2)\n}\n",
        &[
            ("one.fv", "let K: I32 = 1\npub fn a() -> I32 { K }\n"),
            ("two.fv", "let K: I32 = 2\npub fn b() -> I32 { K }\n"),
        ],
    );
}

/// An imported function and a local function of another name that
/// calls a private function of the same name as the imported one's
/// helper. Each call resolves in its own module.
#[test]
fn a_local_function_and_an_imported_helper_of_one_name() {
    assert_runs(
        "use lib::api\n\nfn helper() -> I32 { 100 }\n\npub fn run_checks() {\n    assert(condition: api() == 1)\n    assert(condition: helper() == 100)\n}\n",
        &[("lib.fv", "fn helper() -> I32 { 1 }\npub fn api() -> I32 { helper() }\n")],
    );
}

// ---------------------------------------------------------------------------
// Cycles
// ---------------------------------------------------------------------------

/// docs/user/modules.md: no circular imports allowed.
#[test]
fn two_modules_that_import_each_other_are_rejected() {
    assert_rejects(
        "use a::fa\n\npub fn f() -> I32 { fa() }\n",
        &[
            ("a.fv", "use b::fb\npub fn fa() -> I32 { fb() }\n"),
            ("b.fv", "use a::fa\npub fn fb() -> I32 { 1 }\n"),
        ],
        Some("CircularImport"),
    );
}

/// docs/user/modules.md: no circular imports allowed. A module that
/// imports itself is the shortest cycle.
#[test]
fn a_module_that_imports_itself_is_rejected() {
    assert_rejects(
        "use a::fa\n\npub fn f() -> I32 { fa() }\n",
        &[(
            "a.fv",
            "use a::fb\npub fn fa() -> I32 { 1 }\npub fn fb() -> I32 { 2 }\n",
        )],
        Some("CircularImport"),
    );
}

/// docs/user/modules.md: no circular imports allowed. A three-step cycle.
#[test]
fn a_three_module_cycle_is_rejected() {
    assert_rejects(
        "use a::fa\n\npub fn f() -> I32 { fa() }\n",
        &[
            ("a.fv", "use b::fb\npub fn fa() -> I32 { fb() }\n"),
            ("b.fv", "use c::fc\npub fn fb() -> I32 { fc() }\n"),
            ("c.fv", "use a::fa\npub fn fc() -> I32 { 1 }\n"),
        ],
        Some("CircularImport"),
    );
}

/// A diamond is not a cycle: two modules import one shared leaf.
#[test]
fn a_diamond_of_imports_is_not_a_cycle() {
    assert_runs(
        "use left::l\nuse right::r\n\npub fn run_checks() {\n    assert(condition: l() + r() == 3)\n}\n",
        &[
            ("leaf.fv", "pub fn base() -> I32 { 1 }\n"),
            ("left.fv", "use leaf::base\npub fn l() -> I32 { base() }\n"),
            ("right.fv", "use leaf::base\npub fn r() -> I32 { base() + 1 }\n"),
        ],
    );
}

// ---------------------------------------------------------------------------
// Imported values
// ---------------------------------------------------------------------------

/// An imported `pub let` is immutable in the importer.
#[test]
fn an_imported_let_cannot_be_assigned() {
    assert_rejects(
        "use lib::K\n\npub fn f() -> I32 {\n    K = 2\n    K\n}\n",
        &[("lib.fv", "pub let K: I32 = 1\n")],
        Some("AssignmentToImmutable"),
    );
}

/// An imported `pub let mut` belongs to its module. The importer may not
/// assign it: that would change the state of another module.
#[test]
fn an_imported_mutable_let_cannot_be_assigned() {
    assert_rejects(
        "use lib::K\n\npub fn f() -> I32 {\n    K = 2\n    K\n}\n",
        &[("lib.fv", "pub let mut K: I32 = 1\n")],
        None,
    );
}

/// An imported `let` is shared by every module that imports it, so one
/// call may not own it. Rust: cannot move out of a static item.
#[test]
fn an_imported_let_cannot_be_sunk() {
    assert_rejects(
        "use lib::S\n\nfn take(sink s: String) -> I32 { s.len() }\n\npub fn f() -> I32 { take(s: S) }\n",
        &[("lib.fv", "pub let S: String = \"x\"\n")],
        None,
    );
}

/// An imported `let` cannot go to a `mut` parameter.
#[test]
fn an_imported_let_cannot_be_a_mut_argument() {
    assert_rejects(
        "use lib::K\n\nfn bump(mut n: I32) { n = n + 1 }\n\npub fn f() -> I32 {\n    bump(n: K)\n    K\n}\n",
        &[("lib.fv", "pub let K: I32 = 1\n")],
        Some("MutabilityMismatch"),
    );
}

/// An imported `let` keeps its value.
#[test]
fn an_imported_let_keeps_its_value() {
    assert_runs(
        "use lib::K\n\npub fn run_checks() {\n    assert(condition: K == 5)\n}\n",
        &[("lib.fv", "pub let K: I32 = 5\n")],
    );
}

/// An imported `let` whose value reads a private `let` of its module.
#[test]
fn an_imported_let_reads_a_private_let() {
    assert_runs(
        "use lib::K\n\npub fn run_checks() {\n    assert(condition: K == 6)\n}\n",
        &[("lib.fv", "let BASE: I32 = 5\npub let K: I32 = BASE + 1\n")],
    );
}

/// An imported enum. A `.variant` with the imported type as its context.
#[test]
fn an_imported_enum_variant_by_its_short_form() {
    assert_runs(
        "use lib::Status\n\npub fn run_checks() {\n    let s: Status = .busy\n    assert(condition: s == Status.busy)\n}\n",
        &[("lib.fv", "pub enum Status { idle, busy }\n")],
    );
}

/// An imported function that returns a type from a third module.
#[test]
fn an_imported_function_returns_a_type_of_a_third_module() {
    assert_runs(
        "use mid::make\n\npub fn run_checks() {\n    assert(condition: make().x == 3)\n}\n",
        &[
            ("leaf.fv", "pub struct P { x: I32 }\n"),
            ("mid.fv", "use leaf::P\npub fn make() -> P { P(x: 3) }\n"),
        ],
    );
}

/// An imported trait, implemented by a local struct.
#[test]
fn an_imported_trait_is_implemented_locally() {
    assert_compiles(
        "use lib::Named\n\npub struct S { name: String }\n\nimpl Named for S { }\n",
        &[("lib.fv", "pub trait Named { name: String }\n")],
    );
}

/// An imported trait whose field is missing in the local struct.
#[test]
fn an_imported_trait_checks_its_fields() {
    assert_rejects(
        "use lib::Named\n\npub struct S { id: I32 }\n\nimpl Named for S { }\n",
        &[("lib.fv", "pub trait Named { name: String }\n")],
        Some("MissingTraitField"),
    );
}

/// An imported function checks its argument types.
#[test]
fn an_imported_function_checks_its_argument_types() {
    assert_rejects(
        "use lib::api\n\npub fn f() -> I32 { api(n: \"x\") }\n",
        &[("lib.fv", "pub fn api(n: I32) -> I32 { n }\n")],
        Some("TypeMismatch"),
    );
}

/// An imported function checks its argument count.
#[test]
fn an_imported_function_checks_its_argument_count() {
    assert_rejects(
        "use lib::api\n\npub fn f() -> I32 { api(n: 1, m: 2) }\n",
        &[("lib.fv", "pub fn api(n: I32) -> I32 { n }\n")],
        None,
    );
}

/// An imported function keeps its `mut` convention.
#[test]
fn an_imported_function_keeps_its_mut_convention() {
    assert_rejects(
        "use lib::bump\n\npub fn f() -> I32 {\n    let x: I32 = 1\n    bump(n: x)\n    x\n}\n",
        &[("lib.fv", "pub fn bump(mut n: I32) { n = n + 1 }\n")],
        Some("MutabilityMismatch"),
    );
}

/// An imported function keeps its `sink` convention.
#[test]
fn an_imported_function_keeps_its_sink_convention() {
    assert_rejects(
        "use lib::take\n\npub fn f() -> I32 {\n    let s: String = \"x\"\n    let a = take(s: s)\n    a + s.len()\n}\n",
        &[("lib.fv", "pub fn take(sink s: String) -> I32 { s.len() }\n")],
        Some("UseAfterSink"),
    );
}

/// An imported function with two `mut` parameters follows the rule of
/// exclusive access.
#[test]
fn an_imported_function_follows_exclusive_access() {
    assert_rejects(
        "use lib::swap\n\npub fn f() -> I32 {\n    let mut x: I32 = 1\n    swap(a: x, b: x)\n    x\n}\n",
        &[("lib.fv", "pub fn swap(mut a: I32, mut b: I32) { }\n")],
        Some("OverlappingArguments"),
    );
}

/// A struct from a module, used as a dictionary key in the importer.
#[test]
fn an_imported_struct_with_a_float_field_is_not_a_key() {
    assert_rejects(
        "use lib::K\n\npub fn f() -> I32 {\n    let d: [K: I32] = [:]\n    d.len()\n}\n",
        &[("lib.fv", "pub struct K { x: F64 }\n")],
        None,
    );
}

/// A module in a subdirectory, by its path.
#[test]
fn a_module_in_a_subdirectory() {
    assert_runs(
        "use utils::helpers::double\n\npub fn run_checks() {\n    assert(condition: double(n: 4) == 8)\n}\n",
        &[("utils/helpers.fv", "pub fn double(n: I32) -> I32 { n * 2 }\n")],
    );
}

/// A module in a subdirectory that imports a module next to it.
#[test]
fn a_module_in_a_subdirectory_imports_its_neighbour() {
    assert_runs(
        "use utils::helpers::double\n\npub fn run_checks() {\n    assert(condition: double(n: 4) == 9)\n}\n",
        &[
            ("utils/base.fv", "pub fn one() -> I32 { 1 }\n"),
            ("utils/helpers.fv", "use utils::base::one\npub fn double(n: I32) -> I32 { n * 2 + one() }\n"),
        ],
    );
}

/// An imported module sees the prelude, as the entry module does. The
/// prelude declares `len` on `String`.
#[test]
fn an_imported_module_sees_the_prelude() {
    assert_runs(
        "use lib::size\n\npub fn run_checks() {\n    assert(condition: size(s: \"abc\") == 3)\n}\n",
        &[("lib.fv", "pub fn size(s: String) -> I32 { s.len() }\n")],
    );
}

/// An imported module may call `assert` from the prelude.
#[test]
fn an_imported_module_may_call_the_prelude_assert() {
    assert_runs(
        "use lib::check\n\npub fn run_checks() {\n    check(n: 1)\n}\n",
        &[(
            "lib.fv",
            "pub fn check(n: I32) {\n    assert(condition: n == 1)\n}\n",
        )],
    );
}

/// A method of an imported struct calls a private function of the
/// struct's module. The entry has a function of the same name, and the
/// method must not call it.
#[test]
fn an_imported_method_calls_its_own_modules_helper() {
    assert_runs(
        "use lib::S\n\nfn helper() -> I32 { 100 }\n\npub fn run_checks() {\n    assert(condition: S(x: 1).get() == 2)\n    assert(condition: helper() == 100)\n}\n",
        &[(
            "lib.fv",
            "fn helper() -> I32 { 1 }\npub struct S { x: I32 }\nimpl S {\n    fn get(self) -> I32 { self.x + helper() }\n}\n",
        )],
    );
}
