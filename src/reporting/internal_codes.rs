//! Sub-classifier for `CompilerError::InternalError` codes.

/// Audit2 B32: pick a specific internal-error code based on the leading
/// subsystem prefix on the `detail` string. Returns `"E999"` (generic)
/// for any prefix the table doesn't know.
pub(super) fn internal_error_code(detail: &str) -> &'static str {
    if detail.starts_with("IR lowering:") {
        "E931"
    } else if detail.starts_with("monomorphise:") {
        "E932"
    } else if detail.contains("id ") && detail.contains("registration lookup") {
        "E933"
    } else if detail.contains("inferred-enum") {
        "E934"
    } else {
        "E999"
    }
}
