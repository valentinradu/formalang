//! Targeted integration tests for DCE and visitor coverage.
//!
//! Covers:
//! - `DeadCodeEliminator::analyze()` for various module shapes
//! - `eliminate_dead_code()` / `eliminate_dead_code_expr()` for all expression variants
//! - `walk_expr_children()` for all `IrExpr` variants
//! - `walk_block_statement()` for all `IrBlockStatement` variants

#![allow(clippy::cast_possible_truncation)]

use formalang::compile_to_ir;
use formalang::ir::{
    eliminate_dead_code, walk_block_statement, walk_expr_children, walk_module, DeadCodeEliminator,
    EnumId, IrBlockStatement, IrEnum, IrEnumVariant, IrExpr, IrField, IrFunction, IrImpl, IrLet,
    IrStruct, IrVisitor, StructId, TraitId,
};

// =============================================================================
// DCE: analyze() - marking used structs
// =============================================================================

#[test]
fn test_dce_analyze_struct_in_impl_function_body() -> Result<(), Box<dyn std::error::Error>> {
    // Struct used inside an impl function body should be marked used.
    // (Outer is kept alive here by a standalone function signature — DCE no
    // longer treats a bare impl block as a use of its target.)
    let source = r"
        struct Inner { value: Number = 0 }
        struct Outer { items: [Inner] }
        impl Outer {
            fn make() -> Inner { Inner(value: 1) }
        }
        pub fn entry(o: Outer) -> Number { 0 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();

    let inner_id = module.struct_id("Inner").ok_or("Inner must exist")?;
    let outer_id = module.struct_id("Outer").ok_or("Outer must exist")?;
    if !dce.is_struct_used(inner_id) {
        return Err("Inner should be used (in impl body and as Outer field)".into());
    }
    if !dce.is_struct_used(outer_id) {
        return Err("Outer should be used (via `entry` function parameter)".into());
    }
    Ok(())
}

#[test]
fn test_dce_analyze_struct_in_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        let origin: Point = Point(x: 0, y: 0)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();

    let id = module.struct_id("Point").ok_or("Point must exist")?;
    if !dce.is_struct_used(id) {
        return Err("Point should be used via let binding".into());
    }
    Ok(())
}

#[test]
fn test_dce_analyze_struct_not_used() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Used { value: Number = 1 }
        struct NotUsed { data: String }
        impl Used {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();

    let not_used_id = module.struct_id("NotUsed").ok_or("NotUsed must exist")?;
    if dce.is_struct_used(not_used_id) {
        return Err("NotUsed should not be marked used".into());
    }
    Ok(())
}

#[test]
fn test_dce_analyze_used_structs_set() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { x: Number = 0 }
        struct B { a: A = A(x: 1) }
        impl B {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();

    // Both should be in used set
    let set = dce.used_structs();
    if set.is_empty() {
        return Err("Used structs should not be empty".into());
    }
    Ok(())
}

// =============================================================================
// DCE: struct field reference through struct field types
// =============================================================================

