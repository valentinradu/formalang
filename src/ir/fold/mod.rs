//! Constant folding pass for IR optimization.
//!
//! This module evaluates constant expressions at compile time:
//! - Arithmetic: `1 + 2` → `3`
//! - Boolean: `true && false` → `false`
//! - Comparison: `1 < 2` → `true`
//!
//! # Example
//!
//! ```formalang
//! struct Config {
//!     scale: f32
//! }
//! impl Config {
//!     scale: 2.0 * 3.0  // Folded to 6.0
//! }
//! ```

mod ops;

use crate::ast::{BinaryOperator, Literal, UnaryOperator};
use crate::ir::{IrExpr, IrModule, ResolvedType};

/// Constant folder that evaluates compile-time constant expressions.
///
/// # Folding contract
///
/// - Folds only when both operands of a binary op are concrete
///   `IrExpr::Literal` values; let-binding values are NOT propagated
///   (a `let x = 1` followed by `x + 1` stays a `BinaryOp`).
/// - Division and modulo by zero are **left unfoldable** by design.
///   Backends decide whether to emit `IEEE 754` infinity / `NaN`, trap, or
///   reject, so the IR keeps the `BinaryOp` and exposes the literal
///   operands for the backend to inspect.
/// - Folding never crosses an effectful boundary (function call,
///   method call, field access on a non-literal receiver).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ConstantFolder;

impl ConstantFolder {
    /// Create a new constant folder.
    ///
    /// previously held a `_module: &IrModule` field that was
    /// never read. The folder is fully stateless; the constructor takes
    /// no arguments now.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Fold constants in an expression, returning a potentially simplified expression.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match over all IrExpr variants"
    )]
    pub fn fold_expr(&self, expr: IrExpr) -> IrExpr {
        match expr {
            IrExpr::BinaryOp {
                left,
                op,
                right,
                ty,
            } => self.fold_binary_op_expr(*left, op, *right, ty),
            IrExpr::UnaryOp { op, operand, ty } => self.fold_unary_op_expr(op, *operand, ty),
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => self.fold_if_expr(*condition, *then_branch, else_branch, ty),
            IrExpr::Array { elements, ty } => IrExpr::Array {
                elements: elements.into_iter().map(|e| self.fold_expr(e)).collect(),
                ty,
            },
            IrExpr::Tuple { fields, ty } => IrExpr::Tuple {
                fields: fields
                    .into_iter()
                    .map(|(n, e)| (n, self.fold_expr(e)))
                    .collect(),
                ty,
            },
            IrExpr::StructInst {
                struct_id,
                type_args,
                fields,
                ty,
            } => IrExpr::StructInst {
                struct_id,
                type_args,
                fields: fields
                    .into_iter()
                    .map(|(n, idx, e)| (n, idx, self.fold_expr(e)))
                    .collect(),
                ty,
            },
            IrExpr::FunctionCall { path, args, ty } => IrExpr::FunctionCall {
                path,
                args: args
                    .into_iter()
                    .map(|(name, expr)| (name, self.fold_expr(expr)))
                    .collect(),
                ty,
            },
            IrExpr::MethodCall {
                receiver,
                method,
                method_idx,
                args,
                dispatch,
                ty,
            } => IrExpr::MethodCall {
                receiver: Box::new(self.fold_expr(*receiver)),
                method,
                method_idx,
                args: args
                    .into_iter()
                    .map(|(name, expr)| (name, self.fold_expr(expr)))
                    .collect(),
                dispatch,
                ty,
            },
            IrExpr::Literal { .. }
            | IrExpr::Reference { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::LetRef { .. } => expr,
            IrExpr::FieldAccess {
                object,
                field,
                field_idx,
                ty,
            } => IrExpr::FieldAccess {
                object: Box::new(self.fold_expr(*object)),
                field,
                field_idx,
                ty,
            },
            IrExpr::For {
                var,
                var_ty,
                collection,
                body,
                ty,
            } => IrExpr::For {
                var,
                var_ty,
                collection: Box::new(self.fold_expr(*collection)),
                body: Box::new(self.fold_expr(*body)),
                ty,
            },
            IrExpr::Match {
                scrutinee,
                arms,
                ty,
            } => IrExpr::Match {
                scrutinee: Box::new(self.fold_expr(*scrutinee)),
                arms: arms
                    .into_iter()
                    .map(|arm| crate::ir::IrMatchArm {
                        variant: arm.variant,
                        variant_idx: arm.variant_idx,
                        is_wildcard: arm.is_wildcard,
                        bindings: arm.bindings,
                        body: self.fold_expr(arm.body),
                    })
                    .collect(),
                ty,
            },
            IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields,
                ty,
            } => IrExpr::EnumInst {
                enum_id,
                variant,
                variant_idx,
                fields: fields
                    .into_iter()
                    .map(|(n, idx, e)| (n, idx, self.fold_expr(e)))
                    .collect(),
                ty,
            },
            IrExpr::DictLiteral { entries, ty } => IrExpr::DictLiteral {
                entries: entries
                    .into_iter()
                    .map(|(k, v)| (self.fold_expr(k), self.fold_expr(v)))
                    .collect(),
                ty,
            },
            IrExpr::DictAccess { dict, key, ty } => IrExpr::DictAccess {
                dict: Box::new(self.fold_expr(*dict)),
                key: Box::new(self.fold_expr(*key)),
                ty,
            },
            IrExpr::Block {
                statements,
                result,
                ty,
            } => IrExpr::Block {
                statements: statements
                    .into_iter()
                    .map(|stmt| stmt.map_exprs(|e| self.fold_expr(e)))
                    .collect(),
                result: Box::new(self.fold_expr(*result)),
                ty,
            },
            IrExpr::Closure {
                params,
                captures,
                body,
                ty,
            } => IrExpr::Closure {
                params,
                captures,
                body: Box::new(self.fold_expr(*body)),
                ty,
            },
            IrExpr::ClosureRef {
                funcref,
                env_struct,
                ty,
            } => IrExpr::ClosureRef {
                funcref,
                env_struct: Box::new(self.fold_expr(*env_struct)),
                ty,
            },
        }
    }

    /// Fold a binary operation: recursively fold children, then try constant folding.
    fn fold_binary_op_expr(
        &self,
        left: IrExpr,
        op: BinaryOperator,
        right: IrExpr,
        ty: ResolvedType,
    ) -> IrExpr {
        let left_folded = self.fold_expr(left);
        let right_folded = self.fold_expr(right);
        if let (
            IrExpr::Literal {
                value: left_val, ..
            },
            IrExpr::Literal {
                value: right_val, ..
            },
        ) = (&left_folded, &right_folded)
        {
            if let Some(result) = ops::fold_binary_op(left_val, op, right_val, &ty) {
                return result;
            }
        }
        IrExpr::BinaryOp {
            left: Box::new(left_folded),
            op,
            right: Box::new(right_folded),
            ty,
        }
    }

    /// Fold a unary operation: recursively fold the operand, then try constant folding.
    fn fold_unary_op_expr(&self, op: UnaryOperator, operand: IrExpr, ty: ResolvedType) -> IrExpr {
        let operand_folded = self.fold_expr(operand);
        if let IrExpr::Literal {
            value: operand_val, ..
        } = &operand_folded
        {
            if let Some(result) = ops::fold_unary_op(op, operand_val, &ty) {
                return result;
            }
        }
        IrExpr::UnaryOp {
            op,
            operand: Box::new(operand_folded),
            ty,
        }
    }

    /// Fold an if expression: eliminate dead branch when condition is a constant boolean.
    fn fold_if_expr(
        &self,
        condition: IrExpr,
        then_branch: IrExpr,
        else_branch: Option<Box<IrExpr>>,
        ty: ResolvedType,
    ) -> IrExpr {
        let cond_folded = self.fold_expr(condition);
        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = &cond_folded
        {
            if *b {
                return self.fold_expr(then_branch);
            } else if let Some(else_branch) = else_branch {
                return self.fold_expr(*else_branch);
            }
        }
        IrExpr::If {
            condition: Box::new(cond_folded),
            then_branch: Box::new(self.fold_expr(then_branch)),
            else_branch: else_branch.map(|e| Box::new(self.fold_expr(*e))),
            ty,
        }
    }
}

