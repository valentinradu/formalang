//! Tests for IR lowering of module-level definitions.
//!
//! Covers Bug 2 (traits in `mod` blocks silently dropped) and
//! Bug 3 (enum variant binding in match arms).

use formalang::compile_to_ir;
use formalang::ir::IrExpr;

// =============================================================================
// Bug 2: Traits in mod blocks silently dropped from IR
// =============================================================================

#[test]
fn module_trait_lowers_without_error() -> Result<(), Box<dyn std::error::Error>> {
    // A trait defined inside a mod block must appear in the IR module.
    let source = r"
pub mod geometry {
    pub trait Shape {
        area: Number
    }
}
";
    let module =
        compile_to_ir(source).map_err(|e| format!("should compile without errors: {e:?}"))?;

    // The trait must be registered with its qualified name.
    if module.trait_id("geometry::Shape").is_none() {
        return Err("geometry::Shape trait should be present in IR, but it was not found".into());
    }
    Ok(())
}

#[test]
fn module_trait_with_struct_compiles() -> Result<(), Box<dyn std::error::Error>> {
    // A struct that implements a trait defined in the same mod block must
    // compile and the struct must reference the trait in the IR.
    let source = r"
pub mod shapes {
    pub trait Drawable {
        visible: Boolean
    }
    pub struct Circle: Drawable {
        visible: Boolean,
        radius: Number
    }
}
";
    let module =
        compile_to_ir(source).map_err(|e| format!("should compile without errors: {e:?}"))?;

    if module.trait_id("shapes::Drawable").is_none() {
        return Err("shapes::Drawable trait should be present in IR".into());
    }

    let circle = module
        .structs
        .iter()
        .find(|s| s.name == "shapes::Circle")
        .ok_or("shapes::Circle struct should be present in IR")?;

    if circle.fields.len() != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, circle.fields.len()).into());
    }

    let trait_id = module
        .trait_id("shapes::Drawable")
        .ok_or("shapes::Drawable must have an ID")?;

    if !circle.traits.contains(&trait_id) {
        return Err("shapes::Circle should implement shapes::Drawable".into());
    }
    Ok(())
}

// =============================================================================
// Bug 3: Enum variant binding in match arms
// =============================================================================

#[test]
fn event_mapping_explicit_enum_instantiation() -> Result<(), Box<dyn std::error::Error>> {
    // A closure whose body uses an explicit `EnumName.Variant(...)` syntax
    // must lower to an EventMapping node, not a generic Closure node.
    let source = r"
enum Event { changed(value: Number) }
struct Slider {
    onChange: (Number -> Event)? = x -> Event.changed(value: x)
}
";
    let module =
        compile_to_ir(source).map_err(|e| format!("should compile without errors: {e:?}"))?;

    // The default expression on the field should be an EventMapping.
    let slider = module
        .structs
        .iter()
        .find(|s| s.name == "Slider")
        .ok_or("Slider struct should exist")?;

    let field = slider
        .fields
        .iter()
        .find(|f| f.name == "onChange")
        .ok_or("onChange field should exist")?;

    let default_expr = field
        .default
        .as_ref()
        .ok_or("onChange field should have a default")?;

    match default_expr {
        IrExpr::EventMapping { variant, param, .. } => {
            if variant != "changed" {
                return Err(format!("expected {:?} but got {:?}", "changed", variant).into());
            }
            if param.as_deref() != Some("x") {
                return Err(
                    format!("expected {:?} but got {:?}", Some("x"), param.as_deref()).into(),
                );
            }
        }
        other @
(IrExpr::Literal { .. } | IrExpr::StructInst { .. } | IrExpr::EnumInst { .. }
| IrExpr::Array { .. } | IrExpr::Tuple { .. } | IrExpr::Reference { .. } |
IrExpr::SelfFieldRef { .. } | IrExpr::FieldAccess { .. } | IrExpr::LetRef { ..
} | IrExpr::BinaryOp { .. } | IrExpr::UnaryOp { .. } | IrExpr::If { .. } |
IrExpr::For { .. } | IrExpr::Match { .. } | IrExpr::FunctionCall { .. } |
IrExpr::MethodCall { .. } | IrExpr::Closure { .. } | IrExpr::DictLiteral { ..
} | IrExpr::DictAccess { .. } | IrExpr::Block { .. }) => {
            return Err(format!(
                "Expected IrExpr::EventMapping for explicit enum closure, got: {other:?}"
            )
            .into())
        }
    }
    Ok(())
}

// =============================================================================
// IrModule lookup methods: struct_id, trait_id, enum_id, function_id
// =============================================================================

#[test]
fn irmodule_struct_id_returns_correct_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct Alpha { x: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module.struct_id("Alpha").ok_or("Alpha should be found")?;
    let alpha_name = &module.get_struct(id).ok_or("struct not found")?.name;
    if alpha_name != "Alpha" {
        return Err(format!("expected {:?} but got {:?}", "Alpha", alpha_name).into());
    }
    Ok(())
}

#[test]
fn irmodule_struct_id_returns_none_for_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub struct Alpha { x: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    if module.struct_id("NonExistent").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn irmodule_trait_id_returns_correct_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub trait Sized2 { width: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module
        .trait_id("Sized2")
        .ok_or("Sized2 trait should be found")?;
    let trait_name = &module.get_trait(id).ok_or("trait not found")?.name;
    if trait_name != "Sized2" {
        return Err(format!("expected {:?} but got {:?}", "Sized2", trait_name).into());
    }
    Ok(())
}

#[test]
fn irmodule_enum_id_returns_correct_id() -> Result<(), Box<dyn std::error::Error>> {
    let source = "pub enum Direction { north, south, east, west }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module
        .enum_id("Direction")
        .ok_or("Direction enum should be found")?;
    let enum_name = &module.get_enum(id).ok_or("enum not found")?.name;
    if enum_name != "Direction" {
        return Err(format!("expected {:?} but got {:?}", "Direction", enum_name).into());
    }
    Ok(())
}

#[test]
fn irmodule_function_id_returns_correct_id() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::FunctionId;

    let source = "pub fn add(a: f32, b: f32) -> f32 { a + b }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module
        .function_id("add")
        .ok_or("add function should be found")?;
    if module.get_function(id).ok_or("function not found")?.name != "add" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "add",
            module.get_function(id).ok_or("function not found")?.name
        )
        .into());
    }

    // Also test FunctionId direct indexing
    let f = module.get_function(FunctionId(0)).ok_or("function not found")?;
    if f.name != "add" {
        return Err(format!("expected {:?} but got {:?}", "add", f.name).into());
    }
    Ok(())
}

#[test]
fn irmodule_get_let_and_has_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let maxCount: Number = 42";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    if !(module.has_let("maxCount")) {
        return Err("has_let should return true".into());
    }
    let binding = module
        .get_let("maxCount")
        .ok_or("get_let should return Some")?;
    if binding.name != "maxCount" {
        return Err(format!("expected {:?} but got {:?}", "maxCount", binding.name).into());
    }

    if module.has_let("missing") {
        return Err("has_let should return false for unknown".into());
    }
    if module.get_let("missing").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// ResolvedType::display_name
// =============================================================================

