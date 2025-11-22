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
        }
    }

    /// Visit a let binding
    fn visit_let_binding(&mut self, let_binding: &'ast LetBinding) {
        // Check name
        if span_contains_offset(&let_binding.name.span, self.offset) {
            self.found_node = Some(NodeAtPosition::Identifier(&let_binding.name));
            return;
        }

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
