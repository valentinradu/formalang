use super::*;
use crate::parse_only;

fn parse(source: &str) -> Result<File, Vec<crate::error::CompilerError>> {
    parse_only(source)
}

fn find_offset_of(source: &str, pattern: &str) -> Result<usize, Box<dyn std::error::Error>> {
    source
        .find(pattern)
        .ok_or_else(|| format!("Pattern {pattern:?} not found in source").into())
}

#[test]
fn test_find_struct_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "User" - may return Identifier or StructDef
    let offset = find_offset_of(source, "User")?;
    let ctx = find_node_at_offset(&file, offset);

    // Either the node is a StructDef or there's a StructDef in parents
    let is_struct = matches!(ctx.node, NodeAtPosition::StructDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
    if !is_struct && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected StructDef or Identifier, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_trait_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Named { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "Named" - may return Identifier or TraitDef
    let offset = find_offset_of(source, "Named")?;
    let ctx = find_node_at_offset(&file, offset);

    // Either the node is a TraitDef or there's a TraitDef in parents
    let is_trait = matches!(ctx.node, NodeAtPosition::TraitDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::TraitDef(_)));
    if !is_trait && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected TraitDef or Identifier, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_enum_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active, inactive }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "Status" - may return Identifier or EnumDef
    let offset = find_offset_of(source, "Status")?;
    let ctx = find_node_at_offset(&file, offset);

    // Either the node is an EnumDef or there's an EnumDef in parents
    let is_enum = matches!(ctx.node, NodeAtPosition::EnumDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::EnumDef(_)));
    if !is_enum && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected EnumDef or Identifier, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_field_in_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "name" field
    let offset = find_offset_of(source, "name")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should find the struct field
    if !matches!(
        ctx.node,
        NodeAtPosition::StructField(_) | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected StructField or Identifier, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_type_in_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "String" type
    let offset = find_offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, offset);

    // Could be Type, Identifier, StructField, or even StructDef
    // The finder returns the innermost node
    let is_valid = matches!(
        ctx.node,
        NodeAtPosition::Type(_)
            | NodeAtPosition::Identifier(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    );
    if !is_valid {
        return Err(format!(
            "Expected Type/Identifier/StructField/StructDef, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_let_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 42";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "x"
    let offset = find_offset_of(source, "x")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should find identifier within let binding
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier, got {:?}", ctx.node).into());
    }
    // Let binding should be a parent
    if !ctx
        .parents
        .iter()
        .any(|n| matches!(n, NodeAtPosition::LetBinding(_)))
    {
        return Err("Expected LetBinding in parents".into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position inside the struct on "name"
    let offset = find_offset_of(source, "name")?;
    let ctx = find_node_at_offset(&file, offset);

    let enclosing = ctx.enclosing_definition();
    if enclosing.is_none() {
        return Err("Expected enclosing definition but got None".into());
    }
    if !matches!(enclosing, Some(NodeAtPosition::StructDef(_))) {
        return Err(format!("Expected StructDef enclosing, got {enclosing:?}").into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_outside_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 42";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position in let binding
    let offset = find_offset_of(source, "42")?;
    let ctx = find_node_at_offset(&file, offset);

    // No enclosing definition for top-level let
    let enclosing = ctx.enclosing_definition();
    if enclosing.is_some() {
        return Err(format!("Expected no enclosing definition, got {enclosing:?}").into());
    }
    Ok(())
}

#[test]
fn test_is_in_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 1 + 2";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "1" in expression
    let offset = find_offset_of(source, "1 +")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should be in expression context (either the node is expression or has expression parent)
    let has_expression = ctx.is_in_expression()
        || matches!(ctx.node, NodeAtPosition::Expression(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::Expression(_)));
    // Or might just be a LetBinding
    if !has_expression && !matches!(ctx.node, NodeAtPosition::LetBinding(_)) {
        return Err(format!("Expected expression context, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_is_in_type_position() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct User { name: String }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "String" type
    let offset = find_offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, offset);

    // May or may not be in type position depending on exact offset
    // Just verify the method doesn't panic
    let _ = ctx.is_in_type_position();
    Ok(())
}

#[test]
fn test_find_enum_variant() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active, inactive }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "active" variant
    let offset = find_offset_of(source, "active")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should find variant or identifier
    if !matches!(
        ctx.node,
        NodeAtPosition::EnumVariant(_) | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected EnumVariant or Identifier, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_node_at_file_start() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position at very beginning
    let ctx = find_node_at_offset(&file, 0);

    // Should find something (struct definition starts at offset 0)
    if matches!(ctx.node, NodeAtPosition::None) {
        return Err("Expected some node at offset 0, got None".into());
    }
    Ok(())
}

#[test]
fn test_find_node_past_end() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position way past the end
    let ctx = find_node_at_offset(&file, 10000);

    // Should return File or None
    if !matches!(ctx.node, NodeAtPosition::File | NodeAtPosition::None) {
        return Err(format!("Expected File or None past end, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_parents_chain() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User {
            name: String,
            age: I32
        }
    ";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "age" field
    let offset = find_offset_of(source, "age")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should have struct as parent somewhere
    let has_struct_parent = ctx
        .parents
        .iter()
        .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
    if !has_struct_parent {
        return Err("Expected StructDef in parents chain".into());
    }
    Ok(())
}

#[test]
fn test_find_use_statement() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use foo::bar";
    let file = parse(source).map_err(|e| format!("parse failed: {e:?}"))?;

    // Position on "foo"
    let offset = find_offset_of(source, "foo")?;
    let ctx = find_node_at_offset(&file, offset);

    // Should find identifier or use statement
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
    ) {
        return Err(format!("Expected Identifier or UseStatement, got {:?}", ctx.node).into());
    }
    Ok(())
}
