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

use formalang::{compile_to_ir_with_resolver, report_errors, FileSystemResolver, Pipeline};
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

    // Drive the full canonical IR pipeline so `fvc check` only reports
    // OK on IR that is actually backend-consumable. Frontend-only
    // success (`compile_to_ir_with_resolver`) lets a number of
    // structural problems through — unresolved references, surviving
    // generic templates, ill-formed dispatch — that the downstream
    // passes (`ResolveReferencesPass`, the rest of `for_codegen`)
    // catch. Failing here means the IR isn't usable; reporting OK
    // now means it is.
    let module = match compile_to_ir_with_resolver(&source, resolver) {
        Ok(m) => m,
        Err(errors) => {
            eprintln!("{}", report_errors(&errors, &source, input_path));
            return ExitCode::from(1);
        }
    };
    match Pipeline::for_codegen().run(module) {
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

/// The state that `fvc watch` shares between its polling loop and the
/// Ctrl+C handler.
///
/// The handler runs on a separate thread, so both flags are atomic.
/// The type exists on its own — rather than as two locals inside
/// [`watch_command`] — so the protocol can be model-checked without
/// the surrounding filesystem polling. See `loom_watch` below.
///
/// Under `cfg(loom)` the atomics come from `loom`, which explores
/// every legal interleaving instead of the one the hardware happens
/// to produce.
mod watch_state {
    #[cfg(loom)]
    pub(super) use loom::sync::atomic::{AtomicBool, Ordering};
    #[cfg(not(loom))]
    pub(super) use std::sync::atomic::{AtomicBool, Ordering};

    /// Shutdown request plus the outcome of the most recent check.
    #[derive(Debug)]
    pub(super) struct WatchState {
        shutdown: AtomicBool,
        last_succeeded: AtomicBool,
    }

    impl WatchState {
        /// A fresh state: no shutdown requested, and no successful
        /// check yet.
        ///
        /// `last_succeeded` starts `false` so a Ctrl+C that arrives
        /// before the first check finishes reports a non-zero exit
        /// instead of a false success.
        #[cfg_attr(
            not(loom),
            expect(
                clippy::missing_const_for_fn,
                reason = "loom's AtomicBool::new is not const, so this cannot be either"
            )
        )]
        pub(super) fn new() -> Self {
            Self {
                shutdown: AtomicBool::new(false),
                last_succeeded: AtomicBool::new(false),
            }
        }

        /// Ask the loop to stop. Called from the signal handler.
        pub(super) fn request_shutdown(&self) {
            self.shutdown.store(true, Ordering::SeqCst);
        }

        /// Whether a shutdown has been requested.
        pub(super) fn is_shutting_down(&self) -> bool {
            self.shutdown.load(Ordering::SeqCst)
        }

        /// Record the outcome of a check. Called from the loop.
        pub(super) fn record(&self, succeeded: bool) {
            self.last_succeeded.store(succeeded, Ordering::SeqCst);
        }

        /// The process exit status: success only when the most recent
        /// completed check succeeded.
        pub(super) fn exit_is_success(&self) -> bool {
            self.last_succeeded.load(Ordering::SeqCst)
        }
    }
}

use watch_state::WatchState;

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
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    println!("Watching {input_path}... (Ctrl+C to stop)");

    let path = PathBuf::from(input_path);
    let mut last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

    let state = Arc::new(WatchState::new());
    let handler_state = Arc::clone(&state);
    if let Err(err) = ctrlc::set_handler(move || {
        handler_state.request_shutdown();
    }) {
        eprintln!("warning: failed to install Ctrl+C handler: {err}");
    }

    // Run an initial check so the user sees the current state immediately
    // instead of having to touch the file first.
    let initial = check_command(input_path, module_root.map(PathBuf::from));
    state.record(initial == ExitCode::SUCCESS);

    while !state.is_shutting_down() {
        thread::sleep(Duration::from_millis(500));
        if state.is_shutting_down() {
            break;
        }

        let current_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

        if current_modified != last_modified {
            last_modified = current_modified;
            println!("\n--- File changed, rechecking... ---\n");
            let result = check_command(input_path, module_root.map(PathBuf::from));
            state.record(result == ExitCode::SUCCESS);
        }
    }

    println!("\nfvc watch interrupted; returning last check's exit code");
    if state.exit_is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(all(test, not(loom)))]
