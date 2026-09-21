//! The `fvc` command line, driven end to end.
//!
//! `fvc` is what a user runs. Its argument parsing, its exit codes and
//! its output are the contract, and none of it was covered: the binary
//! sat at a fifth of its lines executed.
//!
//! Each test runs the real binary that `cargo test` built, through
//! `CARGO_BIN_EXE_fvc`, over a file in a temporary directory.
//!
//! `fvc watch` is not tested here. It polls the filesystem until it
//! receives SIGINT, so an end-to-end test of it would be a timing
//! test. Its shutdown protocol is model-checked instead — see
//! `loom_watch` in `src/bin/fvc.rs`.

#![expect(
    clippy::expect_used,
    reason = "a CLI test reports a failed invocation by failing loudly"
)]

use crate::common::Checked;

use std::path::Path;
use std::process::{Command, Output};

/// Run `fvc` with `args` and return what it produced.
fn fvc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fvc"))
        .args(args)
        .output()
        .expect("the fvc binary must run")
}

/// `fvc`'s stdout, as text.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `fvc`'s stderr, as text.
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Write `source` to `name` inside `dir` and return the path.
fn write(dir: &Path, name: &str, source: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("the fixture must be writable");
    path.to_string_lossy().into_owned()
}

const GOOD: &str = "pub struct A {\n    a: I32\n}\n\npub fn f(v: A) -> I32 {\n    v.a\n}\n";
const BAD: &str = "pub fn f() -> I32 {\n    missing_name\n}\n";

// ---------------------------------------------------------------------------
// check
// ---------------------------------------------------------------------------

/// A program that compiles exits zero and reports what it found.
#[test]
fn check_accepts_a_valid_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = write(dir.path(), "good.fv", GOOD);

    let output = fvc(&["check", &path]);
    assert!(
        output.status.success(),
        "check failed on a valid file:\n{}",
        stderr(&output)
    );
    let text = stdout(&output);
    assert!(
        text.contains("OK:"),
        "check succeeded but did not say so:\n{text}"
    );
    assert!(
        text.contains("structs"),
        "the summary did not report the definition counts:\n{text}"
    );
}

/// A program that does not compile exits non-zero and explains why.
#[test]
fn check_rejects_an_invalid_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = write(dir.path(), "bad.fv", BAD);

    let output = fvc(&["check", &path]);
    assert!(
        !output.status.success(),
        "check accepted a file that does not compile"
    );
    let text = stderr(&output);
    assert!(
        text.contains("missing_name"),
        "the diagnostic did not name the undefined reference:\n{text}"
    );
}

/// A file that is not there is an error, not a crash.
#[test]
fn check_reports_a_missing_file() {
    let output = fvc(&["check", "no/such/file.fv"]);
    assert!(!output.status.success(), "a missing file exited zero");
    assert!(
        stderr(&output).contains("Error reading"),
        "a missing file produced no explanation:\n{}",
        stderr(&output)
    );
}

/// `--module-root` resolves an import against the directory it names.
#[test]
fn check_resolves_imports_against_the_module_root() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let lib = dir.path().join("lib");
    std::fs::create_dir(&lib).expect("the lib directory must be creatable");
    write(&lib, "shapes.fv", "pub struct Point {\n    x: I32\n}\n");

    let nested = dir.path().join("src");
    std::fs::create_dir(&nested).expect("the src directory must be creatable");
    let main = write(
        &nested,
        "main.fv",
        "use shapes::Point\n\npub fn f() -> I32 {\n    Point(x: 1).x\n}\n",
    );

    // Without the root the import cannot resolve.
    let without = fvc(&["check", &main]);
    assert!(
        !without.status.success(),
        "the import resolved without a module root"
    );

    // With it, it does.
    let with = fvc(&["check", &main, "--module-root", &lib.to_string_lossy()]);
    assert!(
        with.status.success(),
        "the import did not resolve with --module-root:\n{}",
        stderr(&with)
    );
}

/// `--module-root` is accepted before the file as well as after it.
#[test]
fn the_module_root_flag_works_on_either_side_of_the_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = write(dir.path(), "good.fv", GOOD);
    let root = dir.path().to_string_lossy().into_owned();

    let after = fvc(&["check", &path, "--module-root", &root]);
    let before = fvc(&["check", "--module-root", &root, &path]);
    assert_eq!(
        after.status.success(),
        before.status.success(),
        "the flag behaved differently before and after the file"
    );
    assert!(after.status.success(), "both forms should have succeeded");
}

