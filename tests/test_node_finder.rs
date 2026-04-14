//! Integration tests for `node_finder`, queries, `module_resolver`, and location modules

use formalang::semantic::node_finder::{find_node_at_offset, NodeAtPosition};
use formalang::semantic::queries::{CompletionKind, QueryProvider};
use formalang::{compile_with_analyzer, parse_only, Location, Span};
use std::path::PathBuf;

// =============================================================================
// Helper utilities
// =============================================================================

fn offset_of(source: &str, pattern: &str) -> Result<usize, Box<dyn std::error::Error>> {
    source
        .find(pattern)
        .ok_or_else(|| format!("Pattern '{pattern}' not found in source").into())
}

// =============================================================================
// node_finder: let binding with value expressions
// =============================================================================

#[test]
fn test_find_let_binding_value_binary_op() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let result = 1 + 2";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "+")?;
    let ctx = find_node_at_offset(&file, off);
    // Should be in an expression context
    let in_expr = ctx.is_in_expression()
        || matches!(ctx.node, NodeAtPosition::Expression(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::Expression(_)));
    if !in_expr && !matches!(ctx.node, NodeAtPosition::LetBinding(_)) {
        return Err(format!("Expected expression or let binding, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_let_binding_value_reference() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = myvar";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "myvar")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    ) {
        return Err(format!("Expected Identifier or Expression, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_let_binding_pattern_simple() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let abc = 0";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "abc")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier, got {:?}", ctx.node).into());
    }
    let has_let_parent = ctx
        .parents
        .iter()
        .any(|p| matches!(p, NodeAtPosition::LetBinding(_)));
    if !has_let_parent {
        return Err("Let binding should be a parent".into());
    }
    Ok(())
}

// =============================================================================
// node_finder: use statements — glob, multiple, single
// =============================================================================

#[test]
fn test_find_use_statement_glob() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use foo::*";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "foo")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
    ) {
        return Err(format!(
            "Expected Identifier or UseStatement for glob import, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_use_statement_path_segment() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use alpha::beta::gamma";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "alpha")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
    ) {
        return Err(format!("Expected Identifier or UseStatement, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_use_statement_multiple_items() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use foo::{Bar, Baz}";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "Bar")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
    ) {
        return Err(format!("Expected Identifier or UseStatement, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_use_statement_single_item() -> Result<(), Box<dyn std::error::Error>> {
    let source = "use foo::Bar";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "Bar")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
    ) {
        return Err(format!("Expected Identifier or UseStatement, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: trait definitions
// =============================================================================

#[test]
fn test_find_trait_name_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Printable { text: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "Printable")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at trait name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_trait_field_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Printable { text: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "text")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::FieldDef(_)
    ) {
        return Err(format!(
            "Expected Identifier or FieldDef at field name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_trait_field_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Printable { text: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::FieldDef(_)
            | NodeAtPosition::TraitDef(_)
    );
    if !valid {
        return Err(format!("Expected type-related node at String, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_trait_def_itself() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Empty { }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Use offset inside `{}` but past the name so trait itself is returned
    let off = offset_of(source, "}")?;
    let ctx = find_node_at_offset(&file, off);
    // Either TraitDef or something in its parents
    let is_trait_related = matches!(ctx.node, NodeAtPosition::TraitDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::TraitDef(_)));
    if !is_trait_related && !matches!(ctx.node, NodeAtPosition::File) {
        return Err(format!("Expected trait-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_trait_generic_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait Container<T> { item: T }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "T>")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at generic param T, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_trait_composition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Base { x: Number }
        trait Extended: Base { y: Number }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Find "Base" in trait composition (second occurrence)
    let usages: Vec<_> = source.match_indices("Base").collect();
    if usages.len() < 2 {
        return Err("Expected two occurrences of Base".into());
    }
    let off = usages.get(1).ok_or("index out of bounds")?.0;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!(
            "Expected Identifier in trait composition, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// node_finder: struct definitions
// =============================================================================

#[test]
fn test_find_struct_name_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct MyStruct { }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "MyStruct")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at struct name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_struct_field_default_expr() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { count: Number = 42 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "42")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Expression(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
            | NodeAtPosition::Identifier(_)
    );
    if !valid {
        return Err(format!(
            "Expected field-related node at default value, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_struct_generic_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Pair<A, B> { first: A, second: B }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "A,")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at generic param A, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_struct_trait_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        trait Named { name: String }
        struct User: Named { name: String }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("Named").collect();
    if usages.len() < 2 {
        return Err("Expected at least 2 occurrences of Named".into());
    }
    let off = usages.get(1).ok_or("index out of bounds")?.0;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!(
            "Expected Identifier at trait conformance, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_struct_def_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Empty { }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Position at the closing brace — should fall back to StructDef
    let off = offset_of(source, "}")?;
    let ctx = find_node_at_offset(&file, off);
    let is_struct = matches!(ctx.node, NodeAtPosition::StructDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
    if !is_struct && !matches!(ctx.node, NodeAtPosition::File) {
        return Err(format!("Expected StructDef-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: enum definitions
// =============================================================================

#[test]
fn test_find_enum_name_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Color { red, green, blue }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "Color")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at enum name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_enum_variant_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active, inactive }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "inactive")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::EnumVariant(_)
    ) {
        return Err(format!(
            "Expected Identifier or EnumVariant at variant name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_enum_variant_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"enum E { val(msg: String) }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "msg")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::FieldDef(_)
    ) {
        return Err(format!(
            "Expected Identifier or FieldDef at variant field, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_enum_variant_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Status { active }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "active")?;
    let ctx = find_node_at_offset(&file, off);
    // Variant or its name identifier
    let valid = matches!(
        ctx.node,
        NodeAtPosition::EnumVariant(_) | NodeAtPosition::Identifier(_)
    );
    if !valid {
        return Err(format!("Expected variant-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_enum_generic_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum Maybe<T> { some(value: T), none }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "T>")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at generic param, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: impl blocks
// =============================================================================

#[test]
fn test_find_impl_struct_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Val { x: Number = 0 }
        impl Val { fn get() -> Number { self.x } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("Val").collect();
    if usages.len() < 2 {
        return Err("Expected at least 2 occurrences of Val".into());
    }
    let off = usages.get(1).ok_or("index out of bounds")?.0; // The one after `impl`
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!(
            "Expected Identifier at impl struct name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_impl_fn_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Val { x: Number = 0 }
        impl Val { fn compute() -> Number { self.x } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "compute")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at fn name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_impl_fn_param() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Val { x: Number = 0 }
        impl Val { fn add(n: Number) -> Number { self.x + n } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "n:")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::FunctionParam(_)
    ) {
        return Err(format!(
            "Expected Identifier or FunctionParam at param name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_impl_fn_body_expression() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Val { x: Number = 5 }
        impl Val { fn get() -> Number { self.x } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "self.x")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!("Expected expr-related node at body, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_impl_trait_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        trait Greeter { greet: String }
        struct User { greet: String = "hi" }
        impl Greeter for User { }
    "#;
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("Greeter").collect();
    if usages.len() < 2 {
        return Err("Expected at least 2 occurrences of Greeter".into());
    }
    let off = usages.get(1).ok_or("index out of bounds")?.0;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at impl trait name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_impl_def_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Val { x: Number = 0 }
        impl Val { }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Use the closing `}` of the impl block
    let last_brace = source.rfind('}').ok_or("no closing brace")?;
    let ctx = find_node_at_offset(&file, last_brace);
    let is_impl_related = matches!(ctx.node, NodeAtPosition::ImplDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::ImplDef(_)));
    if !is_impl_related && !matches!(ctx.node, NodeAtPosition::File) {
        return Err(format!("Expected ImplDef-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: module definitions
// =============================================================================

#[test]
fn test_find_module_name_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"mod mymod { struct A { } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "mymod")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at module name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_module_nested_struct() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"mod outer { struct Inner { val: String } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "Inner")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!(
            "Expected Identifier at nested struct name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_module_def_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"mod empty { }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let last_brace = source.rfind('}').ok_or("no brace")?;
    let ctx = find_node_at_offset(&file, last_brace);
    let is_mod = matches!(ctx.node, NodeAtPosition::ModuleDef(_))
        || ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::ModuleDef(_)));
    if !is_mod && !matches!(ctx.node, NodeAtPosition::File) {
        return Err(format!("Expected ModuleDef-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: standalone function definitions
// =============================================================================

#[test]
fn test_find_function_def_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"fn myfunction() -> Number { 42 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "myfunction")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier at function name, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_function_def_param_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"fn add(x: Number, y: Number) -> Number { x + y }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "x:")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::FunctionParam(_)
    ) {
        return Err(format!("Expected Identifier or FunctionParam, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_function_def_return_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"fn greet() -> String { "hi" }"#;
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Type(_) | NodeAtPosition::FunctionDef(_)
    );
    if !valid {
        return Err(format!(
            "Expected type-related node at return type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_function_def_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"fn double(n: Number) -> Number { n * 2 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "n *")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::FunctionDef(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in function body, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_function_def_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"fn nothing() -> Number { 0 }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // The `0` literal has no span so expr_span returns None for it
    // Place at start of the function
    let off = offset_of(source, "fn")?;
    let ctx = find_node_at_offset(&file, off);
    // Should find function def or identifier or fall back
    let valid = matches!(
        ctx.node,
        NodeAtPosition::FunctionDef(_) | NodeAtPosition::Identifier(_) | NodeAtPosition::File
    );
    if !valid {
        return Err(format!("Expected FunctionDef-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: type variants
// =============================================================================

#[test]
fn test_find_type_array() -> Result<(), Box<dyn std::error::Error>> {
    // Primitive types have no spans, so the node finder returns StructDef for array of primitives.
    // Use a named type instead to exercise the Array type path with a span.
    let source = r"struct Elem { } struct A { items: [Elem] }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("Elem").collect();
    // Second occurrence is in the array type
    let off = usages.get(1).ok_or("index out of bounds")?.0;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    );
    if !valid {
        return Err(format!("Expected type-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_type_optional() -> Result<(), Box<dyn std::error::Error>> {
    // Use a named type for Optional to exercise type visitor properly
    let source = r"struct Inner { } struct A { opt: Inner? }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("Inner").collect();
    let off = usages.get(1).ok_or("index out of bounds")?.0;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    );
    if !valid {
        return Err(format!("Expected type-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_type_generic() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct Wrap<T> { inner: [T] }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "T]")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Type(_)
    );
    if !valid {
        return Err(format!("Expected type-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_type_dictionary() -> Result<(), Box<dyn std::error::Error>> {
    // Primitive types have no spans so StructDef is returned; just verify it doesn't crash
    let source = r"struct A { map: [String: Number] }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Type(_)
            | NodeAtPosition::StructField(_)
            | NodeAtPosition::StructDef(_)
    );
    if !valid {
        return Err(format!("Expected type-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: expression variants
// =============================================================================

#[test]
fn test_find_expr_for_var() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: [Number] = for item in [1, 2] { item } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "item in")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected Identifier or Expression in for var, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_for_collection() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: [Number] = for i in items { i } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "items")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node for collection, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_if_condition() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number = if flag { 1 } else { 0 } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "flag")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node for if condition, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_if_else_branch() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number = if true { 1 } else { fallback } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "fallback")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in else branch, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_match_scrutinee() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green }
        let col: Color = Color.red
        struct A { x: String = match col { red: "r", green: "g" } }
    "#;
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let usages: Vec<_> = source.match_indices("col").collect();
    // Find col in the match expression (second or third usage)
    let match_pos = source.find("match").unwrap_or(0);
    let off = usages
        .iter()
        .find(|(i, _)| *i > match_pos)
        .map_or(usages.first().ok_or("index out of bounds")?.0, |(i, _)| *i);
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in match scrutinee, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_match_arm_body() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        enum Color { red, green }
        let col: Color = Color.red
        let arm_val = "arm"
        struct A { x: String = match col { red: arm_val, green: "g" } }
    "#;
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Find arm_val in match arm body (last occurrence)
    let usages: Vec<_> = source.match_indices("arm_val").collect();
    let off = usages.last().map(|(i, _)| *i).ok_or("arm_val not found")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in match arm body, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_array_elements() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: [Number] = [alpha, beta] }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "alpha")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in array element, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_tuple_field_name() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { t: (first: Number, second: Number) = (first: 1, second: 2) }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // Find "first" inside the tuple expression (second occurrence)
    let usages: Vec<_> = source.match_indices("first").collect();
    if usages.len() < 2 {
        return Err("Expected at least 2 occurrences of 'first'".into());
    }
    let off = usages.get(1).ok_or("index out of bounds")?.0; // In the expression
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::Expression(_)
            | NodeAtPosition::StructField(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr-related node in tuple field name, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_binary_op_right() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let z = left + right";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "right")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!(
            "Expected expr node on right side of binary op, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_find_expr_group() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number = (inner_expr) }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "inner_expr")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Expression(_)
    );
    if !valid {
        return Err(format!("Expected expr node inside group, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: PositionContext methods
// =============================================================================

#[test]
fn test_enclosing_definition_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = "trait T { x: Number }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "x")?;
    let ctx = find_node_at_offset(&file, off);
    let enc = ctx
        .enclosing_definition()
        .ok_or("Expected enclosing definition for trait field")?;
    if !matches!(enc, NodeAtPosition::TraitDef(_)) {
        return Err(format!("Expected TraitDef enclosing definition, got {enc:?}").into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = "enum E { variant }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "variant")?;
    let ctx = find_node_at_offset(&file, off);
    let enc = ctx
        .enclosing_definition()
        .ok_or("Expected enclosing definition for enum variant")?;
    if !matches!(enc, NodeAtPosition::EnumDef(_)) {
        return Err(format!("Expected EnumDef enclosing definition, got {enc:?}").into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_impl() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct V { x: Number = 0 }
        impl V { fn get() -> Number { self.x } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "get")?;
    let ctx = find_node_at_offset(&file, off);
    let enc = ctx
        .enclosing_definition()
        .ok_or("Expected enclosing definition for fn inside impl")?;
    if !matches!(enc, NodeAtPosition::ImplDef(_)) {
        return Err(format!("Expected ImplDef enclosing definition, got {enc:?}").into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_function_def() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"fn myfn(param: Number) -> Number { param }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "param:")?;
    let ctx = find_node_at_offset(&file, off);
    let enc = ctx
        .enclosing_definition()
        .ok_or("Expected enclosing FunctionDef")?;
    if !matches!(enc, NodeAtPosition::FunctionDef(_)) {
        return Err(format!("Expected FunctionDef enclosing definition, got {enc:?}").into());
    }
    Ok(())
}

#[test]
fn test_enclosing_definition_module_def() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"mod m { struct S { } }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "S")?;
    let ctx = find_node_at_offset(&file, off);
    let enc = ctx
        .enclosing_definition()
        .ok_or("Expected enclosing definition inside module")?;
    // The struct name might be nested inside the module or the struct itself
    let valid_enc = matches!(
        enc,
        NodeAtPosition::ModuleDef(_) | NodeAtPosition::StructDef(_)
    );
    if !valid_enc {
        return Err(format!("Expected ModuleDef or StructDef enclosing, got {enc:?}").into());
    }
    Ok(())
}

#[test]
fn test_is_in_expression_true() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = alpha + beta";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "alpha")?;
    let ctx = find_node_at_offset(&file, off);
    if !ctx.is_in_expression() && !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err("Expected to be in expression context".into());
    }
    Ok(())
}

#[test]
fn test_is_in_expression_false_type_position() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { f: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "String")?;
    let ctx = find_node_at_offset(&file, off);
    // In a type position - is_in_expression should be false
    if ctx.is_in_expression() {
        return Err("String in type position should not be in expression context".into());
    }
    Ok(())
}

#[test]
fn test_is_in_type_position_true() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct Wrap<T> { inner: T }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    // T as a type parameter inside field type - might match Type::TypeParameter
    let off = offset_of(source, "T }")?;
    let ctx = find_node_at_offset(&file, off);
    // T in "inner: T" resolves to an Identifier or Type node
    // Note: is_in_type_position() only checks self.node == Type, not parents
    if !matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Type(_) | NodeAtPosition::StructField(_)
    ) {
        return Err(format!(
            "Expected Identifier/Type/StructField at T in field type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// node_finder: binding patterns
// =============================================================================

#[test]
fn test_find_binding_pattern_array_binding() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let [first, second] = items";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "first")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier in array binding, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_binding_pattern_struct_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let { name, age } = user";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "name")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier in struct binding, got {:?}", ctx.node).into());
    }
    Ok(())
}

#[test]
fn test_find_binding_pattern_tuple() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let (a, b) = pair";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "a,")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier in tuple binding, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: fn_def param without type annotation
// =============================================================================

#[test]
fn test_find_fn_def_param_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct S { x: Number = 0 }
        impl S { fn process(val: String) -> String { val } }
    ";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "String)")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::Type(_) | NodeAtPosition::FunctionParam(_)
    );
    if !valid {
        return Err(format!(
            "Expected type or param node for param type, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// location module tests
// =============================================================================

#[test]
fn test_location_new() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location::new(10, 2, 5);
    if loc.offset != 10 {
        return Err(format!("expected offset 10, got {}", loc.offset).into());
    }
    if loc.line != 2 {
        return Err(format!("expected line 2, got {}", loc.line).into());
    }
    if loc.column != 5 {
        return Err(format!("expected column 5, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_location_start() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location::start();
    if loc.offset != 0 {
        return Err(format!("expected offset 0, got {}", loc.offset).into());
    }
    if loc.line != 1 {
        return Err(format!("expected line 1, got {}", loc.line).into());
    }
    if loc.column != 1 {
        return Err(format!("expected column 1, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_location_default() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location::default();
    if loc.offset != 0 {
        return Err(format!("expected offset 0, got {}", loc.offset).into());
    }
    if loc.line != 1 {
        return Err(format!("expected line 1, got {}", loc.line).into());
    }
    if loc.column != 1 {
        return Err(format!("expected column 1, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_span_new() -> Result<(), Box<dyn std::error::Error>> {
    let start = Location::new(0, 1, 1);
    let end = Location::new(5, 1, 6);
    let span = Span::new(start, end);
    if span.start.offset != 0 {
        return Err(format!("expected start offset 0, got {}", span.start.offset).into());
    }
    if span.end.offset != 5 {
        return Err(format!("expected end offset 5, got {}", span.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_span_single() -> Result<(), Box<dyn std::error::Error>> {
    let loc = Location::new(3, 1, 4);
    let span = Span::single(loc);
    if span.start.offset != 3 {
        return Err(format!("expected start offset 3, got {}", span.start.offset).into());
    }
    if span.end.offset != 3 {
        return Err(format!("expected end offset 3, got {}", span.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_span_merge_first_wins_start() -> Result<(), Box<dyn std::error::Error>> {
    let a = Span::new(Location::new(0, 1, 1), Location::new(5, 1, 6));
    let b = Span::new(Location::new(3, 1, 4), Location::new(10, 1, 11));
    let merged = a.merge(b);
    if merged.start.offset != 0 {
        return Err(format!("expected start offset 0, got {}", merged.start.offset).into());
    }
    if merged.end.offset != 10 {
        return Err(format!("expected end offset 10, got {}", merged.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_span_merge_second_wins_start() -> Result<(), Box<dyn std::error::Error>> {
    let a = Span::new(Location::new(5, 1, 6), Location::new(10, 1, 11));
    let b = Span::new(Location::new(2, 1, 3), Location::new(8, 1, 9));
    let merged = a.merge(b);
    if merged.start.offset != 2 {
        return Err(format!("expected start offset 2, got {}", merged.start.offset).into());
    }
    if merged.end.offset != 10 {
        return Err(format!("expected end offset 10, got {}", merged.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_span_merge_identical() -> Result<(), Box<dyn std::error::Error>> {
    let loc1 = Location::new(0, 1, 1);
    let loc2 = Location::new(5, 1, 6);
    let a = Span::new(loc1, loc2);
    let b = Span::new(loc1, loc2);
    let merged = a.merge(b);
    if merged.start.offset != 0 {
        return Err(format!("expected start offset 0, got {}", merged.start.offset).into());
    }
    if merged.end.offset != 5 {
        return Err(format!("expected end offset 5, got {}", merged.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_span_from_range() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span::from_range(2, 8);
    if span.start.offset != 2 {
        return Err(format!("expected start offset 2, got {}", span.start.offset).into());
    }
    if span.end.offset != 8 {
        return Err(format!("expected end offset 8, got {}", span.end.offset).into());
    }
    // line/column are zeroed when using from_range
    if span.start.line != 0 {
        return Err(format!("expected start line 0, got {}", span.start.line).into());
    }
    if span.start.column != 0 {
        return Err(format!("expected start column 0, got {}", span.start.column).into());
    }
    Ok(())
}

#[test]
fn test_span_from_range_with_source() -> Result<(), Box<dyn std::error::Error>> {
    let source = "hello\nworld";
    let span = Span::from_range_with_source(6, 11, source);
    // 'w' is the first char of second line
    if span.start.offset != 6 {
        return Err(format!("expected start offset 6, got {}", span.start.offset).into());
    }
    if span.start.line != 2 {
        return Err(format!("expected start line 2, got {}", span.start.line).into());
    }
    if span.start.column != 1 {
        return Err(format!("expected start column 1, got {}", span.start.column).into());
    }
    Ok(())
}

#[test]
fn test_span_default() -> Result<(), Box<dyn std::error::Error>> {
    let span = Span::default();
    if span.start.offset != 0 {
        return Err(format!("expected start offset 0, got {}", span.start.offset).into());
    }
    if span.end.offset != 0 {
        return Err(format!("expected end offset 0, got {}", span.end.offset).into());
    }
    Ok(())
}

#[test]
fn test_offset_to_location_start() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::offset_to_location;
    let source = "abc";
    let loc = offset_to_location(0, source);
    if loc.line != 1 {
        return Err(format!("expected line 1, got {}", loc.line).into());
    }
    if loc.column != 1 {
        return Err(format!("expected column 1, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_offset_to_location_second_line() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::offset_to_location;
    let source = "ab\ncd";
    let loc = offset_to_location(3, source);
    if loc.line != 2 {
        return Err(format!("expected line 2, got {}", loc.line).into());
    }
    if loc.column != 1 {
        return Err(format!("expected column 1, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_offset_to_location_mid_first_line() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::offset_to_location;
    let source = "abcdef";
    let loc = offset_to_location(3, source);
    if loc.line != 1 {
        return Err(format!("expected line 1, got {}", loc.line).into());
    }
    if loc.column != 4 {
        return Err(format!("expected column 4, got {}", loc.column).into());
    }
    Ok(())
}

#[test]
fn test_fill_span_positions() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::location::fill_span_positions;
    let source = "hello\nworld";
    // Create span with raw offsets
    let raw = Span::from_range(0, 5);
    let filled = fill_span_positions(raw, source);
    if filled.start.line != 1 {
        return Err(format!("expected start line 1, got {}", filled.start.line).into());
    }
    if filled.end.line != 1 {
        return Err(format!("expected end line 1, got {}", filled.end.line).into());
    }
    if filled.end.column != 6 {
        return Err(format!("expected end column 6, got {}", filled.end.column).into());
    }
    Ok(())
}

// =============================================================================
// module_resolver: FileSystemResolver tests
// =============================================================================

#[test]
fn test_filesystem_resolver_not_found_error() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::module_resolver::{FileSystemResolver, ModuleError, ModuleResolver};
    let resolver = FileSystemResolver::new(PathBuf::from("/nonexistent/path/xyz123"));
    let path = vec!["missing".to_string(), "module".to_string()];
    let result = resolver.resolve(&path, None);
    if result.is_ok() {
        return Err("Expected NotFound error".into());
    }
    match result {
        Err(ModuleError::NotFound { path: p, .. }) => {
            if p != path {
                return Err(format!("expected path {path:?}, got {p:?}").into());
            }
        }
        Err(e) => return Err(format!("Expected NotFound, got {e:?}").into()),
        Ok(_) => return Err("Expected error, got Ok".into()),
    }
    Ok(())
}

#[test]
fn test_filesystem_resolver_single_segment_not_found() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::module_resolver::{FileSystemResolver, ModuleResolver};
    let resolver = FileSystemResolver::new(PathBuf::from("/nonexistent/xyz999"));
    let path = vec!["somemod".to_string()];
    let result = resolver.resolve(&path, None);
    if result.is_ok() {
        return Err("Expected error for missing single-segment module".into());
    }
    Ok(())
}

#[test]
fn test_filesystem_resolver_finds_existing_file() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::module_resolver::{FileSystemResolver, ModuleResolver};
    // We'll create a temp file for this test
    let tmp = std::env::temp_dir().join("fv_test_mod");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Failed creating temp dir: {e}"))?;
    std::fs::write(tmp.join("testmod.fv"), "struct Hello { }")
        .map_err(|e| format!("write failed: {e}"))?;

    let resolver2 = FileSystemResolver::new(tmp.clone());
    let path = vec!["testmod".to_string()];
    let result = resolver2.resolve(&path, None);
    // Clean up
    let _ = std::fs::remove_file(tmp.join("testmod.fv"));
    if result.is_err() {
        return Err(format!(
            "Expected Ok for existing module file, got {:?}",
            result.err()
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_filesystem_resolver_with_current_file() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::module_resolver::{FileSystemResolver, ModuleResolver};
    let resolver = FileSystemResolver::new(PathBuf::from("/nonexistent/path"));
    let current = PathBuf::from("/nonexistent/path/current.fv");
    let path = vec!["missing".to_string()];
    // current_file is ignored by FileSystemResolver but shouldn't panic
    let result = resolver.resolve(&path, Some(&current));
    if result.is_ok() {
        return Err("Expected error when file not found".into());
    }
    Ok(())
}

// =============================================================================
// queries: QueryProvider coverage
// =============================================================================

#[test]
fn test_query_provider_hover_trait_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub trait Container<T> { items: [T] }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("Container")
        .ok_or("Should have hover for Container")?;
    if !h.signature.contains("Container") {
        return Err(format!("Signature should contain 'Container', got: {}", h.signature).into());
    }
    if !h.signature.contains("trait") {
        return Err(format!("Signature should contain 'trait', got: {}", h.signature).into());
    }
    if !h.signature.contains("pub") {
        return Err(format!("Signature should contain 'pub', got: {}", h.signature).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_struct_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub struct Box<T> { value: T }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("Box")
        .ok_or("Should have hover for Box")?;
    if !h.signature.contains("Box") {
        return Err(format!("Signature should contain 'Box', got: {}", h.signature).into());
    }
    if !h.signature.contains("pub") {
        return Err(format!(
            "Pub struct should have 'pub' in signature, got: {}",
            h.signature
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_struct_private() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        struct Internal { x: Number }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("Internal")
        .ok_or("Should have hover for Internal")?;
    if h.signature.contains("pub") {
        return Err(format!("Private struct should not have 'pub', got: {}", h.signature).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_enum_with_generics() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub enum Maybe<T> { some(value: T), none }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("Maybe")
        .ok_or("Should have hover for Maybe")?;
    if !h.signature.contains("Maybe") {
        return Err(format!("Signature should contain 'Maybe', got: {}", h.signature).into());
    }
    if !h.signature.contains("pub") {
        return Err(format!(
            "Pub enum should have 'pub' in signature, got: {}",
            h.signature
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_enum_private() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        enum Color { red, green }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("Color")
        .ok_or("Should have hover for Color")?;
    if h.signature.contains("pub") {
        return Err(format!("Private enum should not have 'pub', got: {}", h.signature).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_let_binding_with_type() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        let count = 42
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("count")
        .ok_or("Should have hover for count")?;
    if !h.signature.contains("count") {
        return Err(format!("Signature should contain 'count', got: {}", h.signature).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_let_binding_pub() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"
        pub let threshold = 100
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let h = provider
        .get_hover_for_symbol("threshold")
        .ok_or("Should have hover for threshold")?;
    if !h.signature.contains("pub") {
        return Err(format!(
            "Pub let should have 'pub' in signature, got: {}",
            h.signature
        )
        .into());
    }
    Ok(())
}

#[test]
fn test_query_provider_hover_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let hover = provider.get_hover_for_symbol("NonExistent");
    if hover.is_some() {
        return Err("Should return None for unknown symbol".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"pub trait Serializable { data: String }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let d = provider
        .find_definition_by_name("Serializable")
        .ok_or("Should find definition for Serializable")?;
    if d.symbol_name != "Serializable" {
        return Err(format!("expected 'Serializable', got '{}'", d.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"enum Direction { north, south, east, west }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let d = provider
        .find_definition_by_name("Direction")
        .ok_or("Should find definition for Direction")?;
    if d.symbol_name != "Direction" {
        return Err(format!("expected 'Direction', got '{}'", d.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_let() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"let myval = "hello""#;
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let d = provider
        .find_definition_by_name("myval")
        .ok_or("Should find definition for myval")?;
    if d.symbol_name != "myval" {
        return Err(format!("expected 'myval', got '{}'", d.symbol_name).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_find_definition_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct A { x: Number }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let def = provider.find_definition_by_name("DoesNotExist");
    if def.is_some() {
        return Err("Should return None for unknown symbol".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completion_view_trait() -> Result<(), Box<dyn std::error::Error>> {
    // A trait with mount fields is a ViewTrait
    let source = r"
        trait Displayable {
            @text: String
        }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();
    let candidate = completions
        .iter()
        .find(|c| c.label == "Displayable")
        .ok_or("Expected Displayable in completions")?;
    if !matches!(
        candidate.kind,
        CompletionKind::ViewTrait | CompletionKind::ModelTrait
    ) {
        return Err("Expected ViewTrait or ModelTrait kind".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completion_view_struct() -> Result<(), Box<dyn std::error::Error>> {
    // A struct with mount fields is a View
    let source = r"
        struct Button {
            @label: String
        }
    ";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_all_completions();
    let candidate = completions
        .iter()
        .find(|c| c.label == "Button")
        .ok_or("Expected Button in completions")?;
    if !matches!(candidate.kind, CompletionKind::View | CompletionKind::Model) {
        return Err("Expected View or Model kind".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_type_completions_includes_enum() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"enum Color { red, green }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_type_completions();
    let has_color = completions.iter().any(|c| c.label == "Color");
    if !has_color {
        return Err("Expected Color in type completions".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_type_completions_includes_trait() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"trait Sortable { key: Number }";
    let (_, analyzer) =
        compile_with_analyzer(source).map_err(|e| format!("compile failed: {e:?}"))?;
    let provider = QueryProvider::new(analyzer.symbols());
    let completions = provider.get_type_completions();
    let has_sortable = completions.iter().any(|c| c.label == "Sortable");
    if !has_sortable {
        return Err("Expected Sortable in type completions".into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completion_with_detail() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::queries::CompletionCandidate;
    let c = CompletionCandidate::new("foo", CompletionKind::Keyword).with_detail("Some detail");
    if c.label != "foo" {
        return Err(format!("expected label 'foo', got '{}'", c.label).into());
    }
    if c.detail.as_deref() != Some("Some detail") {
        return Err(format!("expected detail 'Some detail', got {:?}", c.detail).into());
    }
    Ok(())
}

#[test]
fn test_query_provider_completion_candidate_new() -> Result<(), Box<dyn std::error::Error>> {
    use formalang::semantic::queries::CompletionCandidate;
    let c = CompletionCandidate::new("bar", CompletionKind::Field);
    if c.label != "bar" {
        return Err(format!("expected label 'bar', got '{}'", c.label).into());
    }
    if c.detail.is_some() {
        return Err("expected no detail".into());
    }
    if c.insert_text.is_some() {
        return Err("expected no insert_text".into());
    }
    if c.documentation.is_some() {
        return Err("expected no documentation".into());
    }
    Ok(())
}

// =============================================================================
// node_finder: struct mount fields
// =============================================================================

#[test]
fn test_find_struct_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"struct View { @label: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "label")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_)
            | NodeAtPosition::MountField(_)
            | NodeAtPosition::StructField(_)
    );
    if !valid {
        return Err(format!("Expected mount field-related node, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// node_finder: enclosing_definition returns None at top-level
// =============================================================================

#[test]
fn test_enclosing_definition_none_at_top_level() -> Result<(), Box<dyn std::error::Error>> {
    let source = "let x = 0";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "x")?;
    let ctx = find_node_at_offset(&file, off);
    if ctx.enclosing_definition().is_some() {
        return Err("Top-level let should have no enclosing definition".into());
    }
    Ok(())
}

// =============================================================================
// node_finder: position context offset is preserved
// =============================================================================

#[test]
fn test_position_context_offset_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let source = "struct A { x: Number }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let target = offset_of(source, "Number")?;
    let ctx = find_node_at_offset(&file, target);
    if ctx.offset != target {
        return Err(format!(
            "Offset must be preserved in context: expected {target}, got {}",
            ctx.offset
        )
        .into());
    }
    Ok(())
}

// =============================================================================
// node_finder: array pattern rest element
// =============================================================================

#[test]
fn test_find_array_pattern_rest_named() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"let [head, ...tail] = items";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "tail")?;
    let ctx = find_node_at_offset(&file, off);
    if !matches!(ctx.node, NodeAtPosition::Identifier(_)) {
        return Err(format!("Expected Identifier for rest element, got {:?}", ctx.node).into());
    }
    Ok(())
}

// =============================================================================
// Additional: trait with mount fields (for complete visit_trait_def coverage)
// =============================================================================

#[test]
fn test_find_trait_mount_field() -> Result<(), Box<dyn std::error::Error>> {
    let source = r"trait T { @child: String }";
    let file = parse_only(source).map_err(|e| format!("parse failed: {e:?}"))?;
    let off = offset_of(source, "child")?;
    let ctx = find_node_at_offset(&file, off);
    let valid = matches!(
        ctx.node,
        NodeAtPosition::Identifier(_) | NodeAtPosition::FieldDef(_)
    );
    if !valid {
        return Err(format!(
            "Expected Identifier or FieldDef in trait mount field, got {:?}",
            ctx.node
        )
        .into());
    }
    Ok(())
}
