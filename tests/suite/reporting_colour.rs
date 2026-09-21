//! The renderer's use of the process-global colour switch.
//!
//! `yansi`'s switch is process-global, and ariadne re-exports
//! `yansi::Color`, so turning colour off is the only way to stop the
//! inline highlights inside label messages from emitting ANSI codes.
//! `NO_COLOR` is process-global too.
//!
//! These checks therefore live in their own file, and in a single
//! test. Rust gives each `tests/*.rs` file its own binary but runs the
//! tests inside one file on several threads, so a second test here
//! could see the first one's half-applied state.

use formalang::{compile_to_ir, report_errors, CompilerError};

/// Compile something that fails, so there is a diagnostic to render.
fn errors_for(source: &str) -> Vec<CompilerError> {
    compile_to_ir(source).err().unwrap_or_default()
}

/// Rendering must leave the colour setting as it found it, and must
/// honour `NO_COLOR` while it renders.
///
/// The renderer used to call `yansi::enable()` unconditionally on the
/// way out, which handed colour back to an embedder that had
/// deliberately turned it off.
#[test]
fn rendering_honours_no_color_and_restores_the_setting() {
    let source = "pub fn broken( {";
    let errors = errors_for(source);
    assert!(!errors.is_empty(), "the fixture must fail to compile");

    let original = yansi::is_enabled();

    // 1. An embedder that turned colour off, with NO_COLOR set, keeps
    //    it off.
    std::env::set_var("NO_COLOR", "1");
    yansi::disable();
    let quiet = report_errors(&errors, source, "input.fv");
    let stayed_disabled = !yansi::is_enabled();

    // 2. An embedder that wants colour keeps it, even though this
    //    render emits none because NO_COLOR is set.
    yansi::enable();
    let _ = report_errors(&errors, source, "input.fv");
    let stayed_enabled = yansi::is_enabled();

    // 3. Without NO_COLOR the setting is still untouched.
    std::env::remove_var("NO_COLOR");
    yansi::disable();
    let _ = report_errors(&errors, source, "input.fv");
    let stayed_disabled_without_the_variable = !yansi::is_enabled();

    // Put the process back before asserting, so a failure here cannot
    // leak into whatever runs next.
    if original {
        yansi::enable();
    } else {
        yansi::disable();
    }

    assert!(
        stayed_disabled,
        "rendering turned colour back on for the whole process"
    );
    assert!(
        stayed_enabled,
        "rendering turned colour off for the whole process"
    );
    assert!(
        stayed_disabled_without_the_variable,
        "rendering changed the colour setting even with NO_COLOR unset"
    );
    assert!(
        !quiet.contains('\u{1b}'),
        "NO_COLOR was set but the report still holds an ANSI escape"
    );
    assert!(!quiet.is_empty(), "the report must not be empty");
}