/// Fold constants in an entire IR module.
///
/// This creates a new module with constant expressions folded.
#[must_use]
pub fn fold_constants(module: &IrModule) -> IrModule {
    let folder = ConstantFolder::new();
    let mut result = module.clone();

    // Fold constants in impl block expressions
    for impl_block in &mut result.impls {
        for func in &mut impl_block.functions {
            func.body = func.body.take().map(|body| folder.fold_expr(body));
        }
    }

    // Fold constants in standalone functions
    for func in &mut result.functions {
        func.body = func.body.take().map(|body| folder.fold_expr(body));
    }

    // Fold constants in let bindings
    for let_binding in &mut result.lets {
        let_binding.value = folder.fold_expr(let_binding.value.clone());
    }

    // Fold constants in struct field defaults
    for struct_def in &mut result.structs {
        for field in &mut struct_def.fields {
            if let Some(default) = &mut field.default {
                *default = folder.fold_expr(default.clone());
            }
        }
    }

    result
}

/// An [`IrPass`] that evaluates constant expressions at compile time.
///
/// Wraps [`fold_constants`] for use in a [`Pipeline`].
///
/// [`IrPass`]: crate::pipeline::IrPass
/// [`Pipeline`]: crate::pipeline::Pipeline
#[derive(Debug)]
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
pub struct ConstantFoldingPass;

impl ConstantFoldingPass {
    /// Create a new constant folding pass.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ConstantFoldingPass {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::pipeline::IrPass for ConstantFoldingPass {
    fn name(&self) -> &'static str {
        "constant-folding"
    }

    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<crate::error::CompilerError>> {
        Ok(fold_constants(&module))
    }
}

#[cfg(test)]
mod tests;
