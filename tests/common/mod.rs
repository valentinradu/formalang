//! Shared test helpers. Not a public API — only consumed by integration
//! tests in this `tests/` directory.
//!
//! Rust's integration-test layout compiles each `tests/*.rs` as a
//! separate crate; helpers live here under `tests/common/` and are
//! included via `#[path = "common/mod.rs"] mod common;` at the top of
//! each test file that needs them.

#![allow(dead_code, unreachable_pub)]

#[path = "interpreter.rs"]
pub mod interpreter;

#[path = "matrix.rs"]
pub mod matrix;

use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};
use std::collections::HashMap;
use std::path::PathBuf;

/// In-memory resolver for module-loading tests. Previously duplicated
/// across three test files — consolidated per audit finding #54.
pub struct MemResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MemResolver {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn add(&mut self, path: Vec<String>, source: &str) {
        let file_path = PathBuf::from(format!("{}.forma", path.join("/")));
        self.modules.insert(path, (source.to_string(), file_path));
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
                searched_paths: vec![],
            })
    }
}

/// A counter that fails the test when too few checks actually ran.
///
/// Many tests here walk a corpus and skip whatever does not apply —
/// an example that no longer compiles, a name a fixture does not
/// declare. That is the right shape, but it means the test passes
/// when the corpus is empty, when the path is wrong, or when every
/// item started failing for an unrelated reason. The assertions then
/// stop protecting anything, silently.
///
/// Wrap such a loop in a `Checked`: call [`Checked::hit`] each time an
/// assertion really ran, and the floor is enforced when the guard
/// drops at the end of the test.
///
/// ```ignore
/// let mut checked = Checked::new("examples compiled", 20);
/// for (name, source) in examples() {
///     let Ok(module) = compile_to_ir(&source) else { continue };
///     assert!(...);
///     checked.hit();
/// }
/// ```
pub struct Checked {
    label: &'static str,
    count: usize,
    floor: usize,
}

impl Checked {
    /// A guard that requires at least `floor` checks.
    pub const fn new(label: &'static str, floor: usize) -> Self {
        Self {
            label,
            count: 0,
            floor,
        }
    }

    /// Record that one check ran.
    pub const fn hit(&mut self) {
        self.count = self.count.saturating_add(1);
    }

    /// How many checks have run so far.
    pub const fn count(&self) -> usize {
        self.count
    }
}

impl std::fmt::Debug for Checked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checked")
            .field("label", &self.label)
            .field("count", &self.count)
            .field("floor", &self.floor)
            .finish()
    }
}

impl Drop for Checked {
    fn drop(&mut self) {
        // A test that is already failing gets one clear report, not two:
        // panicking while unwinding aborts the process.
        if std::thread::panicking() {
            return;
        }
        assert!(
            self.count >= self.floor,
            "{}: only {} check(s) ran, expected at least {}. The assertions in \
             this test are not reaching anything.",
            self.label,
            self.count,
            self.floor
        );
    }
}

/// Run `body` on a thread with a large stack.
///
/// Recursive work — parsing an expression, evaluating one — uses far
/// more stack per frame in a debug build than a release one, and more
/// again under `cargo llvm-cov` instrumentation. A test that recurses
/// deeply overflows the default 2 MB test-thread stack and aborts the
/// whole process, which loses the diagnostic: the run reports only
/// "process didn't exit successfully". On a thread sized for the work,
/// the interpreter's own depth guard fires first and names the program
/// that ran away. Compilers solve this the same way.
#[expect(
    clippy::expect_used,
    reason = "a worker thread that cannot start, or that panics, is a broken \
              test harness rather than a test failure"
)]
pub fn with_a_large_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    const STACK: usize = 64 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(body)
        .expect("the worker thread must start")
        .join()
        .expect("the worker thread must not panic")
}