/// `--module-root` with nothing after it is an error, not a panic.
#[test]
fn a_module_root_without_a_value_is_rejected() {
    let output = fvc(&["check", "x.fv", "--module-root"]);
    assert!(!output.status.success(), "the missing value exited zero");
    assert!(
        stderr(&output).contains("--module-root requires a path"),
        "the missing value produced no explanation:\n{}",
        stderr(&output)
    );
}

/// An unknown flag is reported, not ignored.
#[test]
fn an_unknown_flag_is_rejected() {
    let output = fvc(&["check", "x.fv", "--nonsense"]);
    assert!(!output.status.success(), "an unknown flag exited zero");
    assert!(
        stderr(&output).contains("unknown flag"),
        "an unknown flag produced no explanation:\n{}",
        stderr(&output)
    );
}

/// A second positional argument is reported.
#[test]
fn an_extra_argument_is_rejected() {
    let output = fvc(&["check", "a.fv", "b.fv"]);
    assert!(!output.status.success(), "an extra argument exited zero");
    assert!(
        stderr(&output).contains("unexpected extra argument"),
        "an extra argument produced no explanation:\n{}",
        stderr(&output)
    );
}

/// `check` with no file prints its help and exits non-zero.
#[test]
fn check_without_a_file_is_rejected() {
    let output = fvc(&["check"]);
    assert!(!output.status.success(), "a missing file exited zero");
    assert!(
        stderr(&output).contains("Missing input file"),
        "a missing file produced no explanation:\n{}",
        stderr(&output)
    );
}

// ---------------------------------------------------------------------------
// help and version
// ---------------------------------------------------------------------------

/// Every help form prints usage and exits zero.
#[test]
fn every_help_form_prints_usage() {
    for args in [
        vec!["help"],
        vec!["--help"],
        vec!["-h"],
        vec!["check", "--help"],
        vec!["check", "-h"],
        vec!["watch", "--help"],
    ] {
        let output = fvc(&args);
        assert!(
            output.status.success(),
            "{args:?} exited non-zero:\n{}",
            stderr(&output)
        );
        let text = stdout(&output);
        assert!(
            text.contains("Usage:"),
            "{args:?} printed no usage:\n{text}"
        );
    }
}

/// A subcommand's help is recognised wherever it appears.
#[test]
fn a_help_flag_is_recognised_after_the_file() {
    let output = fvc(&["check", "x.fv", "--help"]);
    assert!(
        output.status.success(),
        "a trailing --help exited non-zero:\n{}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("fvc check"),
        "a trailing --help printed the wrong help:\n{}",
        stdout(&output)
    );
}

/// Both version forms print the crate version.
#[test]
fn every_version_form_prints_the_version() {
    for args in [vec!["version"], vec!["--version"], vec!["-v"]] {
        let output = fvc(&args);
        assert!(output.status.success(), "{args:?} exited non-zero");
        let text = stdout(&output);
        assert!(
            text.contains(env!("CARGO_PKG_VERSION")),
            "{args:?} printed {text:?}, which does not hold the version"
        );
    }
}

// ---------------------------------------------------------------------------
// no subcommand, unknown subcommand
// ---------------------------------------------------------------------------

/// Running `fvc` with no arguments prints usage and exits non-zero.
#[test]
fn no_arguments_prints_usage_and_fails() {
    let output = fvc(&[]);
    assert!(!output.status.success(), "no arguments exited zero");
    assert!(
        stdout(&output).contains("Usage:"),
        "no arguments printed no usage"
    );
}

/// An unknown subcommand names itself in the error.
#[test]
fn an_unknown_subcommand_is_rejected() {
    let output = fvc(&["frobnicate"]);
    assert!(
        !output.status.success(),
        "an unknown subcommand exited zero"
    );
    assert!(
        stderr(&output).contains("frobnicate"),
        "the error did not name the subcommand:\n{}",
        stderr(&output)
    );
}

// ---------------------------------------------------------------------------
// Every example
// ---------------------------------------------------------------------------

/// `fvc check` accepts every example in the tree.
///
/// The examples are the documented programs. If one stops passing the
/// full codegen pipeline that `check` runs, a reader following the
/// documentation hits the failure before we do.
#[test]
fn check_accepts_every_example() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let Ok(entries) = std::fs::read_dir(&examples) else {
        return;
    };
    let mut checked = Checked::new("examples checked by fvc", 20);
    let mut failed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("fv") {
            continue;
        }
        let output = fvc(&["check", &path.to_string_lossy()]);
        if !output.status.success() {
            failed.push(format!(
                "{}: {}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                stderr(&output)
            ));
        }
        checked.hit();
    }
    assert!(
        failed.is_empty(),
        "{} example(s) failed `fvc check`:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