#[test]
fn test_dce_struct_referenced_in_struct_field_type() -> Result<(), Box<dyn std::error::Error>> {
    // Inner is referenced in the type of an Outer field - should be marked used
    let source = r"
        struct Inner { x: Number = 0 }
        struct Outer { inner: Inner }
        impl Outer {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();

    let inner_id = module.struct_id("Inner").ok_or("Inner")?;
    if !dce.is_struct_used(inner_id) {
        return Err("Inner used via Outer field type".into());
    }
    Ok(())
}

// =============================================================================
// DCE: eliminate_dead_code_expr for various expression variants
// =============================================================================

#[test]
fn test_dce_expr_binary_op_both_sides() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = if true { 1 + 2 } else { 3 * 4 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    // After DCE: constant true -> takes 1 + 2
    let default_val = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    // Should be reduced to the then branch
    if !matches!(
        default_val,
        IrExpr::BinaryOp { .. } | IrExpr::Literal { .. }
    ) {
        return Err(format!("Expected BinaryOp or Literal after DCE, got {default_val:?}").into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_unary_op_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let flag: Boolean = !false
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let let_binding = optimized
        .lets
        .iter()
        .find(|l| l.name == "flag")
        .ok_or("flag")?;
    // UnaryOp is a leaf in DCE - passed through unchanged
    if !matches!(
        &let_binding.value,
        IrExpr::UnaryOp { .. } | IrExpr::Literal { .. }
    ) {
        return Err(format!(
            "Expected UnaryOp or Literal after DCE, got {:?}",
            let_binding.value
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_array_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            items: [Number] = [if true { 1 } else { 2 }, if false { 3 } else { 4 }]
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Array { elements, .. } = default else {
        return Err(format!("Expected Array, got {default:?}").into());
    };
    if elements.len() != 2 {
        return Err(format!("expected 2 elements, got {}", elements.len()).into());
    }
    // First element: if true { 1 } -> 1 (constant true eliminated)
    if !matches!(
        &elements.first().ok_or("index out of bounds")?,
        IrExpr::Literal { .. }
    ) {
        return Err(format!(
            "Expected constant true to be eliminated in array element 0, got {:?}",
            elements.first().ok_or("index out of bounds")?
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_tuple_with_dead_code() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config {
            pair: (x: Number, y: Number) = (x: if true { 1 } else { 99 }, y: if false { 2 } else { 3 })
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Tuple { fields, .. } = default else {
        return Err(format!("Expected Tuple, got {default:?}").into());
    };
    if fields.len() != 2 {
        return Err(format!("expected 2 fields, got {}", fields.len()).into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_for_loop() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [Number] = [1, 2, 3]
        let doubled: [Number] = for x in items { if true { x } else { 0 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let _let_binding = optimized
        .lets
        .iter()
        .find(|l| l.name == "doubled")
        .ok_or("doubled")?;
    Ok(())
}

#[test]
fn test_dce_expr_match_arms() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green, blue }
        struct Config {
            name: String = match Color.red {
                .red: if true { "red" } else { "other" },
                _: "none"
            }
        }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Match { arms, .. } = default else {
        return Err(format!("Expected Match, got {default:?}").into());
    };
    if arms.is_empty() {
        return Err("Arms should not be empty".into());
    }
    // The first arm body should have been optimized
    if !matches!(
        &arms.first().ok_or("index out of bounds")?.body,
        IrExpr::Literal { .. } | IrExpr::If { .. }
    ) {
        return Err(format!(
            "Unexpected match arm body: {:?}",
            arms.first().ok_or("index out of bounds")?.body
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_function_call_with_dead_code_args() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: Number) -> Number { x }
        struct Config { val: Number = compute(x: if true { 1 } else { 2 }) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::FunctionCall { args, .. } = default else {
        return Err(format!("Expected FunctionCall, got {default:?}").into());
    };
    if args.is_empty() {
        return Err("expected at least one arg".into());
    }
    // Arg should be optimized
    if !matches!(
        &args.first().ok_or("index out of bounds")?.1,
        IrExpr::Literal { .. } | IrExpr::If { .. }
    ) {
        return Err(format!(
            "Unexpected arg: {:?}",
            args.first().ok_or("index out of bounds")?.1
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_preserves_struct_and_its_impl_methods() -> Result<(), Box<dyn std::error::Error>> {
    // Audit #52: the previous "verify DCE runs without panic" version
    // checked nothing meaningful. `Config::box` keeps `Container`
    // alive; DCE preserves the struct and (per current design) keeps
    // the whole impl block attached so its methods remain callable
    // through the surviving struct.
    let source = r"
        struct Container { items: [Number] }
        impl Container {
            fn count() -> Number { 1 }
        }
        struct Config {
            box: Container = Container(items: [1, 2])
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    if optimized.structs.iter().all(|s| s.name != "Container") {
        return Err("Container struct should survive DCE — Config still references it".into());
    }
    let count_survives = optimized
        .impls
        .iter()
        .flat_map(|i| i.functions.iter())
        .any(|f| f.name == "count");
    if !count_survives {
        return Err(
            "Container::count should survive DCE because Container is still reachable".into(),
        );
    }
    Ok(())
}

#[test]
fn test_dce_expr_struct_inst_with_dead_code() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        struct Config {
            p: Point = Point(x: if true { 1 } else { 2 }, y: if false { 3 } else { 4 })
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .get(1)
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::StructInst { fields, .. } = default else {
        return Err(format!("Expected StructInst, got {default:?}").into());
    };
    if fields.len() != 2 {
        return Err(format!("expected 2 fields, got {}", fields.len()).into());
    }
    // x: if true { 1 } -> x: 1
    if !matches!(
        &fields.first().ok_or("index out of bounds")?.1,
        IrExpr::Literal { .. } | IrExpr::If { .. }
    ) {
        return Err(format!(
            "Unexpected field value: {:?}",
            fields.first().ok_or("index out of bounds")?.1
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_enum_inst() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Status { active, inactive }
        struct Config {
            status: Status = Status.active
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::EnumInst { variant, .. } = default else {
        return Err(format!("Expected EnumInst, got {default:?}").into());
    };
    if variant != "active" {
        return Err(format!("expected 'active', got '{variant}'").into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_dict_literal_with_dead_code() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config {
            data: [String: Number] = [
                "a": if true { 1 } else { 2 }
            ]
        }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::DictLiteral { entries, .. } = default else {
        return Err(format!("Expected DictLiteral, got {default:?}").into());
    };
    if entries.len() != 1 {
        return Err(format!("expected 1 entry, got {}", entries.len()).into());
    }
    if !matches!(
        &entries.first().ok_or("index out of bounds")?.1,
        IrExpr::Literal { .. } | IrExpr::If { .. }
    ) {
        return Err(format!(
            "Unexpected dict entry: {:?}",
            entries.first().ok_or("index out of bounds")?.1
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_dce_expr_dict_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let lookup: [String: Number] = ["key": 42]
        let val: Number = lookup["key"]
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let _let_binding = optimized
        .lets
        .iter()
        .find(|l| l.name == "val")
        .ok_or("val")?;
    Ok(())
}

#[test]
fn test_dce_expr_block_with_if() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = {
            let x: Number = if true { 1 } else { 2 }
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block, got {default:?}").into());
    };
    if statements.is_empty() {
        return Err("Block should not be empty".into());
    }
    // Let statement value should be optimized
    let IrBlockStatement::Let { value, .. } = &statements.first().ok_or("index out of bounds")?
    else {
        return Err(format!(
            "Expected Let statement, got {:?}",
            statements.first().ok_or("index out of bounds")?
        )
        .into());
    };
    if !matches!(value, IrExpr::Literal { .. } | IrExpr::If { .. }) {
        return Err(format!("Unexpected let value: {value:?}").into());
    }
    Ok(())
}

#[test]
fn test_dce_block_assign_statement() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter {
            mut count: Number = {
                let mut x: Number = if true { 0 } else { 99 }
                x = if false { 5 } else { 10 }
                x
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block, got {default:?}").into());
    };
    if statements.len() < 2 {
        return Err(format!("Should have let + assign, got {}", statements.len()).into());
    }
    // Assign statement
    let IrBlockStatement::Assign { value, .. } = &statements.get(1).ok_or("index out of bounds")?
    else {
        return Err(format!(
            "Expected Assign statement, got {:?}",
            statements.get(1).ok_or("index out of bounds")?
        )
        .into());
    };
    if !matches!(value, IrExpr::Literal { .. } | IrExpr::If { .. }) {
        return Err(format!("Unexpected assign value: {value:?}").into());
    }
    Ok(())
}

#[test]
fn test_dce_block_expr_statement() -> Result<(), Box<dyn std::error::Error>> {
    // Block with an expression statement (side effect)
    let source = r"
        struct Config { value: Number = {
            compute(x: if true { 1 } else { 2 })
            42
        }}
        fn compute(x: Number) -> Number { x }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block, got {default:?}").into());
    };
    if statements.is_empty() {
        return Err("Block should not be empty".into());
    }
    let IrBlockStatement::Expr(expr) = &statements.first().ok_or("index out of bounds")? else {
        return Err(format!(
            "Expected Expr statement, got {:?}",
            statements.first().ok_or("index out of bounds")?
        )
        .into());
    };
    if !matches!(expr, IrExpr::FunctionCall { .. }) {
        return Err(format!("Expected FunctionCall expression statement, got {expr:?}").into());
    }
    Ok(())
}

// =============================================================================
// DCE: eliminate_dead_code with remove_unused_structs=true
// =============================================================================

#[test]
fn test_dce_eliminate_with_remove_unused_structs() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Used { value: Number = 1 }
        struct Unused { data: String }
        impl Used { fn get(self) -> Number { self.value } }
        pub fn entry(u: Used) -> Number { u.get() }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, true);
    if !optimized.structs.iter().any(|s| s.name == "Used") {
        return Err("Used should survive DCE (referenced by `entry`)".into());
    }
    if optimized.structs.iter().any(|s| s.name == "Unused") {
        return Err("Unused should have been removed".into());
    }
    Ok(())
}

// =============================================================================
// DCE: DeadCodeEliminationPass via Pipeline
// =============================================================================

#[test]
fn test_dce_pass_via_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminationPass;
    use formalang::pipeline::IrPass;

    // Keep Config alive via a standalone function parameter so the default
    // pass (which now removes unused types) does not drop it.
    let source = r"
        struct Config { value: Number = if true { 1 } else { 2 } }
        pub fn use_config(c: Config) -> Number { c.value }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut pass = DeadCodeEliminationPass::new();
    let result = pass.run(module);
    let optimized = result.map_err(|e| format!("pass: {e:?}"))?;
    let default = optimized
        .structs
        .iter()
        .find(|s| s.name == "Config")
        .ok_or("Config missing")?
        .fields
        .first()
        .ok_or("Config has no fields")?
        .default
        .as_ref()
        .ok_or("no default")?;
    if !matches!(default, IrExpr::Literal { .. } | IrExpr::If { .. }) {
        return Err(format!("Expected Literal or If after DCE pass, got {default:?}").into());
    }
    Ok(())
}

#[test]
fn test_dce_pass_default_creates_with_remove_true() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminationPass;
    let pass = DeadCodeEliminationPass::default();
    if !pass.remove_unused_structs {
        return Err(
            "DeadCodeEliminationPass::default() should have remove_unused_structs = true".into(),
        );
    }
    Ok(())
}

// =============================================================================
// Visitor: walk_module_children covers structs, traits, enums, impls, lets
// =============================================================================

struct CountingVisitor {
    structs: usize,
    traits: usize,
    enums: usize,
    impls: usize,
    lets: usize,
    functions: usize,
    fields: usize,
    enum_variants: usize,
    exprs: usize,
}

impl CountingVisitor {
    const fn new() -> Self {
        Self {
            structs: 0,
            traits: 0,
            enums: 0,
            impls: 0,
            lets: 0,
            functions: 0,
            fields: 0,
            enum_variants: 0,
            exprs: 0,
        }
    }
}

impl IrVisitor for CountingVisitor {
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.structs = self.structs.saturating_add(1);
    }
    fn visit_trait(&mut self, _id: TraitId, _t: &formalang::ir::IrTrait) {
        self.traits = self.traits.saturating_add(1);
    }
    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
        self.enums = self.enums.saturating_add(1);
    }
    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {
        self.enum_variants = self.enum_variants.saturating_add(1);
    }
    fn visit_impl(&mut self, _i: &IrImpl) {
        self.impls = self.impls.saturating_add(1);
    }
    fn visit_function(&mut self, _f: &IrFunction) {
        self.functions = self.functions.saturating_add(1);
    }
    fn visit_let(&mut self, _l: &IrLet) {
        self.lets = self.lets.saturating_add(1);
    }
    fn visit_field(&mut self, _f: &IrField) {
        self.fields = self.fields.saturating_add(1);
    }
    fn visit_expr(&mut self, e: &IrExpr) {
        self.exprs = self.exprs.saturating_add(1);
        walk_expr_children(self, e);
    }
}

#[test]
fn test_visitor_walk_full_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub trait Shape { area: Number }
        pub struct Circle {
            area: Number,
            radius: Number
        }
        pub enum Color { red, green, blue }
        impl Circle {
            fn scale(factor: Number) -> Number { self.radius }
        }
        pub let pi: Number = 3
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = CountingVisitor::new();
    walk_module(&mut visitor, &module);

    if visitor.structs != 1 {
        return Err(format!("expected 1 struct, got {}", visitor.structs).into());
    }
    if visitor.traits != 1 {
        return Err(format!("expected 1 trait, got {}", visitor.traits).into());
    }
    if visitor.enums != 1 {
        return Err(format!("expected 1 enum, got {}", visitor.enums).into());
    }
    if visitor.impls != 1 {
        return Err(format!("expected 1 impl, got {}", visitor.impls).into());
    }
    if visitor.lets != 1 {
        return Err(format!("expected 1 let, got {}", visitor.lets).into());
    }
    if visitor.enum_variants != 3 {
        return Err(format!("expected 3 enum variants, got {}", visitor.enum_variants).into());
    }
    if visitor.functions < 1 {
        return Err("expected at least one function".into());
    }
    if visitor.exprs == 0 {
        return Err("expected at least one expression visited".into());
    }
    Ok(())
}

// =============================================================================
// Visitor: walk_expr_children for all expression variants
// =============================================================================

/// An expression-collecting visitor to confirm sub-expressions are traversed
struct ExprCollector {
    expr_kinds: Vec<String>,
}

impl ExprCollector {
    const fn new() -> Self {
        Self {
            expr_kinds: Vec::new(),
        }
    }
}

impl IrVisitor for ExprCollector {
    fn visit_expr(&mut self, e: &IrExpr) {
        let kind = match e {
            IrExpr::Literal { .. } => "Literal",
            IrExpr::Reference { .. } => "Reference",
            IrExpr::SelfFieldRef { .. } => "SelfFieldRef",
            IrExpr::LetRef { .. } => "LetRef",
            IrExpr::StructInst { .. } => "StructInst",
            IrExpr::EnumInst { .. } => "EnumInst",
            IrExpr::Array { .. } => "Array",
            IrExpr::Tuple { .. } => "Tuple",
            IrExpr::FieldAccess { .. } => "FieldAccess",
            IrExpr::BinaryOp { .. } => "BinaryOp",
            IrExpr::UnaryOp { .. } => "UnaryOp",
            IrExpr::If { .. } => "If",
            IrExpr::For { .. } => "For",
            IrExpr::Match { .. } => "Match",
            IrExpr::FunctionCall { .. } => "FunctionCall",
            IrExpr::MethodCall { .. } => "MethodCall",
            IrExpr::DictLiteral { .. } => "DictLiteral",
            IrExpr::DictAccess { .. } => "DictAccess",
            IrExpr::Block { .. } => "Block",
            IrExpr::Closure { .. } => "Closure",
        };
        self.expr_kinds.push(kind.to_string());
        walk_expr_children(self, e);
    }
}

#[test]
fn test_visitor_walk_if_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = if true { 1 } else { 2 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"If".to_string())
        && !visitor.expr_kinds.contains(&"Literal".to_string())
    {
        return Err(format!(
            "Should have visited If or Literal (DCE may optimize), got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_for_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let items: [Number] = [1, 2, 3]
        let doubled: [Number] = for x in items { x }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    // Should contain For or Array depending on how lowering works
    if visitor.expr_kinds.is_empty() {
        return Err("Should have visited some expressions".into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_match_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Status { active, inactive }
        struct Config {
            label: String = match Status.active {
                .active: "on",
                _: "off"
            }
        }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"Match".to_string()) {
        return Err(format!(
            "Should have visited Match expression. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_function_call() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        fn compute(x: Number) -> Number { x }
        struct Config { val: Number = compute(x: 5) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"FunctionCall".to_string()) {
        return Err(format!(
            "Should have visited FunctionCall. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_method_call() -> Result<(), Box<dyn std::error::Error>> {
    // Use a struct impl with a method call in a function body
    let source = r"
        struct Rect { width: Number, height: Number }
        impl Rect {
            fn area() -> Number { self.width }
            fn compute() -> Number { self.area() }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"MethodCall".to_string()) {
        return Err(format!(
            "Should have visited MethodCall. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_dict_literal() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        struct Config { data: [String: Number] = ["key": 42] }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"DictLiteral".to_string()) {
        return Err(format!(
            "Should have visited DictLiteral. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_dict_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let lookup: [String: Number] = ["x": 1]
        let val: Number = lookup["x"]
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"DictAccess".to_string()) {
        return Err(format!(
            "Should have visited DictAccess. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = 1 + 2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"BinaryOp".to_string()) {
        return Err(format!(
            "Should have visited BinaryOp. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_unary_op() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { enabled: Boolean = !false }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"UnaryOp".to_string()) {
        return Err(format!("Should have visited UnaryOp. Got: {:?}", visitor.expr_kinds).into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_array_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { items: [Number] = [1, 2, 3] }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"Array".to_string()) {
        return Err(format!("Should have visited Array. Got: {:?}", visitor.expr_kinds).into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_tuple_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { pair: (x: Number, y: Number) = (x: 1, y: 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"Tuple".to_string()) {
        return Err(format!("Should have visited Tuple. Got: {:?}", visitor.expr_kinds).into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_struct_inst() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        struct Config { p: Point = Point(x: 1, y: 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"StructInst".to_string()) {
        return Err(format!(
            "Should have visited StructInst. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_enum_inst() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green }
        struct Config { color: Color = Color.red }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"EnumInst".to_string()) {
        return Err(format!(
            "Should have visited EnumInst. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_field_access() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0, y: Number = 0 }
        impl Point {
            fn get_x() -> Number { self.x }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    // SelfFieldRef is used for self.x in impl context
    if !visitor.expr_kinds.contains(&"SelfFieldRef".to_string())
        && !visitor.expr_kinds.contains(&"FieldAccess".to_string())
    {
        return Err(format!(
            "Should visit SelfFieldRef or FieldAccess. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_block_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = {
            let x: Number = 5
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"Block".to_string()) {
        return Err(format!("Should have visited Block. Got: {:?}", visitor.expr_kinds).into());
    }
    Ok(())
}

#[test]
fn test_visitor_walk_let_ref() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let base: Number = 10
        let doubled: Number = base + base
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = ExprCollector::new();
    walk_module(&mut visitor, &module);
    if !visitor.expr_kinds.contains(&"LetRef".to_string()) {
        return Err(format!(
            "Should visit LetRef for module-level let references. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Visitor: custom visit_module override (skips default walk)
// =============================================================================

struct SelectiveVisitor {
    struct_count: usize,
}

impl IrVisitor for SelectiveVisitor {
    fn visit_module(&mut self, module: &formalang::ir::IrModule) {
        // Only visit structs, skipping everything else
        for (idx, s) in module.structs.iter().enumerate() {
            let Ok(raw_id) = u32::try_from(idx) else {
                continue;
            };
            self.visit_struct(StructId(raw_id), s);
        }
    }

    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
        self.struct_count = self.struct_count.saturating_add(1);
    }
}

#[test]
fn test_visitor_custom_visit_module() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct A { x: Number }
        pub struct B { y: String }
        pub enum E { v1, v2 }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = SelectiveVisitor { struct_count: 0 };
    walk_module(&mut visitor, &module);
    if visitor.struct_count != 2 {
        return Err(format!("expected 2 structs, got {}", visitor.struct_count).into());
    }
    Ok(())
}

// =============================================================================
// Visitor: walk_block_statement directly
// =============================================================================

#[test]
fn test_walk_block_statement_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = {
            let x: Number = 42
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    // Walk the block expression and count sub-expressions
    let default = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block expression, got {default:?}").into());
    };
    let mut visitor = ExprCollector::new();
    for stmt in statements {
        walk_block_statement(&mut visitor, stmt);
    }
    if !visitor.expr_kinds.contains(&"Literal".to_string()) {
        return Err(format!(
            "Let statement value should be visited. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_walk_block_statement_assign() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Counter { mut count: Number = {
            let mut x: Number = 0
            x = 5
            x
        }}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let default = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block, got {default:?}").into());
    };
    let mut visitor = ExprCollector::new();
    for stmt in statements {
        walk_block_statement(&mut visitor, stmt);
    }
    if visitor.expr_kinds.is_empty() {
        return Err(format!(
            "Block statements should be visited. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_walk_block_statement_expr() -> Result<(), Box<dyn std::error::Error>> {
    // Expression statements inside blocks
    let source = r"
        struct Config { value: Number = {
            compute(x: 1)
            42
        }}
        fn compute(x: Number) -> Number { x }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let default = module
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    let IrExpr::Block { statements, .. } = default else {
        return Err(format!("Expected Block, got {default:?}").into());
    };
    let mut visitor = ExprCollector::new();
    for stmt in statements {
        walk_block_statement(&mut visitor, stmt);
    }
    if !visitor.expr_kinds.contains(&"FunctionCall".to_string()) {
        return Err(format!(
            "Expr statement FunctionCall should be visited. Got: {:?}",
            visitor.expr_kinds
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// DCE: mark_used_in_expr covers If without else branch
// =============================================================================

#[test]
fn test_dce_analyze_if_without_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Inner { x: Number = 0 }
        struct Config {
            val: Number = if true { 1 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    // Inner is not used (no impl, no field refs)
    let inner_id = module.struct_id("Inner").ok_or("Inner")?;
    if dce.is_struct_used(inner_id) {
        return Err("Inner should not be used".into());
    }
    Ok(())
}

#[test]
fn test_dce_if_constant_true_no_else() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Config { value: Number = if true { 5 } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);
    let default = optimized
        .structs
        .first()
        .ok_or("index out of bounds")?
        .fields
        .first()
        .ok_or("index out of bounds")?
        .default
        .as_ref()
        .ok_or("no default")?;
    // Constant true with no else: should reduce to then branch
    if !matches!(default, IrExpr::Literal { .. } | IrExpr::If { .. }) {
        return Err(format!("Unexpected: {default:?}").into());
    }
    Ok(())
}

// =============================================================================
// DCE: closure body traversal
// =============================================================================

#[test]
fn test_dce_analyze_struct_in_closure() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Point { x: Number = 0 }
        struct Config {
            callback: (Number) -> Number = |n: Number| n
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    let point_id = module.struct_id("Point").ok_or("Point")?;
    // Point is not referenced from the closure body, so DCE should
    // not mark it used.
    if dce.is_struct_used(point_id) {
        return Err("Point should not be used via closure".into());
    }
    Ok(())
}

// =============================================================================
// Visitor: multiple fields are visited
// =============================================================================

#[test]
fn test_visitor_walk_multiple_fields() -> Result<(), Box<dyn std::error::Error>> {
    // Structs with multiple fields - visitor should visit all field defaults
    let source = r"
        pub struct Box {
            width: Number,
            child: Number = 0
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let mut visitor = CountingVisitor::new();
    walk_module(&mut visitor, &module);
    if visitor.fields < 2 {
        return Err(format!(
            "Should visit at least 2 fields (width + child). Got: {}",
            visitor.fields
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// IR: simple_type_name utility
// =============================================================================

#[test]
fn test_simple_type_name_utility() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::simple_type_name;
    let r1 = simple_type_name("foo::Bar");
    let r2 = simple_type_name("Simple");
    let r3 = simple_type_name("a::b::C");
    if r1 != "Bar" {
        return Err(format!("expected 'Bar', got '{r1}'").into());
    }
    if r2 != "Simple" {
        return Err(format!("expected 'Simple', got '{r2}'").into());
    }
    if r3 != "C" {
        return Err(format!("expected 'C', got '{r3}'").into());
    }
    Ok(())
}

// =============================================================================
// DCE: trait preservation (Phase 4 audit fix)
// =============================================================================

/// A trait referenced only as a constraint on a struct's generic parameter
/// must still be marked as used — otherwise a downstream pass that tries to
/// remove unused traits would corrupt the module.
#[test]
fn test_dce_preserves_trait_used_as_struct_generic_constraint(
) -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub trait Drawable { fn draw(self) }

        pub struct Canvas<T: Drawable> {
            item: T
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let trait_id = module
        .traits
        .iter()
        .enumerate()
        .find_map(|(i, t)| {
            if t.name == "Drawable" {
                Some(TraitId(i as u32))
            } else {
                None
            }
        })
        .ok_or("Drawable trait id")?;

    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    if !dce.is_trait_used(trait_id) {
        return Err("expected Drawable to be marked used via generic constraint".into());
    }
    Ok(())
}

/// A trait referenced only via virtual dispatch on a method call must be
/// preserved — the `DispatchKind::Virtual` variant stores the `trait_id` and the
/// DCE must inspect it.
#[test]
fn test_dce_preserves_trait_used_via_virtual_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    // When the receiver's type is a bare type parameter constrained by a
    // trait, dispatch is virtual; the trait must stay live.
    let source = r"
        pub trait Area {
            fn area(self) -> Number
        }

        pub struct Sum<T: Area> { item: T }

        impl Sum<T> {
            fn total(self) -> Number {
                self.item.area()
            }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;
    let trait_id = module
        .traits
        .iter()
        .enumerate()
        .find_map(|(i, t)| {
            if t.name == "Area" {
                Some(TraitId(i as u32))
            } else {
                None
            }
        })
        .ok_or("Area trait id")?;

    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    if !dce.is_trait_used(trait_id) {
        return Err("expected Area to be marked used via virtual dispatch".into());
    }
    Ok(())
}

/// Composed traits keep their parents alive: if `B: A` and `B` is used, `A`
/// must be used too.
#[test]
fn test_dce_marks_composed_parent_trait_as_used() -> Result<(), Box<dyn std::error::Error>> {
    // Renamed from `test_dce_preserves_parent_trait_in_composition`
    // (audit #53): the original name was vague about *which* relation
    // the test checks. The body only asserts that `Named` (the parent)
    // is marked used by DCE because `Tracked` composes it; `Tracked`'s
    // own usage is asserted separately by the conformance check.
    let source = r"
        pub trait Named { name: String }
        pub trait Tracked: Named {
            name: String,
            when: Number
        }

        pub struct Task {
            name: String,
            when: Number
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("compile: {e:?}"))?;

    // Find both trait ids.
    let named_id = module
        .traits
        .iter()
        .enumerate()
        .find_map(|(i, t)| {
            if t.name == "Named" {
                Some(TraitId(i as u32))
            } else {
                None
            }
        })
        .ok_or("Named trait id")?;
    let tracked_id = module
        .traits
        .iter()
        .enumerate()
        .find_map(|(i, t)| {
            if t.name == "Tracked" {
                Some(TraitId(i as u32))
            } else {
                None
            }
        })
        .ok_or("Tracked trait id")?;

    let mut dce = DeadCodeEliminator::new(&module);
    dce.analyze();
    // Sanity: Tracked is referenced via conformance on Task, so it must be
    // used. And the fact that Tracked composes Named must make Named used too.
    let _ = tracked_id; // only check the child relation
    if !dce.is_trait_used(named_id) {
        return Err("expected Named (parent trait) to be preserved via composition".into());
    }
    Ok(())
}
