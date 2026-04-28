//! `FormaLang` Compiler CLI
//!
//! Usage:
//!   `fvc check <file.fv> [--module-root <path>]`
//!   `fvc watch <file.fv> [--module-root <path>]`

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI binary: printing to stdout/stderr is the intended output mechanism"
)]

use formalang::{compile_to_ir_with_resolver, report_errors, FileSystemResolver};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let subcommand_args: &[String] = args.get(2..).unwrap_or(&[]);

    match args.get(1).map(String::as_str) {
        Some("check") => run_subcommand(
            subcommand_args,
            "check",
            check_subcommand_help,
            |path, root| check_command(path, root.map(PathBuf::from)),
        ),
        Some("watch") => run_subcommand(
            subcommand_args,
            "watch",
            watch_subcommand_help,
            |path, root| watch_command(path, root.map(std::path::Path::new)),
        ),
        Some("help" | "--help" | "-h") => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some("version" | "--version" | "-v") => {
            println!("fvc {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(cmd) => {
            eprintln!("Error: Unknown command '{cmd}'");
            print_usage();
            ExitCode::from(1)
        }
        None => {
            print_usage();
            ExitCode::from(1)
        }
    }
}

/// parse a subcommand's args without assuming a fixed
/// positional index. Recognises `-h` / `--help` *anywhere* among the
/// args (not just at index 0) and accepts `--module-root <path>` either
/// before or after the input file.
fn run_subcommand(
    args: &[String],
    subcommand: &str,
    print_help: fn(),
    run: impl FnOnce(&str, Option<&str>) -> ExitCode,
) -> ExitCode {
    let mut input: Option<&str> = None;
    let mut module_root: Option<&str> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "--module-root" => {
                if let Some(value) = iter.next() {
                    module_root = Some(value.as_str());
                } else {
                    eprintln!("Error: --module-root requires a path argument");
                    print_help();
                    return ExitCode::from(1);
                }
            }
            other if other.starts_with("--") => {
                eprintln!("Error: unknown flag '{other}' for `fvc {subcommand}`");
                print_help();
                return ExitCode::from(1);
            }
            other => {
                if input.is_some() {
                    eprintln!("Error: unexpected extra argument '{other}'");
                    print_help();
                    return ExitCode::from(1);
                }
                input = Some(other);
            }
        }
    }
    input.map_or_else(
        || {
            eprintln!("Error: Missing input file");
            print_help();
            ExitCode::from(1)
        },
        |path| run(path, module_root),
    )
}

fn print_usage() {
    println!("FormaLang Compiler v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  fvc check <file.fv> [--module-root <path>]");
    println!("  fvc watch <file.fv> [--module-root <path>]");
    println!("  fvc help                               Show this help");
    println!("  fvc version                            Show version");
    println!();
    println!("Options:");
    println!("  --module-root <path>  Root directory for `use` resolution");
    println!();
    println!("Run `fvc <subcommand> --help` for subcommand-specific help.");
}

fn check_subcommand_help() {
    println!("fvc check — type-check a FormaLang source file");
    println!();
    println!("Usage:");
    println!("  fvc check <file.fv> [--module-root <path>]");
    println!();
    println!("Options:");
    println!("  --module-root <path>  Root directory for `use` resolution");
    println!("                        (default: parent of <file.fv>)");
    println!("  -h, --help            Show this help");
}

fn watch_subcommand_help() {
    println!("fvc watch — re-check a FormaLang source file on every change");
    println!();
    println!("Usage:");
    println!("  fvc watch <file.fv> [--module-root <path>]");
    println!();
    println!("Press Ctrl+C to stop. Exit code reflects the last check's result.");
    println!();
    println!("Options:");
    println!("  --module-root <path>  Root directory for `use` resolution");
    println!("                        (default: parent of <file.fv>)");
    println!("  -h, --help            Show this help");
}

fn resolve_base_dir(input_path: &str, module_root: Option<PathBuf>) -> PathBuf {
    module_root.unwrap_or_else(|| {
        PathBuf::from(input_path)
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf)
    })
}

fn check_command(input_path: &str, module_root: Option<PathBuf>) -> ExitCode {
    let start = Instant::now();
    println!("Checking {input_path}...");

    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {input_path}: {e}");
            return ExitCode::from(1);
        }
    };

    let resolver = FileSystemResolver::new(resolve_base_dir(input_path, module_root));

    match compile_to_ir_with_resolver(&source, resolver) {
        Ok(ir) => {
            let duration = start.elapsed();
            println!(
                "OK: {} structs, {} traits, {} enums ({:.2}ms)",
                ir.structs.len(),
                ir.traits.len(),
                ir.enums.len(),
                duration.as_secs_f64() * 1000.0
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprintln!("{}", report_errors(&errors, &source, input_path));
            ExitCode::from(1)
        }
    }
}

/// Watch mode: re-run `check` every time the input file's mtime changes.
///
/// This subcommand is intended for interactive development. It loops
/// (polling the filesystem every 500 ms) until the user sends SIGINT
/// (Ctrl+C). Each recheck prints its OK/error output and the loop
/// continues. On exit the process returns the last check's exit code,
/// so a CI runner that wraps `fvc watch` and signals it on shutdown
/// will still see a nonzero exit when the most recent check failed.
///
fn watch_command(input_path: &str, module_root: Option<&std::path::Path>) -> ExitCode {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    println!("Watching {input_path}... (Ctrl+C to stop)");

    let path = PathBuf::from(input_path);
    let mut last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

    // Atomics: the signal handler runs on a separate thread. Default
    // `last_succeeded` to `false` so a Ctrl+C before the first check
    // completes reports a non-zero exit instead of a false success.
    let shutdown = Arc::new(AtomicBool::new(false));
    let last_succeeded = Arc::new(AtomicBool::new(false));
    let shutdown_handler = Arc::clone(&shutdown);
    if let Err(err) = ctrlc::set_handler(move || {
        shutdown_handler.store(true, Ordering::SeqCst);
    }) {
        eprintln!("warning: failed to install Ctrl+C handler: {err}");
    }

    // `check_command` returns either `ExitCode::SUCCESS` or
    // `ExitCode::from(1)`; collapse to a bool so we can stash it in an
    // AtomicBool without round-tripping through ExitCode's opaque
    // representation.
    let record = |code: ExitCode, last: &AtomicBool| {
        let ok = code == ExitCode::SUCCESS;
        last.store(ok, Ordering::SeqCst);
    };

    // Run an initial check so the user sees the current state immediately
    // instead of having to touch the file first.
    let initial = check_command(input_path, module_root.map(PathBuf::from));
    record(initial, &last_succeeded);

    while !shutdown.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(500));
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        let current_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

        if current_modified != last_modified {
            last_modified = current_modified;
            println!("\n--- File changed, rechecking... ---\n");
            let result = check_command(input_path, module_root.map(PathBuf::from));
            record(result, &last_succeeded);
        }
    }

    println!("\nfvc watch interrupted; returning last check's exit code");
    if last_succeeded.load(Ordering::SeqCst) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
