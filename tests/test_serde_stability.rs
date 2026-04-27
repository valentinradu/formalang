//! Tests for serde stability (#4)
//!
//! Verifies `format_version` field and that public AST types serialize/deserialize correctly.

use formalang::parse_only;

// =============================================================================
// format_version field exists and is set
// =============================================================================

#[test]
fn test_file_has_format_version() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Foo {
    x: I32
}
";
    let file = parse_only(source).map_err(|e| format!("{e:?}"))?;
    if file.format_version == 0 {
        return Err("format_version must be non-zero (currently 1)".into());
    }
    Ok(())
}

#[test]
fn test_empty_file_has_format_version() -> Result<(), Box<dyn std::error::Error>> {
    let file = parse_only("").map_err(|e| format!("{e:?}"))?;
    if file.format_version == 0 {
        return Err("even empty files must have a non-zero format_version".into());
    }
    Ok(())
}

// =============================================================================
// Round-trip serialization: File -> JSON -> File
// =============================================================================

#[test]
fn test_file_roundtrip_json() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub struct Point {
    x: I32,
    y: I32
}
impl Point {
    fn length(self) -> I32 {
        self.x + self.y
    }
}
";
    let original = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let json =
        serde_json::to_string(&original).map_err(|e| format!("serialization failed: {e}"))?;
    let restored: formalang::File =
        serde_json::from_str(&json).map_err(|e| format!("deserialization failed: {e}"))?;

    if original != restored {
        return Err("round-trip produced a different AST".into());
    }
    Ok(())
}

#[test]
fn test_file_roundtrip_preserves_format_version() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
trait Named {
    name: String
}
";
    let original = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let json =
        serde_json::to_string(&original).map_err(|e| format!("serialization failed: {e}"))?;
    let restored: formalang::File =
        serde_json::from_str(&json).map_err(|e| format!("deserialization failed: {e}"))?;

    if original.format_version != restored.format_version {
        return Err(format!(
            "format_version not preserved: {} -> {}",
            original.format_version, restored.format_version
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_enum_def_roundtrip_json() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub enum Status {
    Active,
    Inactive
}
";
    let original = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let json =
        serde_json::to_string(&original).map_err(|e| format!("serialization failed: {e}"))?;
    let restored: formalang::File =
        serde_json::from_str(&json).map_err(|e| format!("deserialization failed: {e}"))?;
    if original != restored {
        return Err("enum round-trip produced a different AST".into());
    }
    Ok(())
}

#[test]
fn test_extern_fn_roundtrip_json() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
struct Canvas { width: I32, height: I32 }
extern fn create() -> Canvas
";
    let original = parse_only(source).map_err(|e| format!("{e:?}"))?;
    let json =
        serde_json::to_string(&original).map_err(|e| format!("serialization failed: {e}"))?;
    let restored: formalang::File =
        serde_json::from_str(&json).map_err(|e| format!("deserialization failed: {e}"))?;
    if original != restored {
        return Err("extern fn round-trip produced a different AST".into());
    }
    Ok(())
}

// =============================================================================
// format_version appears in the JSON output
// =============================================================================

#[test]
fn test_format_version_in_json_output() -> Result<(), Box<dyn std::error::Error>> {
    let file = parse_only("").map_err(|e| format!("{e:?}"))?;
    let json = serde_json::to_string(&file).map_err(|e| format!("serialization failed: {e}"))?;
    if !json.contains("format_version") {
        return Err(format!("'format_version' key missing from JSON output: {json}").into());
    }
    Ok(())
}

// =============================================================================
// IR stability: IrModule round-trips via JSON
// =============================================================================

#[test]
fn test_ir_module_roundtrip_json() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub struct User {
    name: String,
    age: I32
}

pub enum Status { active, inactive }

pub trait Named {
    name: String
}

pub fn greet(user: User) -> String {
    user.name
}

impl User {
    fn describe(self) -> String {
        self.name
    }
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let json = serde_json::to_string(&module).map_err(|e| format!("serialize: {e}"))?;
    // Sanity: key fields appear in the JSON payload
    for expected in [
        "structs",
        "traits",
        "enums",
        "functions",
        "impls",
        "lets",
        "imports",
    ] {
        if !json.contains(expected) {
            return Err(format!("IR JSON missing '{expected}' key: {json}").into());
        }
    }
    // Round-trip: deserialize back and check it re-serializes identically.
    let restored: formalang::IrModule =
        serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;
    let json2 = serde_json::to_string(&restored).map_err(|e| format!("re-serialize: {e}"))?;
    if json != json2 {
        return Err("IrModule round-trip produced a different JSON payload".into());
    }
    Ok(())
}

#[test]
fn test_ir_closure_captures_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Closures include a `captures` field in the IR; verify it survives a round-trip.
    let source = r"
pub fn make_counter(sink n: I32) -> (I32) -> I32 {
    |x: I32| x + n
}
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let json = serde_json::to_string(&module).map_err(|e| format!("serialize: {e}"))?;
    if !json.contains("captures") {
        return Err(format!("'captures' key missing from IR JSON: {json}").into());
    }
    let restored: formalang::IrModule =
        serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;
    let json2 = serde_json::to_string(&restored).map_err(|e| format!("re-serialize: {e}"))?;
    if json != json2 {
        return Err("closure-capturing IrModule round-trip diverged".into());
    }
    Ok(())
}

// =============================================================================
// IR round-trip on a mixed-feature fixture (structs + enums + traits + impls)
// =============================================================================

#[test]
fn test_ir_round_trip_mixed_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
pub trait Named {
    name: String
}

pub struct User {
    name: String,
    age: I32
}

impl User {
    fn greet(self) -> String {
        self.name
    }
}

pub enum Status {
    active,
    banned(reason: String),
    pending(since: I32, note: String)
}

pub let default_age: I32 = 0
";
    let module = formalang::compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let json = serde_json::to_string(&module).map_err(|e| format!("serialize: {e}"))?;
    let restored: formalang::IrModule =
        serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;

    // Re-serialise and compare for byte-for-byte stability.
    let json2 = serde_json::to_string(&restored).map_err(|e| format!("re-serialize: {e}"))?;
    if json != json2 {
        return Err("mixed-feature IrModule round-trip diverged".into());
    }

    // Spot-check the structure: counts and at least one non-trivial variant.
    if restored.structs.len() != module.structs.len() {
        return Err("struct count changed across round-trip".into());
    }
    if restored.traits.len() != module.traits.len() {
        return Err("trait count changed across round-trip".into());
    }
    if restored.enums.len() != module.enums.len() {
        return Err("enum count changed across round-trip".into());
    }
    let enum_def = restored
        .enums
        .iter()
        .find(|e| e.name == "Status")
        .ok_or("Status enum missing after round-trip")?;
    let banned = enum_def
        .variants
        .iter()
        .find(|v| v.name == "banned")
        .ok_or("banned variant missing")?;
    if banned.fields.is_empty() {
        return Err("banned(reason: String) lost its field shape during round-trip".into());
    }
    Ok(())
}