mod watch_state_tests {
    use super::WatchState;

    /// A fresh state reports a failure, so a Ctrl+C before the first
    /// check finishes gives a non-zero exit.
    #[test]
    fn a_fresh_state_reports_failure() {
        let state = WatchState::new();
        assert!(!state.is_shutting_down());
        assert!(!state.exit_is_success());
    }

    /// The exit status follows the most recent recorded check.
    #[test]
    fn the_exit_status_follows_the_last_check() {
        let state = WatchState::new();
        state.record(true);
        assert!(state.exit_is_success());
        state.record(false);
        assert!(!state.exit_is_success());
        state.record(true);
        assert!(state.exit_is_success());
    }

    /// A shutdown request is not forgotten, and it does not touch the
    /// recorded outcome.
    #[test]
    fn a_shutdown_request_leaves_the_outcome_alone() {
        let state = WatchState::new();
        state.record(true);
        state.request_shutdown();
        assert!(state.is_shutting_down());
        assert!(state.exit_is_success());
    }
}

/// Model-check the watch protocol under `loom`.
///
/// Run with:
///
/// ```text
/// RUSTFLAGS="--cfg loom" cargo test --bin fvc loom_watch
/// ```
///
/// `loom` replaces the atomics in [`watch_state`] and explores every
/// interleaving the memory model allows, rather than the one this
/// machine happens to produce. Two properties matter:
///
/// - a shutdown request is never lost — the loop always sees it and
///   terminates;
/// - the exit status is never a false success — it reports success
///   only when a check actually recorded one.
#[cfg(all(test, loom))]
mod loom_watch {
    use super::WatchState;
    use loom::sync::Arc;
    use loom::thread;

    /// The signal handler stores `shutdown` while the loop is polling
    /// it. Under every interleaving the loop must observe the request.
    #[test]
    fn a_shutdown_request_is_never_lost() {
        loom::model(|| {
            let state = Arc::new(WatchState::new());
            let handler = Arc::clone(&state);

            let signal = thread::spawn(move || {
                handler.request_shutdown();
            });

            // The loop body: poll until the request arrives. `loom`
            // bounds the number of iterations, so a bounded poll is
            // the right shape for the model.
            let mut observed = state.is_shutting_down();
            signal.join().expect("the signal thread must not panic");
            if !observed {
                observed = state.is_shutting_down();
            }

            assert!(
                observed,
                "the loop did not observe the shutdown request after the \
                 handler returned"
            );
        });
    }

    /// The handler and a check racing each other must not produce a
    /// success that no check recorded.
    #[test]
    fn the_exit_status_is_never_a_false_success() {
        loom::model(|| {
            let state = Arc::new(WatchState::new());
            let handler = Arc::clone(&state);

            let signal = thread::spawn(move || {
                handler.request_shutdown();
            });

            // The loop records a failing check while the signal is in
            // flight, then reads the exit status.
            state.record(false);
            let exit = state.exit_is_success();

            signal.join().expect("the signal thread must not panic");

            assert!(
                !exit,
                "the exit status reported success although the only \
                 recorded check failed"
            );
        });
    }

    /// The two flags are independent: a shutdown request must never
    /// overwrite the recorded outcome.
    #[test]
    fn a_shutdown_request_does_not_clobber_the_outcome() {
        loom::model(|| {
            let state = Arc::new(WatchState::new());
            let handler = Arc::clone(&state);

            state.record(true);

            let signal = thread::spawn(move || {
                handler.request_shutdown();
            });
            signal.join().expect("the signal thread must not panic");

            assert!(
                state.exit_is_success(),
                "a shutdown request cleared the successful check"
            );
        });
    }
}
