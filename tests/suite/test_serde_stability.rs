//! Tests for the IR serde (the `serde` feature).
//!
//! An `IrModule` survives a JSON round trip. The test build turns the
//! feature on through the dev-dependency on the crate itself.

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
    (x: I32) -> x + n
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
    let mut restored: formalang::IrModule =
        serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;
    // The private name->id index maps are `#[serde(skip)]`, so after a
    // round-trip the prelude lookups (`Array`, `Dictionary`, `Range`,
    // `Optional`) need to be rebuilt for `user_structs()`/`user_enums()`
    // to filter prelude built-ins correctly.
    restored.rebuild_indices();

    // Re-serialise and compare for byte-for-byte stability.
    let json2 = serde_json::to_string(&restored).map_err(|e| format!("re-serialize: {e}"))?;
    if json != json2 {
        return Err("mixed-feature IrModule round-trip diverged".into());
    }

    // Spot-check the structure: counts and at least one non-trivial variant.
    if restored.user_structs().count() != module.user_structs().count() {
        return Err("struct count changed across round-trip".into());
    }
    if restored.traits.len() != module.traits.len() {
        return Err("trait count changed across round-trip".into());
    }
    if restored.user_enums().count() != module.user_enums().count() {
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
