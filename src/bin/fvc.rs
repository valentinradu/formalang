//! `FormaLang` Compiler CLI
//!
//! Usage:
//!   fvc check <file.fv> [--stdlib-path <path>]
//!   fvc watch <file.fv> [--stdlib-path <path>]

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

    let stdlib_path = parse_stdlib_path(&args);

    match args.get(1).map(String::as_str) {
        Some("check") => {
            let Some(input_path) = args.get(2) else {
                eprintln!("Error: Missing input file");
                print_usage();
                return ExitCode::from(1);
            };
            check_command(input_path, stdlib_path)
        }
        Some("watch") => {
            let Some(input_path) = args.get(2) else {
                eprintln!("Error: Missing input file");
                print_usage();
                return ExitCode::from(1);
            };
            watch_command(input_path, stdlib_path.as_deref())
        }
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

fn print_usage() {
    println!("FormaLang Compiler v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  fvc check <file.fv> [--stdlib-path <path>]");
    println!("  fvc watch <file.fv> [--stdlib-path <path>]");
    println!("  fvc help                               Show this help");
    println!("  fvc version                            Show version");
    println!();
    println!("Options:");
    println!("  --stdlib-path <path>  Root path for stdlib resolution");
}

fn parse_stdlib_path(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--stdlib-path" {
            if let Some(path) = iter.next() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn resolve_base_dir(input_path: &str, stdlib_path: Option<PathBuf>) -> PathBuf {
    stdlib_path.unwrap_or_else(|| {
        PathBuf::from(input_path)
            .parent()
            .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf)
    })
}

fn check_command(input_path: &str, stdlib_path: Option<PathBuf>) -> ExitCode {
    let start = Instant::now();
    println!("Checking {input_path}...");

    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {input_path}: {e}");
            return ExitCode::from(1);
        }
    };

    let resolver = FileSystemResolver::new(resolve_base_dir(input_path, stdlib_path));

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

fn watch_command(input_path: &str, stdlib_path: Option<&std::path::Path>) -> ExitCode {
    use std::thread;
    use std::time::Duration;

    println!("Watching {input_path}... (Ctrl+C to stop)");

    let path = PathBuf::from(input_path);
    let mut last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

    loop {
        thread::sleep(Duration::from_millis(500));

        let current_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

        if current_modified != last_modified {
            last_modified = current_modified;
            println!("\n--- File changed, rechecking... ---\n");
            check_command(input_path, stdlib_path.map(PathBuf::from));
        }
    }
}
