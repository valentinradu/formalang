//! Visitor pattern for IR traversal.
//!
//! The visitor pattern allows code generators to process IR nodes without
//! implementing manual traversal logic.
//!
//! # Example
//!
//! ```
//! use formalang::compile_to_ir;
//! use formalang::ir::{IrVisitor, IrStruct, IrEnum, StructId, EnumId, walk_module};
//!
//! struct TypeCounter {
//!     struct_count: usize,
//!     enum_count: usize,
//! }
//!
//! impl IrVisitor for TypeCounter {
//!     fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {
//!         self.struct_count += 1;
//!     }
//!
//!     fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {
//!         self.enum_count += 1;
//!     }
//! }
//!
//! let source = r#"
//! pub struct User { name: String }
//! pub enum Status { active, inactive }
//! "#;
//! let module = compile_to_ir(source).unwrap();
//! let mut counter = TypeCounter { struct_count: 0, enum_count: 0 };
//! walk_module(&mut counter, &module);
//! assert_eq!(counter.struct_count, 1);
//! assert_eq!(counter.enum_count, 1);
//! ```

use super::{
    EnumId, IrEnum, IrEnumVariant, IrExpr, IrField, IrFunction, IrImpl, IrLet, IrModule, IrStruct,
    IrTrait, StructId, TraitId,
};

/// Visitor trait for traversing IR nodes.
///
/// Implement this trait and override the methods for nodes you care about.
/// Default implementations do nothing, so you only need to implement what you need.
///
/// Use [`walk_module`] to traverse an entire module, or [`walk_expr`] to traverse
/// an expression tree.
pub trait IrVisitor {
    /// Visit the entire module. Default implementation walks all children.
    fn visit_module(&mut self, module: &IrModule) {
        walk_module_children(self, module);
    }

    /// Visit a struct definition.
    fn visit_struct(&mut self, _id: StructId, _s: &IrStruct) {}

    /// Visit a trait definition.
    fn visit_trait(&mut self, _id: TraitId, _t: &IrTrait) {}

    /// Visit an enum definition.
    fn visit_enum(&mut self, _id: EnumId, _e: &IrEnum) {}

    /// Visit an enum variant.
    fn visit_enum_variant(&mut self, _v: &IrEnumVariant) {}

    /// Visit an impl block.
    fn visit_impl(&mut self, _i: &IrImpl) {}

    /// Visit a function definition.
    fn visit_function(&mut self, _f: &IrFunction) {}

    /// Visit a let binding.
    fn visit_let(&mut self, _l: &IrLet) {}

    /// Visit a field definition.
    fn visit_field(&mut self, _f: &IrField) {}

    /// Visit an expression. Default implementation walks children.
    fn visit_expr(&mut self, e: &IrExpr) {
        walk_expr_children(self, e);
    }
}

/// Walk an entire IR module, visiting all definitions.
///
/// This calls `visitor.visit_module()` which by default walks all structs,
/// traits, enums, and impls.
pub fn walk_module<V: IrVisitor + ?Sized>(visitor: &mut V, module: &IrModule) {
    visitor.visit_module(module);
}

/// Walk all children of a module.
///
/// This is called by the default `visit_module` implementation.
/// You can call this manually if you override `visit_module` but still
/// want to walk children.
pub fn walk_module_children<V: IrVisitor + ?Sized>(visitor: &mut V, module: &IrModule) {
    // Visit structs
    for (idx, s) in module.structs.iter().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked by add_struct which errors before len reaches u32::MAX"
        )]
        visitor.visit_struct(StructId(idx as u32), s);
        for field in &s.fields {
            visitor.visit_field(field);
            // Walk field default expressions
            if let Some(default) = &field.default {
                walk_expr(visitor, default);
            }
        }
    }

    // Visit traits
    for (idx, t) in module.traits.iter().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked by add_trait which errors before len reaches u32::MAX"
        )]
        visitor.visit_trait(TraitId(idx as u32), t);
        for field in &t.fields {
            visitor.visit_field(field);
        }
    }

    // Visit enums
    for (idx, e) in module.enums.iter().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked by add_enum which errors before len reaches u32::MAX"
        )]
        visitor.visit_enum(EnumId(idx as u32), e);
        for variant in &e.variants {
            visitor.visit_enum_variant(variant);
            for field in &variant.fields {
                visitor.visit_field(field);
            }
        }
    }

    // Visit impls
    for i in &module.impls {
        visitor.visit_impl(i);
        for f in &i.functions {
            visitor.visit_function(f);
            if let Some(body) = &f.body {
                walk_expr(visitor, body);
            }
        }
    }

    // Visit let bindings
    for l in &module.lets {
        visitor.visit_let(l);
        walk_expr(visitor, &l.value);
    }
}

/// Walk an expression tree, visiting all sub-expressions.
pub fn walk_expr<V: IrVisitor + ?Sized>(visitor: &mut V, expr: &IrExpr) {
    visitor.visit_expr(expr);
}

/// Walk all children of an expression.
///
/// This is called by the default `visit_expr` implementation.
pub fn walk_expr_children<V: IrVisitor + ?Sized>(visitor: &mut V, expr: &IrExpr) {
    match expr {
        IrExpr::StructInst { fields, .. }
        | IrExpr::EnumInst { fields, .. }
        | IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                walk_expr(visitor, e);
            }
        }

        IrExpr::Array { elements, .. } => {
            for e in elements {
                walk_expr(visitor, e);
            }
        }

        IrExpr::FieldAccess { object, .. } => {
            walk_expr(visitor, object);
        }

        IrExpr::BinaryOp { left, right, .. } => {
            walk_expr(visitor, left);
            walk_expr(visitor, right);
        }

        IrExpr::UnaryOp { operand, .. } => {
            walk_expr(visitor, operand);
        }

        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expr(visitor, condition);
            walk_expr(visitor, then_branch);
            if let Some(e) = else_branch {
                walk_expr(visitor, e);
            }
        }

        IrExpr::For {
            collection, body, ..
        } => {
            walk_expr(visitor, collection);
            walk_expr(visitor, body);
        }

        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(visitor, scrutinee);
            for arm in arms {
                walk_expr(visitor, &arm.body);
            }
        }

        IrExpr::FunctionCall { args, .. } => {
            for (_, arg) in args {
                walk_expr(visitor, arg);
            }
        }

        IrExpr::MethodCall { receiver, args, .. } => {
            walk_expr(visitor, receiver);
            for (_, arg) in args {
                walk_expr(visitor, arg);
            }
        }

        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(visitor, k);
                walk_expr(visitor, v);
            }
        }

        IrExpr::DictAccess { dict, key, .. } => {
            walk_expr(visitor, dict);
            walk_expr(visitor, key);
        }

        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements {
                walk_block_statement(visitor, stmt);
            }
            walk_expr(visitor, result);
        }

        IrExpr::Closure { body, .. } => {
            walk_expr(visitor, body);
        }

        IrExpr::EventMapping { .. }
        | IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

/// Walk the children of a block statement.
pub fn walk_block_statement<V: IrVisitor + ?Sized>(
    visitor: &mut V,
    stmt: &crate::ir::IrBlockStatement,
) {
    use crate::ir::IrBlockStatement;
    match stmt {
        IrBlockStatement::Let { value, .. } => {
            walk_expr(visitor, value);
        }
        IrBlockStatement::Assign { target, value } => {
            walk_expr(visitor, target);
            walk_expr(visitor, value);
        }
        IrBlockStatement::Expr(expr) => {
            walk_expr(visitor, expr);
        }
    }
}