#[test]
fn resolved_type_display_name_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ResolvedType;

    let source = "pub struct Rect { w: Number, h: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module.struct_id("Rect").ok_or("Rect should be found")?;
    let ty = ResolvedType::Struct(id);
    if ty.display_name(&module) != "Rect" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Rect",
            ty.display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_trait() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ResolvedType;

    let source = "pub trait Clickable { enabled: Boolean }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module
        .trait_id("Clickable")
        .ok_or("Clickable should be found")?;
    let ty = ResolvedType::Trait(id);
    if ty.display_name(&module) != "Clickable" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "Clickable",
            ty.display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_enum() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ResolvedType;

    let source = "pub enum State { idle, running }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let id = module.enum_id("State").ok_or("State should be found")?;
    let ty = ResolvedType::Enum(id);
    if ty.display_name(&module) != "State" {
        return Err(format!(
            "expected {:?} but got {:?}",
            "State",
            ty.display_name(&module)
        )
        .into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_array() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let inner = ResolvedType::Primitive(PrimitiveType::Number);
    let ty = ResolvedType::Array(Box::new(inner));
    let name = ty.display_name(&module);
    if name != "[Number]" {
        return Err(format!("expected {:?}, got {:?}", "[Number]", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_optional() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let inner = ResolvedType::Primitive(PrimitiveType::String);
    let ty = ResolvedType::Optional(Box::new(inner));
    let name = ty.display_name(&module);
    if name != "String?" {
        return Err(format!("expected {:?}, got {:?}", "String?", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_tuple() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::Tuple(vec![
        (
            "x".to_string(),
            ResolvedType::Primitive(PrimitiveType::Number),
        ),
        (
            "y".to_string(),
            ResolvedType::Primitive(PrimitiveType::Number),
        ),
    ]);
    let name = ty.display_name(&module);
    if !name.contains("x: Number") {
        return Err(format!("expected 'x: Number' in {name}").into());
    }
    if !name.contains("y: Number") {
        return Err(format!("expected 'y: Number' in {name}").into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_primitive_variants() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let cases = [
        (PrimitiveType::String, "String"),
        (PrimitiveType::Number, "Number"),
        (PrimitiveType::Boolean, "Boolean"),
        (PrimitiveType::Path, "Path"),
        (PrimitiveType::Regex, "Regex"),
        (PrimitiveType::F32, "f32"),
        (PrimitiveType::I32, "i32"),
        (PrimitiveType::U32, "u32"),
        (PrimitiveType::Bool, "bool"),
        (PrimitiveType::Vec2, "vec2"),
        (PrimitiveType::Vec3, "vec3"),
        (PrimitiveType::Vec4, "vec4"),
        (PrimitiveType::IVec2, "ivec2"),
        (PrimitiveType::IVec3, "ivec3"),
        (PrimitiveType::IVec4, "ivec4"),
        (PrimitiveType::UVec2, "uvec2"),
        (PrimitiveType::UVec3, "uvec3"),
        (PrimitiveType::UVec4, "uvec4"),
        (PrimitiveType::Mat2, "mat2"),
        (PrimitiveType::Mat3, "mat3"),
        (PrimitiveType::Mat4, "mat4"),
    ];
    for (prim, expected) in cases {
        let ty = ResolvedType::Primitive(prim);
        let got = ty.display_name(&module);
        if got != expected {
            return Err(format!("expected {expected} for {ty:?}, got {got:?}").into());
        }
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_type_param() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::TypeParam("T".to_string());
    let name = ty.display_name(&module);
    if name != "T" {
        return Err(format!("expected {:?}, got {:?}", "T", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_external_no_args() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::ExternalKind;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::External {
        module_path: vec!["utils".to_string()],
        name: "Helper".to_string(),
        kind: ExternalKind::Struct,
        type_args: vec![],
    };
    let name = ty.display_name(&module);
    if name != "Helper" {
        return Err(format!("expected {:?}, got {:?}", "Helper", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_external_with_args() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ir::ExternalKind;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::External {
        module_path: vec!["utils".to_string()],
        name: "Box".to_string(),
        kind: ExternalKind::Struct,
        type_args: vec![ResolvedType::Primitive(PrimitiveType::String)],
    };
    let name = ty.display_name(&module);
    if name != "Box<String>" {
        return Err(format!("expected {:?}, got {:?}", "Box<String>", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_event_mapping_no_param() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::EventMapping {
        param_ty: None,
        return_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Boolean)),
    };
    let name = ty.display_name(&module);
    if !name.contains("()") {
        return Err(format!("expected () in {name}").into());
    }
    if !name.contains("Boolean") {
        return Err(format!("expected Boolean in {name}").into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_event_mapping_with_param() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::EventMapping {
        param_ty: Some(Box::new(ResolvedType::Primitive(PrimitiveType::Number))),
        return_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Boolean)),
    };
    let name = ty.display_name(&module);
    if !name.contains("Number") {
        return Err(format!("expected Number in {name}").into());
    }
    if !name.contains("Boolean") {
        return Err(format!("expected Boolean in {name}").into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::Dictionary {
        key_ty: Box::new(ResolvedType::Primitive(PrimitiveType::String)),
        value_ty: Box::new(ResolvedType::Primitive(PrimitiveType::Number)),
    };
    let name = ty.display_name(&module);
    if name != "[String: Number]" {
        return Err(format!("expected {:?}, got {:?}", "[String: Number]", name).into());
    }
    Ok(())
}

#[test]
fn resolved_type_display_name_closure() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let module = formalang::ir::IrModule::new();
    let ty = ResolvedType::Closure {
        param_tys: vec![
            ResolvedType::Primitive(PrimitiveType::F32),
            ResolvedType::Primitive(PrimitiveType::F32),
        ],
        return_ty: Box::new(ResolvedType::Primitive(PrimitiveType::F32)),
    };
    let name = ty.display_name(&module);
    if name != "(f32, f32) -> f32" {
        return Err(format!("expected {:?}, got {:?}", "(f32, f32) -> f32", name).into());
    }
    Ok(())
}

// =============================================================================
// simple_type_name utility
// =============================================================================

#[test]
fn simple_type_name_strips_module_prefix() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::simple_type_name;

    let cases = [
        ("alignment::Horizontal", "Horizontal"),
        ("a::b::c::Foo", "Foo"),
        ("Button", "Button"),
        ("", ""),
    ];
    for (input, expected) in cases {
        let got = simple_type_name(input);
        if got != expected {
            return Err(format!("simple_type_name({input:?}): expected {expected:?}, got {got:?}").into());
        }
    }
    Ok(())
}

// =============================================================================
// IrModule::rebuild_indices
// =============================================================================

#[test]
fn irmodule_rebuild_indices_after_struct_filter() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::IrPass;

    struct FilterPass;
    impl IrPass for FilterPass {
        fn name(&self) -> &'static str {
            "filter"
        }
        fn run(
            &mut self,
            mut module: formalang::ir::IrModule,
        ) -> Result<formalang::ir::IrModule, Vec<formalang::CompilerError>> {
            module.structs.retain(|s| s.name == "Keep");
            module.rebuild_indices();
            Ok(module)
        }
    }

    let source = r"
        pub struct Keep { x: Number }
        pub struct Drop { y: Number }
    ";
    let ir = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = FilterPass
        .run(ir)
        .map_err(|e| format!("pass should succeed: {e:?}"))?;

    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    if result.struct_id("Keep").is_none() {
        return Err("struct 'Keep' should exist after filter pass".into());
    }
    if result.struct_id("Drop").is_some() {
        return Err("assertion failed".into());
    }
    Ok(())
}

// =============================================================================
// IrExpr::ty() method
// =============================================================================

#[test]
fn irexpr_ty_returns_type_from_literal() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let source = "struct A { x: Number = 42 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let s = module.structs.first().ok_or("no structs")?;
    let field = s.fields.first().ok_or("no fields")?;
    let expr = field.default.as_ref().ok_or("should have default")?;
    let ty = expr.ty();
    if ty != &ResolvedType::Primitive(PrimitiveType::Number) {
        return Err(format!(
            "expected {:?} but got {:?}",
            &ResolvedType::Primitive(PrimitiveType::Number),
            ty
        )
        .into());
    }
    Ok(())
}

#[test]
fn irexpr_ty_returns_type_from_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ResolvedType;

    let source = "struct A { x: Boolean = 1 < 2 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let s = module.structs.first().ok_or("no structs")?;
    let field = s.fields.first().ok_or("no fields")?;
    let expr = field.default.as_ref().ok_or("should have default")?;
    let ty = expr.ty();
    if ty != &ResolvedType::Primitive(PrimitiveType::Boolean) {
        return Err(format!(
            "expected {:?} but got {:?}",
            &ResolvedType::Primitive(PrimitiveType::Boolean),
            ty
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// IrBlockStatement::map_exprs
// =============================================================================

#[test]
fn ir_block_statement_map_exprs_let() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{IrBlockStatement, IrExpr};
    use formalang::ResolvedType;

    let stmt = IrBlockStatement::Let {
        name: "x".to_string(),
        mutable: false,
        ty: None,
        value: IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
    };

    let mapped = stmt.map_exprs(|_e| IrExpr::Literal {
        value: Literal::Number(99.0),
        ty: ResolvedType::Primitive(PrimitiveType::Number),
    });

    if let IrBlockStatement::Let { value, .. } = mapped {
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = value
        {
            if (n - 99.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 99.0, got {n}").into());
            }
        } else {
            return Err("Expected number literal after map_exprs".into());
        }
    } else {
        return Err("Expected Let statement after map_exprs".into());
    }
    Ok(())
}

#[test]
fn ir_block_statement_map_exprs_assign() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{IrBlockStatement, IrExpr};
    use formalang::ResolvedType;

    let stmt = IrBlockStatement::Assign {
        target: IrExpr::Literal {
            value: Literal::Number(0.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
        value: IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
    };

    let mut call_count = 0usize;
    let mapped = stmt.map_exprs(|e| {
        call_count += 1;
        e
    });

    if call_count != 2 {
        return Err(format!("map_exprs on Assign should call f twice (target + value): expected 2, got {call_count}").into());
    }
    if !matches!(mapped, IrBlockStatement::Assign { .. }) {
        return Err("Expected Assign statement after map_exprs".into());
    }
    Ok(())
}

#[test]
fn ir_block_statement_map_exprs_expr() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{IrBlockStatement, IrExpr};
    use formalang::ResolvedType;

    let stmt = IrBlockStatement::Expr(IrExpr::Literal {
        value: Literal::Boolean(false),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    });

    let mapped = stmt.map_exprs(|_e| IrExpr::Literal {
        value: Literal::Boolean(true),
        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
    });

    if let IrBlockStatement::Expr(IrExpr::Literal {
        value: Literal::Boolean(b),
        ..
    }) = mapped
    {
        if !(b) {
            return Err("Expected true after map_exprs".into());
        }
    } else {
        return Err("Expected Expr(Boolean(true)) after map_exprs".into());
    }
    Ok(())
}

// =============================================================================
// DCE: DeadCodeEliminator and eliminate_dead_code_expr
// =============================================================================

#[test]
fn dce_eliminate_dead_code_expr_constant_true_if() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{eliminate_dead_code_expr, IrExpr};
    use formalang::ResolvedType;

    let ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::If {
        condition: Box::new(IrExpr::Literal {
            value: Literal::Boolean(true),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        }),
        then_branch: Box::new(IrExpr::Literal {
            value: Literal::Number(10.0),
            ty: ty.clone(),
        }),
        else_branch: Some(Box::new(IrExpr::Literal {
            value: Literal::Number(20.0),
            ty: ty.clone(),
        })),
        ty,
    };

    let result = eliminate_dead_code_expr(expr);
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = result
    {
        if (n - 10.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 10.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected literal 10.0, got {result:?}").into());
    }
    Ok(())
}

#[test]
fn dce_eliminate_dead_code_expr_constant_false_if() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{eliminate_dead_code_expr, IrExpr};
    use formalang::ResolvedType;

    let ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::If {
        condition: Box::new(IrExpr::Literal {
            value: Literal::Boolean(false),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        }),
        then_branch: Box::new(IrExpr::Literal {
            value: Literal::Number(10.0),
            ty: ty.clone(),
        }),
        else_branch: Some(Box::new(IrExpr::Literal {
            value: Literal::Number(20.0),
            ty: ty.clone(),
        })),
        ty,
    };

    let result = eliminate_dead_code_expr(expr);
    if let IrExpr::Literal {
        value: Literal::Number(n),
        ..
    } = result
    {
        if (n - 20.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 20.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected literal 20.0, got {result:?}").into());
    }
    Ok(())
}

#[test]
fn dce_eliminate_dead_code_expr_no_else_false_preserved() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{eliminate_dead_code_expr, IrExpr};
    use formalang::ResolvedType;

    let ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::If {
        condition: Box::new(IrExpr::Literal {
            value: Literal::Boolean(false),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        }),
        then_branch: Box::new(IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: ty.clone(),
        }),
        else_branch: None,
        ty,
    };

    // false condition with no else: can't eliminate, keep as-is
    let result = eliminate_dead_code_expr(expr);
    if !matches!(result, IrExpr::If { .. }) {
        return Err(format!("Expected If to be preserved when no else branch, got {result:?}").into());
    }
    Ok(())
}

#[test]
fn dce_eliminate_dead_code_expr_binary_op_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{BinaryOperator, Literal, PrimitiveType};
    use formalang::ir::{eliminate_dead_code_expr, IrExpr};
    use formalang::ResolvedType;

    let ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::BinaryOp {
        left: Box::new(IrExpr::Literal {
            value: Literal::Number(3.0),
            ty: ty.clone(),
        }),
        op: BinaryOperator::Add,
        right: Box::new(IrExpr::Literal {
            value: Literal::Number(4.0),
            ty: ty.clone(),
        }),
        ty,
    };

    // DCE doesn't fold constants, just preserves BinaryOp
    let result = eliminate_dead_code_expr(expr);
    if !matches!(result, IrExpr::BinaryOp { .. }) {
        return Err(format!("Expected BinaryOp to pass through DCE unchanged, got {result:?}").into());
    }
    Ok(())
}

#[test]
fn dce_eliminate_dead_code_full_module() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        struct Cfg {
            value: Number = if true { 99 } else { 0 }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let optimized = eliminate_dead_code(&module, false);

    let os = optimized.structs.first().ok_or("no structs")?;
    let default = os.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = default
    {
        if (n - 99.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 99.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded 99.0, got {default:?}").into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_analyze_finds_used_structs_via_field_types(
) -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    // Container references Inner through its field type — Inner should be marked used
    let source = r"
        struct Inner { x: Number }
        struct Container { inner: Inner }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let inner_id = module.struct_id("Inner").ok_or("Inner should exist")?;
    if !(elim.is_struct_used(inner_id)) {
        return Err("Inner should be marked used because Container references it".into());
    }

    // used_structs() should expose the same set
    if !(elim.used_structs().contains(&inner_id)) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_unused_struct_not_marked() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Unused { x: Number }
        struct Other { y: Number }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;

    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let unused_id = module.struct_id("Unused").ok_or("Unused should exist")?;
    if elim.is_struct_used(unused_id) {
        return Err("Unused struct should not be marked used".into());
    }
    Ok(())
}

// =============================================================================
// Constant folding: fold_constants and ConstantFolder
// =============================================================================

#[test]
fn fold_constants_arithmetic_subtraction() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = 10 - 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n - 6.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 6.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_arithmetic_division() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = 10 / 2 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n - 5.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 5.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_arithmetic_modulo() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = 10 % 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n - 1.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 1.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded literal, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_comparison_lt_becomes_bool() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 1 < 2 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("1 < 2 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_comparison_ge_becomes_bool() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 5 >= 5 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("5 >= 5 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_boolean_and() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = true && false }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(!b) {
            return Err("true && false should fold to false".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_boolean_or() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = false || true }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("false || true should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_if_constant_true() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = if true { 7 } else { 8 } }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n - 7.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 7.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected 7.0, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_if_constant_false_with_else() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = if false { 7 } else { 8 } }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n - 8.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 8.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected 8.0, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_unary_negation() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Number = -5 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = expr
    {
        if (n + 5.0).abs() >= f64::EPSILON {
            return Err(format!("Expected -5.0, got {n}").into());
        }
    } else {
        return Err(format!("Expected folded -5.0, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_unary_not() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = !true }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(!b) {
            return Err("!true should fold to false".into());
        }
    } else {
        return Err(format!("Expected folded false, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_in_let_bindings() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "let scale: Number = 3 * 4";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let binding = folded
        .get_let("scale")
        .ok_or("scale let binding should exist")?;
    if let IrExpr::Literal {
        value: formalang::ast::Literal::Number(n),
        ..
    } = &binding.value
    {
        if (n - 12.0).abs() >= f64::EPSILON {
            return Err(format!("Expected 12.0, got {n}").into());
        }
    } else {
        return Err(format!(
            "Expected folded 12.0 in let binding, got {:?}",
            binding.value
        )
        .into());
    }
    Ok(())
}

#[test]
fn fold_constants_eq_comparison() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 3 == 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("3 == 3 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded true, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_ne_comparison() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 3 != 4 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("should have default")?;

    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("3 != 4 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded true, got {expr:?}").into());
    }
    Ok(())
}

// =============================================================================
// Visitor: walk_module, walk_expr, walk_module_children, walk_expr_children
// =============================================================================

#[test]
fn visitor_walk_module_visits_all_structs() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrStruct, IrVisitor, StructId};

    struct Counter(usize);
    impl IrVisitor for Counter {
        fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
            self.0 += 1;
        }
    }

    let source = r"
        struct A { x: Number }
        struct B { y: String }
        struct C { z: Boolean }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut counter = Counter(0);
    walk_module(&mut counter, &module);
    if counter.0 != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_module_visits_enums_and_variants() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrEnumVariant, IrVisitor};

    struct VariantCounter(usize);
    impl IrVisitor for VariantCounter {
        fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {
            self.0 += 1;
        }
    }

    let source = "enum Color { red, green, blue }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut counter = VariantCounter(0);
    walk_module(&mut counter, &module);
    if counter.0 != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_module_visits_fields() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrField, IrVisitor};

    struct FieldNameCollector(Vec<String>);
    impl IrVisitor for FieldNameCollector {
        fn visit_field(&mut self, f: &IrField) {
            self.0.push(f.name.clone());
        }
    }

    let source = "struct Point { x: Number, y: Number, z: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut collector = FieldNameCollector(Vec::new());
    walk_module(&mut collector, &module);
    if collector.0.len() != 3 {
        return Err(format!("expected {:?} but got {:?}", 3, collector.0.len()).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_module_visits_impls_and_functions() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrFunction, IrImpl, IrVisitor};

    struct ImplFnCounter {
        impls: usize,
        functions: usize,
    }
    impl IrVisitor for ImplFnCounter {
        fn visit_impl(&mut self, _i: &IrImpl) {
            self.impls += 1;
        }
        fn visit_function(&mut self, _f: &IrFunction) {
            self.functions += 1;
        }
    }

    let source = r"
        struct Vec2 { x: f32, y: f32 }
        impl Vec2 {
            fn length(self) -> f32 { self.x + self.y }
            fn scale(self, factor: f32) -> f32 { self.x * factor }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut counter = ImplFnCounter {
        impls: 0,
        functions: 0,
    };
    walk_module(&mut counter, &module);
    if counter.impls != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, counter.impls).into());
    }
    if counter.functions != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, counter.functions).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_module_visits_let_bindings() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrLet, IrVisitor};

    struct LetCounter(usize);
    impl IrVisitor for LetCounter {
        fn visit_let(&mut self, _l: &IrLet) {
            self.0 += 1;
        }
    }

    let source = r"
        let a: Number = 1
        let b: Number = 2
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut counter = LetCounter(0);
    walk_module(&mut counter, &module);
    if counter.0 != 2 {
        return Err(format!("expected {:?} but got {:?}", 2, counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_sub_expressions() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            // still walk children
            walk_expr_children(self, e);
        }
    }

    use formalang::ast::{BinaryOperator, Literal, PrimitiveType};
    use formalang::ResolvedType;

    let ty = ResolvedType::Primitive(PrimitiveType::Number);
    let expr = IrExpr::BinaryOp {
        left: Box::new(IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: ty.clone(),
        }),
        op: BinaryOperator::Add,
        right: Box::new(IrExpr::Literal {
            value: Literal::Number(2.0),
            ty: ty.clone(),
        }),
        ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 2 {
        return Err(format!("Expected 2 literal sub-expressions, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_custom_visit_module_override_skips_children() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_module, IrModule, IrStruct, IrVisitor, StructId};

    // Override visit_module to NOT walk children — nothing should be visited
    struct NoOpVisitor(usize);
    impl IrVisitor for NoOpVisitor {
        fn visit_module(&mut self, _module: &IrModule) {
            // intentionally skip walk_module_children
        }
        fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
            self.0 += 1;
        }
    }

    let source = "struct A { x: Number }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut visitor = NoOpVisitor(0);
    walk_module(&mut visitor, &module);
    if visitor.0 != 0 {
        return Err(format!("expected {:?} but got {:?}", 0, visitor.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_block_statement_visits_let_value() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_block_statement, IrBlockStatement, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct ExprCounter(usize);
    impl IrVisitor for ExprCounter {
        fn visit_expr(&mut self, _e: &IrExpr) {
            self.0 += 1;
        }
    }

    let stmt = IrBlockStatement::Let {
        name: "x".to_string(),
        mutable: false,
        ty: None,
        value: IrExpr::Literal {
            value: Literal::Number(42.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
    };

    let mut counter = ExprCounter(0);
    walk_block_statement(&mut counter, &stmt);
    if counter.0 != 1 {
        return Err(format!("Expected 1 expr visited for Let statement, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_block_statement_visits_assign_both_sides() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_block_statement, IrBlockStatement, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct ExprCounter(usize);
    impl IrVisitor for ExprCounter {
        fn visit_expr(&mut self, _e: &IrExpr) {
            self.0 += 1;
        }
    }

    let stmt = IrBlockStatement::Assign {
        target: IrExpr::Literal {
            value: Literal::Number(0.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
        value: IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: ResolvedType::Primitive(PrimitiveType::Number),
        },
    };

    let mut counter = ExprCounter(0);
    walk_block_statement(&mut counter, &stmt);
    if counter.0 != 2 {
        return Err(format!("Expected 2 exprs for Assign (target + value), got {}", counter.0).into());
    }
    Ok(())
}

// =============================================================================
// Error: CompilerError::span() covers all variants
// =============================================================================

#[test]
fn compiler_error_span_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let err = CompilerError::ParseError {
        message: "test".to_string(),
        span: Span::default(),
    };
    let span = err.span();
    if span.start.offset > span.end.offset {
        return Err("span should be valid".into());
    }
    Ok(())
}

#[test]
fn compiler_error_span_undefined_reference() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let err = CompilerError::UndefinedReference {
        name: "foo".to_string(),
        span: Span::default(),
    };
    let span = err.span();
    if span.start.offset > span.end.offset {
        return Err("span should be valid".into());
    }
    Ok(())
}

#[test]
fn compiler_error_display_messages() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let cases: Vec<CompilerError> = vec![
        CompilerError::InvalidCharacter {
            character: '@',
            span: Span::default(),
        },
        CompilerError::UnterminatedString {
            span: Span::default(),
        },
        CompilerError::InvalidNumber {
            value: "abc".to_string(),
            span: Span::default(),
        },
        CompilerError::MixedIndentation {
            span: Span::default(),
        },
        CompilerError::UnexpectedToken {
            expected: "ident".to_string(),
            found: "number".to_string(),
            span: Span::default(),
        },
        CompilerError::DuplicateDefinition {
            name: "Foo".to_string(),
            span: Span::default(),
        },
        CompilerError::TypeMismatch {
            expected: "Number".to_string(),
            found: "String".to_string(),
            span: Span::default(),
        },
        CompilerError::UndefinedType {
            name: "Xyz".to_string(),
            span: Span::default(),
        },
        CompilerError::UndefinedTrait {
            name: "Abc".to_string(),
            span: Span::default(),
        },
        CompilerError::CircularDependency {
            cycle: "A->B->A".to_string(),
            span: Span::default(),
        },
        CompilerError::TooManyDefinitions {
            kind: "struct",
            span: Span::default(),
        },
        CompilerError::ExpressionDepthExceeded {
            span: Span::default(),
        },
    ];

    for err in &cases {
        let msg = format!("{err}");
        if msg.is_empty() {
            return Err(format!("Display for {err:?} should not be empty").into());
        }
        let span = err.span();
        if span.start.offset > span.end.offset {
            return Err("span should be valid".into());
        }
    }
    Ok(())
}

#[test]
fn compiler_error_module_errors_display() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let cases: Vec<CompilerError> = vec![
        CompilerError::ModuleNotFound {
            name: "mod".to_string(),
            span: Span::default(),
        },
        CompilerError::ModuleReadError {
            path: "/x".to_string(),
            error: "io".to_string(),
            span: Span::default(),
        },
        CompilerError::CircularImport {
            cycle: "x->y->x".to_string(),
            span: Span::default(),
        },
        CompilerError::PrivateImport {
            name: "secret".to_string(),
            span: Span::default(),
        },
        CompilerError::ImportItemNotFound {
            item: "Foo".to_string(),
            module: "bar".to_string(),
            available: "Baz".to_string(),
            span: Span::default(),
        },
    ];

    for err in &cases {
        let msg = format!("{err}");
        if msg.is_empty() {
            return Err(format!("Display for {err:?} should not be empty").into());
        }
        let span = err.span();
        if span.start.offset > span.end.offset {
            return Err("span should be valid".into());
        }
    }
    Ok(())
}

#[test]
fn compiler_error_trait_errors_display() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let cases: Vec<CompilerError> = vec![
        CompilerError::MissingTraitField {
            field: "x".to_string(),
            trait_name: "T".to_string(),
            span: Span::default(),
        },
        CompilerError::TraitFieldTypeMismatch {
            field: "x".to_string(),
            trait_name: "T".to_string(),
            expected: "Number".to_string(),
            actual: "String".to_string(),
            span: Span::default(),
        },
        CompilerError::ModelTraitWithMountingPoints {
            name: "M".to_string(),
            span: Span::default(),
        },
        CompilerError::ViewTraitInModel {
            name: "V".to_string(),
            model: "M".to_string(),
            span: Span::default(),
        },
        CompilerError::ModelTraitInView {
            name: "M".to_string(),
            view: "V".to_string(),
            span: Span::default(),
        },
        CompilerError::MissingTraitMountingPoint {
            mount: "children".to_string(),
            trait_name: "T".to_string(),
            span: Span::default(),
        },
        CompilerError::TraitMountingPointTypeMismatch {
            mount: "children".to_string(),
            trait_name: "T".to_string(),
            expected: "[View]".to_string(),
            actual: "[String]".to_string(),
            span: Span::default(),
        },
    ];

    for err in &cases {
        let msg = format!("{err}");
        if msg.is_empty() {
            return Err(format!("Display for {err:?} should not be empty").into());
        }
        let span = err.span();
        if span.start.offset > span.end.offset {
            return Err("span should be valid".into());
        }
    }
    Ok(())
}

#[test]
#[expect(clippy::too_many_lines, reason = "comprehensive error display test")]
fn compiler_error_expression_errors_display() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::Span;
    use formalang::CompilerError;

    let cases: Vec<CompilerError> = vec![
        CompilerError::InvalidBinaryOp {
            op: "+".to_string(),
            left_type: "Boolean".to_string(),
            right_type: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::ForLoopNotArray {
            actual: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::InvalidIfCondition {
            actual: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::MatchNotEnum {
            actual: "String".to_string(),
            span: Span::default(),
        },
        CompilerError::NonExhaustiveMatch {
            missing: "Active".to_string(),
            span: Span::default(),
        },
        CompilerError::DuplicateMatchArm {
            variant: "Active".to_string(),
            span: Span::default(),
        },
        CompilerError::UnknownEnumVariant {
            variant: "Foo".to_string(),
            enum_name: "Status".to_string(),
            span: Span::default(),
        },
        CompilerError::VariantArityMismatch {
            variant: "X".to_string(),
            expected: 1,
            actual: 0,
            span: Span::default(),
        },
        CompilerError::MissingField {
            field: "x".to_string(),
            type_name: "Point".to_string(),
            span: Span::default(),
        },
        CompilerError::UnknownField {
            field: "z".to_string(),
            type_name: "Point".to_string(),
            span: Span::default(),
        },
        CompilerError::AssignmentToImmutable {
            span: Span::default(),
        },
        CompilerError::PositionalArgInStruct {
            struct_name: "S".to_string(),
            position: 0,
            span: Span::default(),
        },
        CompilerError::EnumVariantWithoutData {
            variant: "V".to_string(),
            enum_name: "E".to_string(),
            span: Span::default(),
        },
        CompilerError::EnumVariantRequiresData {
            variant: "V".to_string(),
            enum_name: "E".to_string(),
            span: Span::default(),
        },
        CompilerError::MutabilityMismatch {
            param: "p".to_string(),
            span: Span::default(),
        },
        CompilerError::GenericArityMismatch {
            name: "Box".to_string(),
            expected: 1,
            actual: 2,
            span: Span::default(),
        },
        CompilerError::GenericConstraintViolation {
            arg: "T".to_string(),
            constraint: "Sized".to_string(),
            span: Span::default(),
        },
        CompilerError::OutOfScopeTypeParameter {
            param: "T".to_string(),
            span: Span::default(),
        },
        CompilerError::MissingGenericArguments {
            name: "Box".to_string(),
            span: Span::default(),
        },
        CompilerError::DuplicateGenericParam {
            param: "T".to_string(),
            span: Span::default(),
        },
        CompilerError::UnknownMount {
            mount: "header".to_string(),
            struct_name: "Card".to_string(),
            span: Span::default(),
        },
        CompilerError::CannotInferEnumType {
            variant: "active".to_string(),
            span: Span::default(),
        },
        CompilerError::FunctionReturnTypeMismatch {
            function: "foo".to_string(),
            expected: "Number".to_string(),
            actual: "String".to_string(),
            span: Span::default(),
        },
        CompilerError::ArrayDestructuringNotArray {
            actual: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::StructDestructuringNotStruct {
            actual: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::InvalidIndentation {
            span: Span::default(),
        },
        CompilerError::UnexpectedEof {
            span: Span::default(),
        },
        CompilerError::UndefinedComponent {
            name: "Xyz".to_string(),
            span: Span::default(),
        },
        CompilerError::MountingPointOnSameLine {
            mounting_point: "header".to_string(),
            span: Span::default(),
        },
        CompilerError::PropertyAfterMountingPoint {
            property: "x".to_string(),
            span: Span::default(),
        },
        CompilerError::UnknownProperty {
            component: "Button".to_string(),
            property: "foo".to_string(),
            span: Span::default(),
        },
        CompilerError::MissingRequiredProperty {
            component: "Button".to_string(),
            property: "label".to_string(),
            span: Span::default(),
        },
        CompilerError::InvalidPropertyValue {
            property: "size".to_string(),
            message: "must be positive".to_string(),
            span: Span::default(),
        },
        CompilerError::UnknownMountingPoint {
            component: "Card".to_string(),
            mounting_point: "footer".to_string(),
            span: Span::default(),
        },
        CompilerError::InvalidMountingPointChild {
            component: "Card".to_string(),
            mounting_point: "body".to_string(),
            child_type: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::InvalidComponentPosition {
            component: "Card".to_string(),
            message: "must be nested".to_string(),
            span: Span::default(),
        },
        CompilerError::PrimitiveRedefinition {
            name: "Number".to_string(),
            span: Span::default(),
        },
        CompilerError::NotATrait {
            name: "Point".to_string(),
            actual_kind: "struct".to_string(),
            span: Span::default(),
        },
    ];

    for err in &cases {
        let msg = format!("{err}");
        if msg.is_empty() {
            return Err(format!("Display for {err:?} should not be empty").into());
        }
        let span = err.span();
        if span.start.offset > span.end.offset {
            return Err("span should be valid".into());
        }
    }
    Ok(())
}

// =============================================================================
// DCE: exercise more eliminate_dead_code_expr variants via full module paths
// =============================================================================

#[test]
fn dce_via_pipeline_match_expression() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r#"
        enum Status { active, inactive }
        struct A {
            label: String = match Status.active {
                .active: "yes",
                .inactive: "no"
            }
        }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_enum_inst_in_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        enum Color { red, green(r: Number) }
        struct A { c: Color = Color.green(r: 1) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_for_loop_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    // Use a struct default with for loop to exercise For branch in DCE
    let source = r"
        struct A { items: [Number] = for x in [1, 2, 3] { x } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_tuple_in_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        struct A { t: (x: Number, y: Number) = (x: 1, y: 2) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_block_expression_in_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        struct A {
            x: Number = (
                let a = 1
                let b = 2
                a + b
            )
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_array_in_let() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    let source = r"
        let nums: [Number] = [1, 2, 3]
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, false);
    if result.lets.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.lets.len()).into());
    }
    Ok(())
}

#[test]
fn dce_analyzes_used_structs_via_impl_blocks() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Config { value: Number }
        impl Config {}
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let id = module.struct_id("Config").ok_or("Config should exist")?;
    if !(elim.is_struct_used(id)) {
        return Err("Config should be marked used because it has an impl block".into());
    }
    Ok(())
}

#[test]
fn dce_analyzes_used_structs_in_struct_inst_expr() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    let source = r"
        struct Inner { x: Number }
        let val: Inner = Inner(x: 42)
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let id = module.struct_id("Inner").ok_or("Inner should exist")?;
    if !(elim.is_struct_used(id)) {
        return Err("Inner should be marked used through struct instantiation in let".into());
    }
    Ok(())
}

// =============================================================================
// Fold: cover more expression variants in fold_constants
// =============================================================================

#[test]
fn fold_constants_gt_comparison() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 5 > 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("default")?;
    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("5 > 3 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_le_comparison() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = 3 <= 3 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("default")?;
    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("3 <= 3 should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_boolean_eq() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = true == true }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("default")?;
    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("true == true should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_boolean_ne() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { x: Boolean = true != false }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let expr = fs.fields.first().ok_or("no fields")?
        .default
        .as_ref()
        .ok_or("default")?;
    if let IrExpr::Literal {
        value: formalang::ast::Literal::Boolean(b),
        ..
    } = expr
    {
        if !(*b) {
            return Err("true != false should fold to true".into());
        }
    } else {
        return Err(format!("Expected folded boolean, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_array_with_constant_elements() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "let arr: [Number] = [1 + 1, 2 * 2]";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let binding = folded.get_let("arr").ok_or("arr should exist")?;
    if let IrExpr::Array { elements, .. } = &binding.value {
        if elements.len() != 2 {
            return Err(format!("expected {:?} but got {:?}", 2, elements.len()).into());
        }
        for e in elements {
            if !(matches!(e, IrExpr::Literal { .. })) {
                return Err(format!("Element should be literal: {e:?}").into());
            }
        }
    } else {
        return Err(format!("Expected Array expr, got {:?}", binding.value).into());
    }
    Ok(())
}

#[test]
fn fold_constants_tuple_with_constant_elements() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = "struct A { t: (x: Number, y: Number) = (x: 1 + 1, y: 2 + 2) }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let fs = folded.structs.first().ok_or("no structs")?;
    let field = fs.fields.first().ok_or("no fields")?;
    let expr = field.default.as_ref().ok_or("default")?;
    if let IrExpr::Tuple { fields, .. } = expr {
        for (_, e) in fields {
            if !(matches!(e, IrExpr::Literal { .. })) {
                return Err(format!("Tuple field should be literal: {e:?}").into());
            }
        }
    } else {
        return Err(format!("Expected Tuple expr, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_match_expression_preserved() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = r#"
        enum Status { ok, err }
        struct A {
            label: String = match Status.ok {
                .ok: "yes",
                .err: "no"
            }
        }
    "#;
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, folded.structs.len()).into());
    }
    Ok(())
}

#[test]
fn fold_constants_struct_inst_in_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = r"
        struct Inner { x: Number }
        struct Outer { i: Inner = Inner(x: 2 + 3) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);

    let outer = folded
        .structs
        .iter()
        .find(|s| s.name == "Outer")
        .ok_or("Outer")?;
    let field = outer.fields.first().ok_or("no fields in Outer")?;
    let expr = field.default.as_ref().ok_or("default")?;
    if let IrExpr::StructInst { fields, .. } = expr {
        let (_, x_val) = fields.first().ok_or("no fields in StructInst")?;
        if let IrExpr::Literal {
            value: formalang::ast::Literal::Number(n),
            ..
        } = x_val
        {
            if (n - 5.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 5.0 from 2+3, got {n}").into());
            }
        } else {
            return Err(format!("Expected folded 5.0, got {x_val:?}").into());
        }
    } else {
        return Err(format!("Expected StructInst, got {expr:?}").into());
    }
    Ok(())
}

#[test]
fn fold_constants_method_call_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = r"
        struct Vec2 { x: f32, y: f32 }
        impl Vec2 {
            fn len(self) -> f32 { self.x + self.y }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, folded.structs.len()).into());
    }
    Ok(())
}

#[test]
fn fold_constants_block_expression_in_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    let source = r"
        struct A {
            x: Number = (
                let a = 2 + 3
                let b = 4 * 2
                a + b
            )
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, folded.structs.len()).into());
    }
    Ok(())
}

#[test]
fn fold_constants_for_loop_in_impl() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::fold_constants;

    // Use struct field default to exercise For fold path without the return-type complexity
    let source = r"
        struct A { items: [Number] = for x in [1, 2] { x } }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let folded = fold_constants(&module);
    if folded.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, folded.structs.len()).into());
    }
    Ok(())
}

// =============================================================================
// Visitor: cover more expression variants via walk_expr_children
// =============================================================================

#[test]
fn visitor_walk_expr_visits_if_branches() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let bool_ty = ResolvedType::Primitive(PrimitiveType::Boolean);
    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::If {
        condition: Box::new(IrExpr::Literal {
            value: Literal::Boolean(true),
            ty: bool_ty,
        }),
        then_branch: Box::new(IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: num_ty.clone(),
        }),
        else_branch: Some(Box::new(IrExpr::Literal {
            value: Literal::Number(2.0),
            ty: num_ty.clone(),
        })),
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 3 {
        return Err(format!("Expected condition + then + else = 3 literals, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_for_loop() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::For {
        var: "x".to_string(),
        var_ty: num_ty.clone(),
        collection: Box::new(IrExpr::Array {
            elements: vec![
                IrExpr::Literal {
                    value: Literal::Number(1.0),
                    ty: num_ty.clone(),
                },
                IrExpr::Literal {
                    value: Literal::Number(2.0),
                    ty: num_ty.clone(),
                },
            ],
            ty: ResolvedType::Array(Box::new(num_ty.clone())),
        }),
        body: Box::new(IrExpr::Literal {
            value: Literal::Number(0.0),
            ty: num_ty.clone(),
        }),
        ty: ResolvedType::Array(Box::new(num_ty)),
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    // 2 in array collection + 1 body literal = 3
    if counter.0 != 3 {
        return Err(format!("Expected 3 literals from for loop walk, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_match_arms() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrMatchArm, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);
    let str_ty = ResolvedType::Primitive(PrimitiveType::String);

    let expr = IrExpr::Match {
        scrutinee: Box::new(IrExpr::Literal {
            value: Literal::Number(1.0),
            ty: num_ty,
        }),
        arms: vec![
            IrMatchArm {
                variant: "a".to_string(),
                is_wildcard: false,
                bindings: vec![],
                body: IrExpr::Literal {
                    value: Literal::String("x".to_string()),
                    ty: str_ty.clone(),
                },
            },
            IrMatchArm {
                variant: "b".to_string(),
                is_wildcard: false,
                bindings: vec![],
                body: IrExpr::Literal {
                    value: Literal::String("y".to_string()),
                    ty: str_ty.clone(),
                },
            },
        ],
        ty: str_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    // scrutinee (1) + arm a body (1) + arm b body (1) = 3
    if counter.0 != 3 {
        return Err(format!("Expected 3 literals from match walk, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_function_call_args() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::FunctionCall {
        path: vec!["math".to_string(), "add".to_string()],
        args: vec![
            (
                Some("a".to_string()),
                IrExpr::Literal {
                    value: Literal::Number(1.0),
                    ty: num_ty.clone(),
                },
            ),
            (
                Some("b".to_string()),
                IrExpr::Literal {
                    value: Literal::Number(2.0),
                    ty: num_ty.clone(),
                },
            ),
        ],
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 2 {
        return Err(format!("Expected 2 args in function call, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_method_call_receiver_and_args() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::MethodCall {
        receiver: Box::new(IrExpr::Literal {
            value: Literal::Number(0.0),
            ty: num_ty.clone(),
        }),
        method: "scale".to_string(),
        args: vec![(
            Some("factor".to_string()),
            IrExpr::Literal {
                value: Literal::Number(2.0),
                ty: num_ty.clone(),
            },
        )],
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 2 {
        return Err(format!("Expected receiver + 1 arg = 2 literals, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_dict_literal_entries() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let str_ty = ResolvedType::Primitive(PrimitiveType::String);
    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::DictLiteral {
        entries: vec![(
            IrExpr::Literal {
                value: Literal::String("key".to_string()),
                ty: str_ty.clone(),
            },
            IrExpr::Literal {
                value: Literal::Number(1.0),
                ty: num_ty.clone(),
            },
        )],
        ty: ResolvedType::Dictionary {
            key_ty: Box::new(str_ty),
            value_ty: Box::new(num_ty),
        },
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 2 {
        return Err(format!("Expected key + value = 2 literals, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_dict_access() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let str_ty = ResolvedType::Primitive(PrimitiveType::String);
    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let dict_expr = IrExpr::DictLiteral {
        entries: vec![],
        ty: ResolvedType::Dictionary {
            key_ty: Box::new(str_ty.clone()),
            value_ty: Box::new(num_ty.clone()),
        },
    };

    let expr = IrExpr::DictAccess {
        dict: Box::new(dict_expr),
        key: Box::new(IrExpr::Literal {
            value: Literal::String("k".to_string()),
            ty: str_ty,
        }),
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    // key is a literal: 1 literal
    if counter.0 != 1 {
        return Err(format!("Expected 1 literal from dict access key, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_closure_body() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::Closure {
        params: vec![("x".to_string(), num_ty.clone())],
        body: Box::new(IrExpr::Literal {
            value: Literal::Number(42.0),
            ty: num_ty.clone(),
        }),
        ty: ResolvedType::Closure {
            param_tys: vec![num_ty.clone()],
            return_ty: Box::new(num_ty),
        },
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 1 {
        return Err(format!("Expected 1 literal from closure body, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_visits_event_mapping_no_children() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::PrimitiveType;
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct AnyCounter(usize);
    impl IrVisitor for AnyCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 += 1;
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);
    let bool_ty = ResolvedType::Primitive(PrimitiveType::Boolean);

    let expr = IrExpr::EventMapping {
        enum_id: None,
        variant: "changed".to_string(),
        param: Some("x".to_string()),
        field_bindings: vec![],
        ty: ResolvedType::EventMapping {
            param_ty: Some(Box::new(num_ty)),
            return_ty: Box::new(bool_ty),
        },
    };

    let mut counter = AnyCounter(0);
    walk_expr(&mut counter, &expr);
    // EventMapping has no child expressions to walk, so only 1 visit (itself)
    if counter.0 != 1 {
        return Err(format!("EventMapping has no children, so only 1 visit, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_field_access_child() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::FieldAccess {
        object: Box::new(IrExpr::Literal {
            value: Literal::Number(0.0),
            ty: num_ty.clone(),
        }),
        field: "x".to_string(),
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 1 {
        return Err(format!("Expected 1 literal from field access object, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_unary_op_child() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType, UnaryOperator};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::UnaryOp {
        op: UnaryOperator::Neg,
        operand: Box::new(IrExpr::Literal {
            value: Literal::Number(5.0),
            ty: num_ty.clone(),
        }),
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 1 {
        return Err(format!("Expected 1 literal from unary op operand, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_block_statements_and_result() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrBlockStatement, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::Block {
        statements: vec![IrBlockStatement::Let {
            name: "a".to_string(),
            mutable: false,
            ty: None,
            value: IrExpr::Literal {
                value: Literal::Number(1.0),
                ty: num_ty.clone(),
            },
        }],
        result: Box::new(IrExpr::Literal {
            value: Literal::Number(2.0),
            ty: num_ty.clone(),
        }),
        ty: num_ty,
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    // statement value (1) + result (1) = 2
    if counter.0 != 2 {
        return Err(format!("Expected 2 literals from block (stmt + result), got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_expr_enum_inst_fields() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::EnumInst {
        enum_id: None,
        variant: "active".to_string(),
        fields: vec![(
            "count".to_string(),
            IrExpr::Literal {
                value: Literal::Number(3.0),
                ty: num_ty,
            },
        )],
        ty: ResolvedType::TypeParam("E".to_string()),
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    if counter.0 != 1 {
        return Err(format!("Expected 1 literal from enum inst field, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_module_visits_struct_field_defaults() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{walk_expr_children, walk_module, IrExpr, IrVisitor};

    struct ExprCounter(usize);
    impl IrVisitor for ExprCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 += 1;
            walk_expr_children(self, e);
        }
    }

    let source = "struct A { x: Number = 1 + 2 }";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut counter = ExprCounter(0);
    walk_module(&mut counter, &module);
    // "1 + 2" produces at least 3 expressions: BinaryOp + Literal(1) + Literal(2)
    if counter.0 < 3 {
        return Err(format!(
            "Expected at least 3 expressions walked (BinaryOp + 2 Literals), got {}",
            counter.0
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// Additional coverage: visitor mount fields, block stmt Expr, DCE type variants
// =============================================================================

#[test]
fn visitor_walk_block_statement_expr_variant() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{
        walk_block_statement, walk_expr_children, IrBlockStatement, IrExpr, IrVisitor,
    };
    use formalang::ResolvedType;

    // Specifically exercises IrBlockStatement::Expr in walk_block_statement
    struct ExprCounter(usize);
    impl IrVisitor for ExprCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 += 1;
            walk_expr_children(self, e);
        }
    }

    let stmt = IrBlockStatement::Expr(IrExpr::Literal {
        value: Literal::Number(1.0),
        ty: ResolvedType::Primitive(PrimitiveType::Number),
    });

    let mut counter = ExprCounter(0);
    walk_block_statement(&mut counter, &stmt);
    if counter.0 != 1 {
        return Err(format!("Expected 1 expression from Expr statement, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn visitor_walk_struct_inst_with_mounts() -> Result<(), Box<dyn std::error::Error>> {
    // Exercises the mounts branch in walk_expr_children for StructInst
    use formalang::ast::{Literal, PrimitiveType};
    use formalang::ir::{walk_expr, walk_expr_children, IrExpr, IrVisitor};
    use formalang::ResolvedType;

    struct LiteralCounter(usize);
    impl IrVisitor for LiteralCounter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e, IrExpr::Literal { .. }) {
                self.0 += 1;
            }
            walk_expr_children(self, e);
        }
    }

    let num_ty = ResolvedType::Primitive(PrimitiveType::Number);

    let expr = IrExpr::StructInst {
        struct_id: None,
        type_args: vec![],
        fields: vec![(
            "x".to_string(),
            IrExpr::Literal {
                value: Literal::Number(1.0),
                ty: num_ty.clone(),
            },
        )],
        mounts: vec![(
            "children".to_string(),
            IrExpr::Literal {
                value: Literal::Number(2.0),
                ty: num_ty,
            },
        )],
        ty: ResolvedType::TypeParam("S".to_string()),
    };

    let mut counter = LiteralCounter(0);
    walk_expr(&mut counter, &expr);
    // 1 field + 1 mount = 2 literals
    if counter.0 != 2 {
        return Err(format!("Expected 2 literals (field + mount) from StructInst, got {}", counter.0).into());
    }
    Ok(())
}

#[test]
fn dce_default_pass_has_remove_unused_structs_enabled() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminationPass;

    let pass = DeadCodeEliminationPass::default();
    if !pass.remove_unused_structs {
        return Err("default() should have remove_unused_structs = true".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_generic_type_marks_base_struct() -> Result<(), Box<dyn std::error::Error>> {
    
    use formalang::ir::DeadCodeEliminator;
    
    

    // Create a module and test the mark_used_in_type method for Generic variant
    // We do this by compiling a source that has a generic field type
    let source = r"
        struct Box<T> { value: T }
        struct Container { inner: Box<Number> }
    ";
    let module = compile_to_ir(source)
        .map_err(|e| format!("generic struct fields should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();
    let box_id = module
        .struct_id("Box")
        .ok_or("Box should be registered in IR")?;
    if !(elim.is_struct_used(box_id)) {
        return Err("Box should be marked used when Container references it generically".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_array_type_marks_inner_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    // Array type field: Container has [Item] — marks Item as used
    let source = r"
        struct Item { x: Number }
        struct Container { items: [Item] }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let item_id = module.struct_id("Item").ok_or("Item should exist")?;
    if !(elim.is_struct_used(item_id)) {
        return Err("Item should be marked used through [Item] field type in Container".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_optional_type_marks_inner_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    // Optional field: Container has Item? — marks Item as used
    let source = r"
        struct Item { x: Number }
        struct Container { item: Item? }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let item_id = module.struct_id("Item").ok_or("Item should exist")?;
    if !(elim.is_struct_used(item_id)) {
        return Err("Item should be marked used through Item? field type".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_tuple_field_types_marks_structs() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    // Tuple field containing a struct type
    let source = r"
        struct Inner { x: Number }
        struct Outer { pair: (a: Inner, b: Number) }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();

    let inner_id = module.struct_id("Inner").ok_or("Inner should exist")?;
    if !(elim.is_struct_used(inner_id)) {
        return Err("Inner should be marked used through tuple field type".into());
    }
    Ok(())
}

#[test]
fn dce_eliminator_dict_field_type_marks_struct() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::DeadCodeEliminator;

    // Dictionary field type: Container has [String: Item] — marks Item as used via value_ty
    let source = r"
        struct Item { x: Number }
        struct Container { lookup: [String: Item] }
    ";
    let module =
        compile_to_ir(source).map_err(|e| format!("dict field struct should compile: {e:?}"))?;
    let mut elim = DeadCodeEliminator::new(&module);
    elim.analyze();
    let item_id = module
        .struct_id("Item")
        .ok_or("Item should be registered in IR")?;
    if !(elim.is_struct_used(item_id)) {
        return Err("Item should be marked used through [String: Item] field type".into());
    }
    Ok(())
}

#[test]
fn dce_via_pipeline_method_call_with_struct_receiver() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::eliminate_dead_code;

    // This exercises the impl block processing path in eliminate_dead_code
    let source = r"
        struct V2 { x: f32, y: f32 }
        impl V2 {
            fn len(self) -> f32 { self.x + self.y }
        }
    ";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let result = eliminate_dead_code(&module, true);
    if result.structs.len() != 1 {
        return Err(format!("expected {:?} but got {:?}", 1, result.structs.len()).into());
    }
    Ok(())
}

#[test]
fn dce_pass_name_and_default() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::{ConstantFoldingPass, DeadCodeEliminationPass};
    use formalang::IrPass;

    let dce = DeadCodeEliminationPass {
        remove_unused_structs: true,
    };
    if dce.name() != "dead-code-elimination" {
        return Err(format!("Expected 'dead-code-elimination', got {:?}", dce.name()).into());
    }

    let fold = ConstantFoldingPass {};
    if fold.name() != "constant-folding" {
        return Err(format!("Expected 'constant-folding', got {:?}", fold.name()).into());
    }
    Ok(())
}

#[test]
fn irexpr_ty_covers_all_variants_via_ir() -> Result<(), Box<dyn std::error::Error>> {
    

    // Array
    let source = "let arr: [Number] = [1, 2, 3]";
    let module = compile_to_ir(source).map_err(|e| format!("should compile: {e:?}"))?;
    let binding = module.get_let("arr").ok_or("arr")?;
    let ty = binding.value.ty();
    if !(matches!(ty, formalang::ResolvedType::Array(_))) {
        return Err("assertion failed".into());
    }

    // Tuple
    let source2 = "struct A { t: (x: Number, y: Number) = (x: 1, y: 2) }";
    let module2 = compile_to_ir(source2).map_err(|e| format!("should compile: {e:?}"))?;
    let s2 = module2.structs.first().ok_or("no structs")?;
    let field = s2.fields.first().ok_or("no fields")?;
    let expr = field.default.as_ref().ok_or("default")?;
    let ty2 = expr.ty();
    if !(matches!(ty2, formalang::ResolvedType::Tuple(_))) {
        return Err("assertion failed".into());
    }
    Ok(())
}

#[test]
fn visibility_is_public_method() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ast::Visibility;

    if !Visibility::Public.is_public() {
        return Err("Visibility::Public.is_public() should be true".into());
    }
    if Visibility::Private.is_public() {
        return Err("Visibility::Private.is_public() should be false".into());
    }
    Ok(())
}

#[test]
fn fold_constants_default_pass() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::ir::ConstantFoldingPass;
    use formalang::IrPass;

    let pass = ConstantFoldingPass {};
    if pass.name() != "constant-folding" {
        return Err(format!("Expected 'constant-folding', got {:?}", pass.name()).into());
    }
    Ok(())
}
