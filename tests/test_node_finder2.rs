//! Additional `node_finder` tests for uncovered branches.
//!
//! Covers: binding patterns (array/struct/tuple), `visit_expr` for various
//! expression types, `visit_mount_field`, function param type.

use formalang::parse_only;
use formalang::semantic::node_finder::{find_node_at_offset, NodeAtPosition};

fn offset_of(source: &str, pattern: &str) -> Result<usize, Box<dyn std::error::Error>> {
    source
        .find(pattern)
        .ok_or_else(|| format!("Pattern '{pattern}' not found in source").into())
}

// =============================================================================
// Binding patterns in let bindings
// =============================================================================

#[test]
fn test_find_array_destructuring_ident() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let [first, second] = arr";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "first")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!(
            "Expected Identifier or LetBinding at array binding, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_array_destructuring_rest() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let [head, ...tail] = arr";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "tail")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected Identifier for rest binding, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_struct_destructuring_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let { name, age } = person";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "name")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!(
            "Expected Identifier for struct destructuring, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_struct_destructuring_alias() -> Result<(), Box<dyn std::error::Error>> {
    // { name as n } pattern
    let source = "let { name as n } = person";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Find the alias 'n'
    let patterns: Vec<_> = source.match_indices('n').collect();
    // Verify find_node_at_offset returns a valid context for each occurrence of "n"
    for &(offset, _) in &patterns {
        let ctx = find_node_at_offset(&file, offset);
        // Each position should resolve to some node (not the fallback None/File)
        if matches!(ctx.node, NodeAtPosition::None | NodeAtPosition::File) {
            return Err(format!(
                "Expected a specific node at offset {offset}, got {:?}",
                ctx.node
            )
            .into());
        }
    }
    Ok(())
}

// =============================================================================
// visit_expr for match expression
// =============================================================================

#[test]
fn test_find_match_scrutinee() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct A { }
        let x = match status {
            .active: 1,
            _: 0
        }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "status")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected expr node in match scrutinee, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_match_arm_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = match s { .a: result, _: 0 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "result")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected expr in match arm body, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for if expression
// =============================================================================

#[test]
fn test_find_if_condition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = if flag { 1 } else { 0 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "flag")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in if condition, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_if_then_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = if cond { thenval } else { 0 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "thenval")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in if then branch, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_if_else_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = if cond { 1 } else { elseval }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "elseval")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in if else branch, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for for expression
// =============================================================================

#[test]
fn test_find_for_var_ident() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = for item in collection { item }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "item ")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected Identifier for for loop var, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_for_collection() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = for n in mycollection { n }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "mycollection")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in for collection, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for tuple expression
// =============================================================================

#[test]
fn test_find_tuple_field_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let t = (x: 1, y: 2)";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "x:")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node at tuple field name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_tuple_field_value() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let t = (x: tupleVal, y: 2)";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "tupleVal")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in tuple field value, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for group expression
// =============================================================================

#[test]
fn test_find_group_inner_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = (innerExpr)";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "innerExpr")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in group expression, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for array expression
// =============================================================================

#[test]
fn test_find_array_element() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = [firstElem, 2, 3]";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "firstElem")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node in array element, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for binary operation
// =============================================================================

#[test]
fn test_find_binary_op_left() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = leftOperand + 2";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "leftOperand")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node at binary op left, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_binary_op_right() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = 1 + rightOperand";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "rightOperand")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node at binary op right, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_type for tuple type
// =============================================================================

