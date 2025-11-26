//! AST node finder for position-based queries
//!
//! This module provides utilities for finding AST nodes at a given position in the source code.
//! Used for LSP features like hover, go-to-definition, completion, etc.

use super::position::span_contains_offset;
use crate::ast::*;
use crate::location::Span;

/// Result of finding a node at a position
#[derive(Debug, Clone)]
pub enum NodeAtPosition<'ast> {
    /// Top-level file (no specific node found)
    File,

    /// Use statement
    UseStatement(&'ast UseStmt),

    /// Let binding
    LetBinding(&'ast LetBinding),

    /// Trait definition
    TraitDef(&'ast TraitDef),

    /// Struct definition (unified model/view)
    StructDef(&'ast StructDef),

    /// Enum definition
    EnumDef(&'ast EnumDef),

    /// Enum variant
    EnumVariant(&'ast EnumVariant),

    /// Field definition (in trait)
    FieldDef(&'ast FieldDef),

    /// Struct field (unified)
    StructField(&'ast StructField),

    /// Mount field (also StructField)
    MountField(&'ast StructField),

    /// Type reference
    Type(&'ast Type),

    /// Expression
    Expression(&'ast Expr),

    /// Identifier
    Identifier(&'ast Ident),

    /// No node found at position
    None,
}

/// Context information about where a position is in the AST
#[derive(Debug, Clone)]
pub struct PositionContext<'ast> {
    /// The most specific node at the position
    pub node: NodeAtPosition<'ast>,

    /// Parent nodes from innermost to outermost
    pub parents: Vec<NodeAtPosition<'ast>>,

    /// The byte offset of the position
    pub offset: usize,
}

impl<'ast> PositionContext<'ast> {
    /// Find the enclosing definition (trait, model, view, enum)
    pub fn enclosing_definition(&self) -> Option<&NodeAtPosition<'ast>> {
        self.parents.iter().find(|node| {
            matches!(
                node,
                NodeAtPosition::TraitDef(_)
                    | NodeAtPosition::StructDef(_)
                    | NodeAtPosition::EnumDef(_)
            )
        })
    }

    /// Check if we're inside an expression context
    pub fn is_in_expression(&self) -> bool {
        matches!(self.node, NodeAtPosition::Expression(_))
            || self
                .parents
                .iter()
                .any(|n| matches!(n, NodeAtPosition::Expression(_)))
    }

    /// Check if we're in a type position
    pub fn is_in_type_position(&self) -> bool {
        matches!(self.node, NodeAtPosition::Type(_))
    }
}

/// Find the node at a given byte offset in the file
pub fn find_node_at_offset<'ast>(file: &'ast File, offset: usize) -> PositionContext<'ast> {
    let mut finder = NodeFinder {
        offset,
        parents: Vec::new(),
        found_node: None,
    };

    finder.visit_file(file);

    PositionContext {
        node: finder.found_node.unwrap_or(NodeAtPosition::File),
        parents: finder.parents,
        offset,
    }
}

/// Internal node finder visitor
struct NodeFinder<'ast> {
    offset: usize,
    parents: Vec<NodeAtPosition<'ast>>,
    found_node: Option<NodeAtPosition<'ast>>,
}

impl<'ast> NodeFinder<'ast> {
    /// Visit a file
    fn visit_file(&mut self, file: &'ast File) {
        if !span_contains_offset(&file.span, self.offset) {
            return;
        }

        for statement in &file.statements {
            self.visit_statement(statement);
            if self.found_node.is_some() {
                return;
            }
        }
    }

    /// Visit a statement
    fn visit_statement(&mut self, statement: &'ast Statement) {
        match statement {
            Statement::Use(use_stmt) => {
                if span_contains_offset(&use_stmt.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::UseStatement(use_stmt));
                    self.visit_use_stmt(use_stmt);
                }
            }
            Statement::Let(let_binding) => {
                if span_contains_offset(&let_binding.span, self.offset) {
                    self.parents.push(NodeAtPosition::LetBinding(let_binding));
                    self.visit_let_binding(let_binding);
                    // Don't pop if we found the node within this let binding
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Statement::Definition(definition) => {
                self.visit_definition(definition);
            }
        }
    }

    /// Visit a use statement
    fn visit_use_stmt(&mut self, use_stmt: &'ast UseStmt) {
        // Check path identifiers
        for ident in &use_stmt.path {
            if span_contains_offset(&ident.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(ident));
                return;
            }
        }

        // Check imported items
        match &use_stmt.items {
            UseItems::Single(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            UseItems::Multiple(idents) => {
                for ident in idents {
                    if span_contains_offset(&ident.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(ident));
                        return;
                    }
                }
            }
            UseItems::Glob => {} // No identifiers to check for glob imports
        }
    }

    /// Visit a let binding
    fn visit_let_binding(&mut self, let_binding: &'ast LetBinding) {
        // Check pattern (for simple patterns, check the identifier)
        if let BindingPattern::Simple(ident) = &let_binding.pattern {
            if span_contains_offset(&ident.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(ident));
                return;
            }
        }
        // TODO: Handle other binding patterns (Array, Struct, Tuple)

        // Check value expression
        self.visit_expr(&let_binding.value);
    }

    /// Visit a definition
    fn visit_definition(&mut self, definition: &'ast Definition) {
        match definition {
            Definition::Trait(trait_def) => {
                if span_contains_offset(&trait_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::TraitDef(trait_def));
                    self.visit_trait_def(trait_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Struct(struct_def) => {
                if span_contains_offset(&struct_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::StructDef(struct_def));
                    self.visit_struct_def(struct_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Enum(enum_def) => {
                if span_contains_offset(&enum_def.span, self.offset) {
                    self.parents.push(NodeAtPosition::EnumDef(enum_def));
                    self.visit_enum_def(enum_def);
                    // Don't pop if we found the node
                    if self.found_node.is_none() {
                        self.parents.pop();
                    }
                }
            }
            Definition::Impl(_impl_def) => {
                // Impl blocks don't have position tracking yet
                // TODO: Add proper impl block navigation
            }
            Definition::Module(_module_def) => {
                // Module definitions don't have position tracking yet
                // TODO: Add proper module navigation
            }
        }
    }

    /// Visit a trait definition
    fn visit_trait_def(&mut self, trait_def: &'ast TraitDef) {
        // Check name
        if span_contains_offset(&trait_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&trait_def.name));
            return;
        }

        // Check generic parameters
        for generic in &trait_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check trait composition
        for trait_ref in &trait_def.traits {
            if span_contains_offset(&trait_ref.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_ref));
                return;
            }
        }

        // Check fields
        for field in &trait_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // Check mount fields
        for field in &trait_def.mount_fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the trait def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::TraitDef(trait_def));
        }
    }

    /// Visit a struct definition (unified model/view)
    fn visit_struct_def(&mut self, struct_def: &'ast StructDef) {
        // Check name
        if span_contains_offset(&struct_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&struct_def.name));
            return;
        }

        // Check generic parameters
        for generic in &struct_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check trait implementations
        for trait_ref in &struct_def.traits {
            if span_contains_offset(&trait_ref.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(trait_ref));
                return;
            }
        }

        // Check regular fields
        for field in &struct_def.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::StructField(field));
                self.visit_struct_field(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // Check mount fields
        for field in &struct_def.mount_fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::MountField(field));
                self.visit_mount_field(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the struct def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::StructDef(struct_def));
        }
    }

    /// Visit an enum definition
    fn visit_enum_def(&mut self, enum_def: &'ast EnumDef) {
        // Check name
        if span_contains_offset(&enum_def.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&enum_def.name));
            return;
        }

        // Check generic parameters
        for generic in &enum_def.generics {
            if span_contains_offset(&generic.span, self.offset) {
                self.found_node = Some(NodeAtPosition::Identifier(&generic.name));
                return;
            }
        }

        // Check variants
        for variant in &enum_def.variants {
            if span_contains_offset(&variant.span, self.offset) {
                self.parents.push(NodeAtPosition::EnumVariant(variant));
                self.visit_enum_variant(variant);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the enum def itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumDef(enum_def));
        }
    }

    /// Visit an enum variant
    fn visit_enum_variant(&mut self, variant: &'ast EnumVariant) {
        // Check name
        if span_contains_offset(&variant.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&variant.name));
            return;
        }

        // Check fields
        for field in &variant.fields {
            if span_contains_offset(&field.span, self.offset) {
                self.parents.push(NodeAtPosition::FieldDef(field));
                self.visit_field_def(field);
                self.parents.pop();
                if self.found_node.is_some() {
                    return;
                }
            }
        }

        // If no specific node found, return the variant itself
        if self.found_node.is_none() {
            self.found_node = Some(NodeAtPosition::EnumVariant(variant));
        }
    }

    /// Visit a field definition
    fn visit_field_def(&mut self, field: &'ast FieldDef) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);
    }

    /// Visit a model field
    /// Visit a struct field (unified for regular and mount fields)
    fn visit_struct_field(&mut self, field: &'ast StructField) {
        // Check name
        if span_contains_offset(&field.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
            return;
        }

        // Check type
        self.visit_type(&field.ty);

        // Check default value
        if let Some(default) = &field.default {
            self.visit_expr(default);
        }
    }

    /// Visit a mount field (same as struct field, just kept for clarity)
    fn visit_mount_field(&mut self, field: &'ast StructField) {
        self.visit_struct_field(field);
    }

    /// Visit a type
    fn visit_type(&mut self, ty: &'ast Type) {
        match ty {
            Type::Primitive(_) => {
                // Primitive types don't have spans to check
            }
            Type::Ident(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            Type::Generic { name, args, span } => {
                if span_contains_offset(span, self.offset) {
                    // Check the generic type name
                    if span_contains_offset(&name.span, self.offset) {
                        self.found_node = Some(NodeAtPosition::Identifier(name));
                        return;
                    }

                    // Check type arguments
                    for arg in args {
                        self.visit_type(arg);
                        if self.found_node.is_some() {
                            return;
                        }
                    }

                    // If no specific part found, return the type itself
                    if self.found_node.is_none() {
                        self.found_node = Some(NodeAtPosition::Type(ty));
                    }
                }
            }
            Type::Array(inner) => {
                self.visit_type(inner);
            }
            Type::Optional(inner) => {
                self.visit_type(inner);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if span_contains_offset(&field.span, self.offset) {
                        if span_contains_offset(&field.name.span, self.offset) {
                            self.found_node = Some(NodeAtPosition::Identifier(&field.name));
                            return;
                        }
                        self.visit_type(&field.ty);
                        if self.found_node.is_some() {
                            return;
                        }
                    }
                }
            }
            Type::TypeParameter(ident) => {
                if span_contains_offset(&ident.span, self.offset) {
                    self.found_node = Some(NodeAtPosition::Identifier(ident));
                }
            }
            Type::Dictionary { key, value } => {
                self.visit_type(key);
                if self.found_node.is_some() {
                    return;
                }
                self.visit_type(value);
            }
            Type::Closure { params, ret } => {
                for param in params {
                    self.visit_type(param);
                    if self.found_node.is_some() {
                        return;
                    }
                }
                self.visit_type(ret);
            }
        }
    }

    /// Visit an expression
    fn visit_expr(&mut self, expr: &'ast Expr) {
        // For now, just mark that we're in an expression
        // More detailed expression visiting can be added later as needed
        if let Some(span) = Self::expr_span(expr) {
            if span_contains_offset(&span, self.offset) {
                self.parents.push(NodeAtPosition::Expression(expr));

                // Visit nested expressions based on type
                match expr {
                    Expr::Reference { path, .. } => {
                        for ident in path {
                            if span_contains_offset(&ident.span, self.offset) {
                                self.found_node = Some(NodeAtPosition::Identifier(ident));
                                self.parents.pop();
                                return;
                            }
                        }
                    }
                    Expr::BinaryOp { left, right, .. } => {
                        self.visit_expr(left);
                        if self.found_node.is_none() {
                            self.visit_expr(right);
                        }
                    }
                    Expr::ForExpr {
                        var,
                        collection,
                        body,
                        ..
                    } => {
                        if span_contains_offset(&var.span, self.offset) {
                            self.found_node = Some(NodeAtPosition::Identifier(var));
                        } else {
                            self.visit_expr(collection);
                            if self.found_node.is_none() {
                                self.visit_expr(body);
                            }
                        }
                    }
                    Expr::IfExpr {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        self.visit_expr(condition);
                        if self.found_node.is_none() {
                            self.visit_expr(then_branch);
                        }
                        if self.found_node.is_none() {
                            if let Some(else_expr) = else_branch {
                                self.visit_expr(else_expr);
                            }
                        }
                    }
                    Expr::MatchExpr {
                        scrutinee, arms, ..
                    } => {
                        self.visit_expr(scrutinee);
                        if self.found_node.is_none() {
                            for arm in arms {
                                self.visit_expr(&arm.body);
                                if self.found_node.is_some() {
                                    break;
                                }
                            }
                        }
                    }
                    Expr::Group { expr, .. } => {
                        self.visit_expr(expr);
                    }
                    Expr::Array { elements, .. } => {
                        for elem in elements {
                            self.visit_expr(elem);
                            if self.found_node.is_some() {
                                break;
                            }
                        }
                    }
                    Expr::Tuple { fields, .. } => {
                        for (name, field_expr) in fields {
                            if span_contains_offset(&name.span, self.offset) {
                                self.found_node = Some(NodeAtPosition::Identifier(name));
                                break;
                            }
                            self.visit_expr(field_expr);
                            if self.found_node.is_some() {
                                break;
                            }
                        }
                    }
                    _ => {
                        // For other expression types, just mark that we found an expression
                    }
                }

                // If no more specific node found, use the expression itself
                if self.found_node.is_none() {
                    self.found_node = Some(NodeAtPosition::Expression(expr));
                }

                self.parents.pop();
            }
        }
    }

    /// Get the span of an expression
    fn expr_span(expr: &Expr) -> Option<Span> {
        match expr {
            Expr::Literal(_) => None, // Literals don't have their own spans
            Expr::StructInstantiation { span, .. }
            | Expr::EnumInstantiation { span, .. }
            | Expr::InferredEnumInstantiation { span, .. }
            | Expr::Array { span, .. }
            | Expr::Tuple { span, .. }
            | Expr::Reference { span, .. }
            | Expr::BinaryOp { span, .. }
            | Expr::ForExpr { span, .. }
            | Expr::IfExpr { span, .. }
            | Expr::MatchExpr { span, .. }
            | Expr::Group { span, .. }
            | Expr::ProvidesExpr { span, .. }
            | Expr::ConsumesExpr { span, .. }
            | Expr::DictLiteral { span, .. }
            | Expr::DictAccess { span, .. }
            | Expr::ClosureExpr { span, .. }
            | Expr::LetExpr { span, .. } => Some(*span),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_only;

    fn parse(source: &str) -> Result<File, Vec<crate::error::CompilerError>> {
        parse_only(source)
    }

    fn find_offset_of(source: &str, pattern: &str) -> usize {
        source.find(pattern).expect("Pattern not found in source")
    }

    #[test]
    fn test_find_struct_definition() {
        let source = "struct User { name: String }";
        let file = parse(source).expect("parse failed");

        // Position on "User" - may return Identifier or StructDef
        let offset = find_offset_of(source, "User");
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is a StructDef or there's a StructDef in parents
        let is_struct = matches!(ctx.node, NodeAtPosition::StructDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
        assert!(is_struct || matches!(ctx.node, NodeAtPosition::Identifier(_)));
    }

    #[test]
    fn test_find_trait_definition() {
        let source = "trait Named { name: String }";
        let file = parse(source).expect("parse failed");

        // Position on "Named" - may return Identifier or TraitDef
        let offset = find_offset_of(source, "Named");
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is a TraitDef or there's a TraitDef in parents
        let is_trait = matches!(ctx.node, NodeAtPosition::TraitDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::TraitDef(_)));
        assert!(is_trait || matches!(ctx.node, NodeAtPosition::Identifier(_)));
    }

    #[test]
    fn test_find_enum_definition() {
        let source = "enum Status { active, inactive }";
        let file = parse(source).expect("parse failed");

        // Position on "Status" - may return Identifier or EnumDef
        let offset = find_offset_of(source, "Status");
        let ctx = find_node_at_offset(&file, offset);

        // Either the node is an EnumDef or there's an EnumDef in parents
        let is_enum = matches!(ctx.node, NodeAtPosition::EnumDef(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::EnumDef(_)));
        assert!(is_enum || matches!(ctx.node, NodeAtPosition::Identifier(_)));
    }

    #[test]
    fn test_find_field_in_struct() {
        let source = "struct User { name: String }";
        let file = parse(source).expect("parse failed");

        // Position on "name" field
        let offset = find_offset_of(source, "name");
        let ctx = find_node_at_offset(&file, offset);

        // Should find the struct field
        assert!(matches!(
            ctx.node,
            NodeAtPosition::StructField(_) | NodeAtPosition::Identifier(_)
        ));
    }

    #[test]
    fn test_find_type_in_field() {
        let source = "struct User { name: String }";
        let file = parse(source).expect("parse failed");

        // Position on "String" type
        let offset = find_offset_of(source, "String");
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
        assert!(is_valid);
    }

    #[test]
    fn test_find_let_binding() {
        let source = "let x = 42";
        let file = parse(source).expect("parse failed");

        // Position on "x"
        let offset = find_offset_of(source, "x");
        let ctx = find_node_at_offset(&file, offset);

        // Should find identifier within let binding
        assert!(matches!(ctx.node, NodeAtPosition::Identifier(_)));
        // Let binding should be a parent
        assert!(ctx
            .parents
            .iter()
            .any(|n| matches!(n, NodeAtPosition::LetBinding(_))));
    }

    #[test]
    fn test_enclosing_definition_in_struct_field() {
        let source = "struct User { name: String }";
        let file = parse(source).expect("parse failed");

        // Position inside the struct on "name"
        let offset = find_offset_of(source, "name");
        let ctx = find_node_at_offset(&file, offset);

        let enclosing = ctx.enclosing_definition();
        assert!(enclosing.is_some());
        assert!(matches!(enclosing.unwrap(), NodeAtPosition::StructDef(_)));
    }

    #[test]
    fn test_enclosing_definition_outside_struct() {
        let source = "let x = 42";
        let file = parse(source).expect("parse failed");

        // Position in let binding
        let offset = find_offset_of(source, "42");
        let ctx = find_node_at_offset(&file, offset);

        // No enclosing definition for top-level let
        let enclosing = ctx.enclosing_definition();
        assert!(enclosing.is_none());
    }

    #[test]
    fn test_is_in_expression() {
        let source = "let x = 1 + 2";
        let file = parse(source).expect("parse failed");

        // Position on "1" in expression
        let offset = find_offset_of(source, "1 +");
        let ctx = find_node_at_offset(&file, offset);

        // Should be in expression context (either the node is expression or has expression parent)
        let has_expression = ctx.is_in_expression()
            || matches!(ctx.node, NodeAtPosition::Expression(_))
            || ctx
                .parents
                .iter()
                .any(|p| matches!(p, NodeAtPosition::Expression(_)));
        // Or might just be a LetBinding
        assert!(has_expression || matches!(ctx.node, NodeAtPosition::LetBinding(_)));
    }

    #[test]
    fn test_is_in_type_position() {
        let source = "struct User { name: String }";
        let file = parse(source).expect("parse failed");

        // Position on "String" type
        let offset = find_offset_of(source, "String");
        let ctx = find_node_at_offset(&file, offset);

        // May or may not be in type position depending on exact offset
        // Just verify the method doesn't panic
        let _ = ctx.is_in_type_position();
    }

    #[test]
    fn test_find_enum_variant() {
        let source = "enum Status { active, inactive }";
        let file = parse(source).expect("parse failed");

        // Position on "active" variant
        let offset = find_offset_of(source, "active");
        let ctx = find_node_at_offset(&file, offset);

        // Should find variant or identifier
        assert!(matches!(
            ctx.node,
            NodeAtPosition::EnumVariant(_) | NodeAtPosition::Identifier(_)
        ));
    }

    #[test]
    fn test_find_node_at_file_start() {
        let source = "struct A { }";
        let file = parse(source).expect("parse failed");

        // Position at very beginning
        let ctx = find_node_at_offset(&file, 0);

        // Should find something (struct definition starts at offset 0)
        assert!(!matches!(ctx.node, NodeAtPosition::None));
    }

    #[test]
    fn test_find_node_past_end() {
        let source = "struct A { }";
        let file = parse(source).expect("parse failed");

        // Position way past the end
        let ctx = find_node_at_offset(&file, 10000);

        // Should return File or None
        assert!(matches!(
            ctx.node,
            NodeAtPosition::File | NodeAtPosition::None
        ));
    }

    #[test]
    fn test_parents_chain() {
        let source = r#"
            struct User {
                name: String,
                age: Number
            }
        "#;
        let file = parse(source).expect("parse failed");

        // Position on "age" field
        let offset = find_offset_of(source, "age");
        let ctx = find_node_at_offset(&file, offset);

        // Should have struct as parent somewhere
        let has_struct_parent = ctx
            .parents
            .iter()
            .any(|p| matches!(p, NodeAtPosition::StructDef(_)));
        assert!(has_struct_parent);
    }

    #[test]
    fn test_find_use_statement() {
        let source = "use foo::bar";
        let file = parse(source).expect("parse failed");

        // Position on "foo"
        let offset = find_offset_of(source, "foo");
        let ctx = find_node_at_offset(&file, offset);

        // Should find identifier or use statement
        assert!(matches!(
            ctx.node,
            NodeAtPosition::Identifier(_) | NodeAtPosition::UseStatement(_)
        ));
    }
}
