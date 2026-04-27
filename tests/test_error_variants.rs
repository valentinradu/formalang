//! Tests that trigger specific `CompilerError` variants from real source code.
//!
//! Many variants are covered implicitly through broader tests or only via
//! `Display` tests in `test_ir_lower_modules.rs`. This file adds focused
//! end-to-end triggers for variants that were only lightly covered prior to
//! the Phase 5 audit fix pass.
//!
//! Not included: `TooManyDefinitions` (only fires at `u32::MAX` definitions,
//! impractical to construct) and `ModuleReadError` (already covered by
//! `module_resolution.rs`).

use formalang::CompilerError;

// =============================================================================
// ExpressionDepthExceeded — deep nesting exhausts MAX_EXPR_DEPTH (500)
// =============================================================================

fn compile(source: &str) -> Result<formalang::ast::File, Vec<formalang::CompilerError>> {
    formalang::compile_with_analyzer(source).map(|(file, _analyzer)| file)
}

#[test]
fn expression_depth_exceeded_triggers_on_deeply_nested_expr(
) -> Result<(), Box<dyn std::error::Error>> {
    // MAX_EXPR_DEPTH in the semantic validator is 500. We build a source with
    // >500 nested levels and run the compile on a dedicated thread with a
    // generous stack so the parser does not itself stack-overflow before the
    // validator can detect the depth.
    let depth: usize = 600;
    let mut src = String::from("struct A { x: I32 = ");
    for _ in 0..depth {
        src.push('(');
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }
    src.push_str(" }");

    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || compile(&src))
        .map_err(|e| format!("spawn: {e}"))?;
    let result = handle.join().map_err(|_| "thread panicked")?;
    let errors = result.err().ok_or("expected compilation error")?;
    let has_depth_err = errors
        .iter()
        .any(|e| matches!(e, CompilerError::ExpressionDepthExceeded { .. }));
    if !has_depth_err {
        return Err(format!("expected ExpressionDepthExceeded, got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// DuplicateMatchArm — same variant listed twice in a match
// =============================================================================

#[test]
fn duplicate_match_arm_triggers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, inactive }

        pub fn describe(s: Status) -> String {
            match s {
                .active: "on",
                .active: "on again",
                .inactive: "off"
            }
        }
    "#;
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_dup = errors.iter().any(
        |e| matches!(e, CompilerError::DuplicateMatchArm { variant, .. } if variant == "active"),
    );
    if !has_dup {
        return Err(format!("expected DuplicateMatchArm for 'active', got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// VariantArityMismatch — match arm binds wrong number of fields for a variant
// =============================================================================

#[test]
fn variant_arity_mismatch_too_few_bindings_in_match() -> Result<(), Box<dyn std::error::Error>> {
    // `.high(urgency)` binds one field, but `.high` has zero bindings here.
    let source = r#"
        enum Priority { low, high(urgency: I32) }

        pub fn describe(p: Priority) -> String {
            match p {
                .low: "low",
                .high(urgency, extra): "high"
            }
        }
    "#;
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_arity = errors.iter().any(
        |e| matches!(e, CompilerError::VariantArityMismatch { variant, .. } if variant == "high"),
    );
    if !has_arity {
        return Err(format!("expected VariantArityMismatch for 'high', got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// PrimitiveRedefinition — user definition reuses a primitive type name
// =============================================================================

#[test]
fn primitive_redefinition_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Number { x: I32 }
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_redef = errors.iter().any(
        |e| matches!(e, CompilerError::PrimitiveRedefinition { name, .. } if name == "Number"),
    );
    if !has_redef {
        return Err(format!("expected PrimitiveRedefinition for 'Number', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn primitive_redefinition_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum String { a, b }
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_redef = errors.iter().any(
        |e| matches!(e, CompilerError::PrimitiveRedefinition { name, .. } if name == "String"),
    );
    if !has_redef {
        return Err(format!("expected PrimitiveRedefinition for 'String', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn primitive_redefinition_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Boolean { }
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_redef = errors.iter().any(
        |e| matches!(e, CompilerError::PrimitiveRedefinition { name, .. } if name == "Boolean"),
    );
    if !has_redef {
        return Err(format!("expected PrimitiveRedefinition for 'Boolean', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn primitive_redefinition_function() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub fn Never() -> I32 { 0 }
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_redef = errors
        .iter()
        .any(|e| matches!(e, CompilerError::PrimitiveRedefinition { name, .. } if name == "Never"));
    if !has_redef {
        return Err(format!("expected PrimitiveRedefinition for 'Never', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn primitive_redefinition_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let Path: I32 = 5
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_redef = errors
        .iter()
        .any(|e| matches!(e, CompilerError::PrimitiveRedefinition { name, .. } if name == "Path"));
    if !has_redef {
        return Err(format!("expected PrimitiveRedefinition for 'Path', got {errors:?}").into());
    }
    Ok(())
}

#[test]
fn primitive_name_still_works_at_type_position() -> Result<(), Box<dyn std::error::Error>> {
    // Verify normal use of primitive names at type position still compiles.
    let source = r#"
        let x: I32 = 5
        let s: String = "hi"
        let b: Boolean = true
    "#;
    let result = compile(source);
    if result.is_err() {
        return Err(format!("expected clean compile, got {:?}", result.err()).into());
    }
    Ok(())
}

// =============================================================================
// InvalidIfCondition — condition is a plain number literal
// =============================================================================

#[test]
fn invalid_if_condition_number_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A {
            x: I32 = if 42 { 1 } else { 0 }
        }
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_if_err = errors
        .iter()
        .any(|e| matches!(e, CompilerError::InvalidIfCondition { .. }));
    if !has_if_err {
        return Err(format!("expected InvalidIfCondition, got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// ArrayDestructuringNotArray — destructure [a, b] = non_array
// =============================================================================

#[test]
fn array_destructuring_not_array_triggers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let [a, b] = 42
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_not_array = errors
        .iter()
        .any(|e| matches!(e, CompilerError::ArrayDestructuringNotArray { .. }));
    if !has_not_array {
        return Err(format!("expected ArrayDestructuringNotArray, got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// StructDestructuringNotStruct — destructure { x, y } = non_struct
// =============================================================================

#[test]
fn struct_destructuring_not_struct_triggers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let { x, y } = 42
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_not_struct = errors
        .iter()
        .any(|e| matches!(e, CompilerError::StructDestructuringNotStruct { .. }));
    if !has_not_struct {
        return Err(format!("expected StructDestructuringNotStruct, got {errors:?}").into());
    }
    Ok(())
}

// =============================================================================
// UnknownEnumVariant — .nonexistent on a known enum
// =============================================================================

#[test]
fn unknown_enum_variant_in_instantiation_triggers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        let s: Status = Status.nonexistent
    ";
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_unknown = errors.iter().any(
        |e| matches!(e, CompilerError::UnknownEnumVariant { variant, .. } if variant == "nonexistent"),
    );
    if !has_unknown {
        return Err(
            format!("expected UnknownEnumVariant for 'nonexistent', got {errors:?}").into(),
        );
    }
    Ok(())
}

#[test]
fn unknown_enum_variant_in_match_arm_triggers() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, inactive }

        pub fn describe(s: Status) -> String {
            match s {
                .active: "on",
                .inactive: "off",
                .bogus: "??"
            }
        }
    "#;
    let errors = compile(source).err().ok_or("expected compilation error")?;
    let has_unknown = errors.iter().any(
        |e| matches!(e, CompilerError::UnknownEnumVariant { variant, .. } if variant == "bogus"),
    );
    if !has_unknown {
        return Err(format!("expected UnknownEnumVariant for 'bogus', got {errors:?}").into());
    }
    Ok(())
}
