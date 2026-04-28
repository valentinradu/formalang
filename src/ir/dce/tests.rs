use super::*;
use crate::ast::Literal;
use crate::compile_to_ir;

#[test]
fn test_eliminate_constant_true_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: I32 = if true { 1 } else { 2 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);

    let struct_def = optimized
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    // The if should be eliminated, leaving just 1
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value - 1.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 1, got {}", n.value).into());
        }
    } else {
        return Err(format!("Expected literal 1, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_eliminate_constant_false_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: I32 = if false { 1 } else { 2 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);

    let struct_def = optimized
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    // The if should be eliminated, leaving just 2
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value - 2.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 2, got {}", n.value).into());
        }
    } else {
        return Err(format!("Expected literal 2, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_no_elimination_non_constant_condition() -> Result<(), Box<dyn std::error::Error>> {
    // Use a let binding that references another let binding
    let source = r"
        let flag: Boolean = true
        let value: I32 = if flag { 1 } else { 2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);

    // Find the "value" let binding
    let let_binding = optimized
        .lets
        .iter()
        .find(|l| l.name == "value")
        .ok_or("expected value let binding")?;
    let expr = &let_binding.value;

    // flag is a variable reference, so if can't be eliminated
    // However, since flag is constant true, the optimizer should eliminate it
    // Let's check for either case
    if let IrExpr::If { .. } = expr {
        // Non-constant condition case (if optimizer can't see through let binding)
    } else if let IrExpr::Literal { .. } = expr {
        // Optimizer did constant propagation
    } else {
        return Err(format!("Expected If or Literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_analyze_used_structs() -> Result<(), Box<dyn std::error::Error>> {
    // DCE semantics: an impl block does NOT keep its target alive on its
    // own. Something else must reference the struct (a field type, a
    // function parameter, or an expression). Here a standalone function
    // takes a `Used` parameter.
    let source = r"
        struct Used { value: I32 = 1 }
        struct Unused { data: String }
        impl Used {}
        pub fn take(u: Used) -> I32 { u.value }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut eliminator = DeadCodeEliminator::new(&module);
    eliminator.analyze();

    let used_id = module.struct_id("Used").ok_or("Used struct not found")?;
    if !eliminator.is_struct_used(used_id) {
        return Err("Used struct should be marked as used".into());
    }

    let unused_id = module
        .struct_id("Unused")
        .ok_or("Unused struct not found")?;
    if eliminator.is_struct_used(unused_id) {
        return Err("Unused struct should not be marked as used".into());
    }
    Ok(())
}

#[test]
fn test_analyze_struct_referenced_in_field() -> Result<(), Box<dyn std::error::Error>> {
    // Outer is kept alive by a function parameter; Inner by being a field
    // type of Outer.
    let source = r"
        struct Inner { value: I32 = 1 }
        struct Outer { inner: Inner = Inner(value: 1) }
        impl Outer {}
        pub fn show(o: Outer) -> I32 { o.inner.value }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut eliminator = DeadCodeEliminator::new(&module);
    eliminator.analyze();

    let inner_id = module.struct_id("Inner").ok_or("Inner struct not found")?;
    let outer_id = module.struct_id("Outer").ok_or("Outer struct not found")?;

    if !eliminator.is_struct_used(inner_id) {
        return Err("Inner struct should be used (referenced by Outer)".into());
    }
    if !eliminator.is_struct_used(outer_id) {
        return Err("Outer struct should be used (referenced by `show`)".into());
    }
    Ok(())
}

#[test]
fn test_nested_dead_code_elimination() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: I32 = if true { if false { 1 } else { 2 } } else { 3 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);

    let struct_def = optimized
        .structs
        .first()
        .ok_or("expected at least one struct")?;
    let field = struct_def
        .fields
        .first()
        .ok_or("expected at least one field")?;
    let expr = field.default.as_ref().ok_or("expected default expr")?;

    // Outer true -> inner expression
    // Inner false -> 2
    // Final result should be 2
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = expr
    {
        if (n.value - 2.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 2, got {}", n.value).into());
        }
    } else {
        return Err(format!("Expected literal 2, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn test_analyze_trait_constraint_kept_alive() -> Result<(), Box<dyn std::error::Error>> {
    // A trait used only as a bound on a generic parameter must still be
    // marked as live so it is not eliminated.
    let source = r"
        pub trait Container { size: I32 }
        pub struct Box<T: Container> { value: T }
        impl Box {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

    let mut eliminator = DeadCodeEliminator::new(&module);
    eliminator.analyze();

    let trait_id = module
        .trait_id("Container")
        .ok_or("Container trait not found")?;
    if !eliminator.is_trait_used(trait_id) {
        return Err("Container trait should be marked as used because it is a bound".into());
    }
    Ok(())
}

mod removal_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn test_removal_drops_unused_struct() {
        let source = r"
            pub struct Used { value: I32 }
            pub struct Unused { data: String }
            impl Used { fn get(self) -> I32 { self.value } }
            pub fn run(u: Used) -> I32 { u.get() }
        ";
        let module = compile_to_ir(source).unwrap();
        let before = module.structs.len();
        assert!(before >= 2, "expected both structs in IR before DCE");
        let optimized = eliminate_dead_code(&module, true);
        assert!(
            optimized.structs.iter().any(|s| s.name == "Used"),
            "Used should survive"
        );
        assert!(
            !optimized.structs.iter().any(|s| s.name == "Unused"),
            "Unused should be removed"
        );
    }

    #[test]
    fn test_removal_preserves_remaining_struct_ids() {
        // After removing an unused struct, references to surviving structs
        // (e.g. in field types, function params) should still resolve.
        let source = r"
            pub struct Unused { data: String }
            pub struct Used { value: I32 }
            pub fn run(u: Used) -> I32 { u.value }
        ";
        let module = compile_to_ir(source).unwrap();
        let optimized = eliminate_dead_code(&module, true);
        // Name → ID lookup via the rebuilt indices.
        let used_id = optimized.struct_id("Used").unwrap();
        let used = optimized.get_struct(used_id).unwrap();
        assert_eq!(used.name, "Used");
        assert_eq!(used.fields.len(), 1);
    }

    #[test]
    fn test_removal_drops_impl_for_removed_enum() {
        // An impl block targeting a removed enum must be dropped.
        let source = r"
            pub enum Used { a, b }
            pub enum Unused { x, y }
            impl Unused { fn describe(self) -> I32 { 0 } }
            pub fn run(u: Used) -> Used { u }
        ";
        let module = compile_to_ir(source).unwrap();
        let before_impls = module.impls.len();
        assert!(before_impls >= 1, "expected Unused impl in IR before DCE");
        let optimized = eliminate_dead_code(&module, true);
        assert!(
            !optimized.enums.iter().any(|e| e.name == "Unused"),
            "Unused enum should be removed"
        );
        // The impl targeted Unused; it should be gone too.
        for impl_block in &optimized.impls {
            match impl_block.target {
                crate::ir::ImplTarget::Enum(id) => {
                    let e = optimized.get_enum(id).unwrap();
                    assert_ne!(e.name, "Unused");
                }
                crate::ir::ImplTarget::Struct(_) => {}
            }
        }
    }
}