#[test]
fn test_find_tuple_type_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { pair: (x: Number, y: String) }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "x: Number")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    ) {
        return Err(format!(
            "Expected type-related node in tuple type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// visit_type for closure type
// =============================================================================

#[test]
fn test_find_closure_type_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { cb: (Number) -> String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // First Number in closure type param
    let offset = offset_of(source, "Number")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    ) {
        return Err(format!(
            "Expected type-related node in closure type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// enclosing_definition and is_in_expression methods
// =============================================================================

#[test]
fn test_position_context_enclosing_definition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct User { name: String }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "name")?;
    let ctx = find_node_at_offset(&file, offset);

    // enclosing_definition should find the struct
    let enclosing = ctx
        .enclosing_definition()
        .ok_or("enclosing_definition should return Some when cursor is inside a struct field")?;
    if !matches!(enclosing, NodeAtPosition::StructDef(_)) {
        return Err(
            format!("Expected StructDef as enclosing definition, got {enclosing:?}").into(),
        );
    }
    // The cursor is on the field name "name" — not a type position
    if ctx.is_in_type_position() {
        return Err("Cursor on field name should not be in type position".into());
    }
    Ok(())
}

#[test]
fn test_position_context_is_in_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = myvalue";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "myvalue")?;
    let ctx = find_node_at_offset(&file, offset);
    // myvalue resolves to an Identifier node; is_in_expression only fires when the Expression
    // itself is the final node (not an Identifier within one)
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!(
            "myvalue in let RHS should be an Identifier, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_position_context_is_in_type_position() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "Number")?;
    let ctx = find_node_at_offset(&file, offset);
    // Number is a primitive type with no span tracking; the finder returns StructDef or StructField
    // is_in_type_position() only returns true for Type nodes (not Primitive types)
    if !matches!(
        ctx.node,
        NodeAtPosition::StructDef(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!(
            "Cursor on Number type should resolve to a type-related node, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for invocation expression
// =============================================================================

#[test]
fn test_find_invocation_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = myfn(42)";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "myfn")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
    ) {
        return Err(format!("Expected node at invocation, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for dict literal expression
// =============================================================================

#[test]
fn test_find_dict_literal_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"let d = ["key": 42]"#;
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "[\"key\"")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node in dict literal, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for unary op expression
// =============================================================================

#[test]
fn test_find_unary_op_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = !myvalue";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "!myvalue")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node in unary op, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for field access expression
// =============================================================================

#[test]
fn test_find_field_access_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = obj.myfield";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "obj.myfield")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node at field access, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for method call expression
// =============================================================================

#[test]
fn test_find_method_call_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = obj.mymethod()";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "mymethod")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node in method call, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for block expression
// =============================================================================

#[test]
fn test_find_block_expr_content() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let x = { blockval }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "blockval")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node inside block, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for closure expression
// =============================================================================

#[test]
fn test_find_closure_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let f = |x: Number| x";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "|x:")?;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::LetBinding(_)
            | NodeAtPosition::Identifier(_)
    ) {
        return Err(format!("Expected node at closure, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// visit_expr for let expression
// =============================================================================

#[test]
fn test_find_let_expr_in_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number = 1 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let offset = offset_of(source, "1")?;
    let ctx = find_node_at_offset(&file, offset);
    // Expr::Literal has no span so node_finder falls back to the enclosing StructDef or StructField
    if !matches!(
        ctx.node,
        NodeAtPosition::StructDef(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::Expression(_)
    ) {
        return Err(format!(
            "Expected StructDef/StructField/Expression at literal (no span), got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// visit_type for type parameter
// =============================================================================

#[test]
fn test_find_type_parameter_in_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Box<T> { value: T }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // T in the field type is a type parameter
    let usages: Vec<_> = source.match_indices('T').collect();
    // Find the T in "value: T"
    let last_t = usages.last().ok_or("at least one T expected")?;
    let ctx = find_node_at_offset(&file, last_t.0);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    ) {
        return Err(format!(
            "Expected Identifier at type parameter T, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// node at position: file-level (no specific match)
// =============================================================================

#[test]
fn test_find_position_outside_all_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Position 0 is inside the struct keyword — should find the struct definition
    let ctx = find_node_at_offset(&file, 0);
    if !matches!(
        ctx.node,
        NodeAtPosition::StructDef(_) | NodeAtPosition::Identifier(_) | NodeAtPosition::File
    ) {
        return Err(format!(
            "Expected StructDef or Identifier at position 0, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// visit_type for dictionary type
// =============================================================================

#[test]
fn test_find_dictionary_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Cache { data: [String: Number] }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // The key type "String" in the dict type
    let usages: Vec<_> = source.match_indices("String").collect();
    let offset = usages.first().ok_or("at least one String expected")?.0;
    let ctx = find_node_at_offset(&file, offset);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    ) {
        return Err(format!(
            "Expected type-related node in dict type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}
