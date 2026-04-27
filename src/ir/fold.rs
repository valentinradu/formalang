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

use crate::ast::{BinaryOperator, Literal, PrimitiveType, UnaryOperator};
use crate::ir::{IrExpr, IrModule, ResolvedType};

/// Constant folder that evaluates compile-time constant expressions.
///
/// # Folding contract (audit2 B29)
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
    /// Audit2 B26: previously held a `_module: &IrModule` field that was
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
                    .map(|(n, e)| (n, self.fold_expr(e)))
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
                args,
                dispatch,
                ty,
            } => IrExpr::MethodCall {
                receiver: Box::new(self.fold_expr(*receiver)),
                method,
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
            IrExpr::FieldAccess { object, field, ty } => IrExpr::FieldAccess {
                object: Box::new(self.fold_expr(*object)),
                field,
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
                fields,
                ty,
            } => IrExpr::EnumInst {
                enum_id,
                variant,
                fields: fields
                    .into_iter()
                    .map(|(n, e)| (n, self.fold_expr(e)))
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
            if let Some(result) = Self::fold_binary_op(left_val, op, right_val, &ty) {
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
            if let Some(result) = Self::fold_unary_op(op, operand_val, &ty) {
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

    /// Try to fold a binary operation on two literal values.
    fn fold_binary_op(
        left: &Literal,
        op: BinaryOperator,
        right: &Literal,
        ty: &ResolvedType,
    ) -> Option<IrExpr> {
        match (left, right) {
            // Numeric operations
            (Literal::Number(l), Literal::Number(r)) => {
                // Suffix preservation: arithmetic results carry the left
                // operand's suffix. Mismatched-suffix mixing isn't yet
                // type-checked by semantic — that lands with literal-defaulting
                // and IR concrete-typing in later microcommits.
                let combine = |v: f64| {
                    Literal::Number(crate::ast::NumberLiteral::from_lex(v, l.suffix, l.kind))
                };
                let lv = l.value;
                let rv = r.value;
                let result = match op {
                    BinaryOperator::Add => Some(combine(lv + rv)),
                    BinaryOperator::Sub => Some(combine(lv - rv)),
                    BinaryOperator::Mul => Some(combine(lv * rv)),
                    BinaryOperator::Div if rv != 0.0 => Some(combine(lv / rv)),
                    #[expect(
                        clippy::modulo_arithmetic,
                        reason = "f64 modulo with rv != 0 guard mirrors BinaryOp::Mod runtime semantics"
                    )]
                    BinaryOperator::Mod if rv != 0.0 => Some(combine(lv % rv)),
                    BinaryOperator::Lt => Some(Literal::Boolean(lv < rv)),
                    BinaryOperator::Le => Some(Literal::Boolean(lv <= rv)),
                    BinaryOperator::Gt => Some(Literal::Boolean(lv > rv)),
                    BinaryOperator::Ge => Some(Literal::Boolean(lv >= rv)),
                    // Use IEEE 754 semantics for equality: NaN != NaN and
                    // +0.0 == -0.0, matching `f64::eq` and the ordering ops
                    // above. A bit-level comparison would disagree with the
                    // ordering operators on signed zero.
                    #[expect(
                        clippy::float_cmp,
                        reason = "IEEE 754 equality is intentional for constant folding"
                    )]
                    BinaryOperator::Eq => Some(Literal::Boolean(lv == rv)),
                    #[expect(
                        clippy::float_cmp,
                        reason = "IEEE 754 inequality is intentional for constant folding"
                    )]
                    BinaryOperator::Ne => Some(Literal::Boolean(lv != rv)),
                    BinaryOperator::Div
                    | BinaryOperator::Mod
                    | BinaryOperator::And
                    | BinaryOperator::Or
                    | BinaryOperator::Range => None,
                };

                result.map(|value| {
                    let result_ty = match &value {
                        Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
                        Literal::String(_)
                        | Literal::Number(_)
                        | Literal::Regex { .. }
                        | Literal::Path(_)
                        | Literal::Nil => ty.clone(),
                    };
                    IrExpr::Literal {
                        value,
                        ty: result_ty,
                    }
                })
            }

            // Boolean operations
            (Literal::Boolean(l), Literal::Boolean(r)) => {
                let result = match op {
                    BinaryOperator::And => Some(Literal::Boolean(*l && *r)),
                    BinaryOperator::Or => Some(Literal::Boolean(*l || *r)),
                    BinaryOperator::Eq => Some(Literal::Boolean(l == r)),
                    BinaryOperator::Ne => Some(Literal::Boolean(l != r)),
                    BinaryOperator::Add
                    | BinaryOperator::Sub
                    | BinaryOperator::Mul
                    | BinaryOperator::Div
                    | BinaryOperator::Mod
                    | BinaryOperator::Lt
                    | BinaryOperator::Gt
                    | BinaryOperator::Le
                    | BinaryOperator::Ge
                    | BinaryOperator::Range => None,
                };

                result.map(|value| IrExpr::Literal {
                    value,
                    ty: ResolvedType::Primitive(PrimitiveType::Boolean),
                })
            }

            // String concatenation
            (Literal::String(l), Literal::String(r)) => {
                if op == BinaryOperator::Add {
                    Some(IrExpr::Literal {
                        value: Literal::String(format!("{l}{r}")),
                        ty: ResolvedType::Primitive(PrimitiveType::String),
                    })
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    fn fold_unary_op(op: UnaryOperator, operand: &Literal, ty: &ResolvedType) -> Option<IrExpr> {
        match operand {
            // Numeric negation
            Literal::Number(n) => {
                if op == UnaryOperator::Neg {
                    Some(IrExpr::Literal {
                        value: Literal::Number(crate::ast::NumberLiteral::from_lex(
                            -n.value, n.suffix, n.kind,
                        )),
                        ty: ty.clone(),
                    })
                } else {
                    None
                }
            }
            // Boolean negation
            Literal::Boolean(b) => {
                if op == UnaryOperator::Not {
                    Some(IrExpr::Literal {
                        value: Literal::Boolean(!b),
                        ty: ResolvedType::Primitive(PrimitiveType::Boolean),
                    })
                } else {
                    None
                }
            }
            Literal::String(_) | Literal::Regex { .. } | Literal::Path(_) | Literal::Nil => None,
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
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_fold_numeric_addition() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { scale: I32 = 1 + 2 }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        // Check the default was folded
        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (n.value - 3.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 3, got {}", n.value).into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_numeric_multiplication() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { scale: I32 = 2 * 3 }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (n.value - 6.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 6, got {}", n.value).into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_chained_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { value: I32 = 2 + 3 * 4 }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        // 2 + 3 * 4 = 2 + 12 = 14
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (n.value - 14.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 14, got {}", n.value).into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_boolean_and() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { flag: Boolean = true && false }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            if *b {
                return Err(format!("Expected false, got {b}").into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_boolean_or() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { flag: Boolean = true || false }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            if !*b {
                return Err(format!("Expected true, got {b}").into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_comparison() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { result: Boolean = 1 < 2 }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            if !*b {
                return Err(format!("Expected true, got {b}").into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_if_constant_condition() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { value: I32 = if true { 1 } else { 2 } }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (n.value - 1.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 1, got {}", n.value).into());
            }
        } else {
            return Err(format!("Expected folded literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_no_fold_non_constant() -> Result<(), Box<dyn std::error::Error>> {
        // Use a let binding that references another let binding
        let source = r"
            let x: I32 = 1
            let y: I32 = x + 1
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        // The y references x, which is a variable - the folder may or may not optimize this
        // depending on whether it does constant propagation through let bindings
        let let_binding = folded
            .lets
            .iter()
            .find(|l| l.name == "y")
            .ok_or("expected y let binding")?;
        let expr = &let_binding.value;

        // Accept either BinaryOp (no constant propagation) or Literal (with propagation)
        match expr {
            IrExpr::BinaryOp { .. } | IrExpr::Literal { .. } => {}
            IrExpr::StructInst { .. }
            | IrExpr::EnumInst { .. }
            | IrExpr::Array { .. }
            | IrExpr::Tuple { .. }
            | IrExpr::Reference { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::FieldAccess { .. }
            | IrExpr::LetRef { .. }
            | IrExpr::UnaryOp { .. }
            | IrExpr::If { .. }
            | IrExpr::For { .. }
            | IrExpr::Match { .. }
            | IrExpr::FunctionCall { .. }
            | IrExpr::MethodCall { .. }
            | IrExpr::Closure { .. }
            | IrExpr::ClosureRef { .. }
            | IrExpr::DictLiteral { .. }
            | IrExpr::DictAccess { .. }
            | IrExpr::Block { .. } => {
                return Err(format!("Expected BinaryOp or Literal, got {expr:?}").into())
            }
        }
        Ok(())
    }

    #[test]
    fn test_fold_string_concat() -> Result<(), Box<dyn std::error::Error>> {
        let source = r#"
            struct Config { name: String = "Hello" + " World" }
        "#;
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let folded = fold_constants(&module);

        let struct_def = folded
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        if let IrExpr::Literal {
            value: Literal::String(s),
            ..
        } = expr
        {
            if s != "Hello World" {
                return Err(format!("Expected 'Hello World', got {s:?}").into());
            }
        } else {
            return Err(format!("Expected folded string literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_fold_float_eq_signed_zero() -> Result<(), Box<dyn std::error::Error>> {
        // IEEE 754: +0.0 == -0.0 must fold to `true`.
        let ir_module = IrModule::new();
        let _ = &ir_module;
        let folder = ConstantFolder::new();
        let number_ty = ResolvedType::Primitive(PrimitiveType::I32);
        let expression = IrExpr::BinaryOp {
            left: Box::new(IrExpr::Literal {
                value: Literal::Number(0.0.into()),
                ty: number_ty.clone(),
            }),
            op: BinaryOperator::Eq,
            right: Box::new(IrExpr::Literal {
                value: Literal::Number((-0.0_f64).into()),
                ty: number_ty,
            }),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        };
        let result = folder.fold_expr(expression);
        if let IrExpr::Literal {
            value: Literal::Boolean(true),
            ..
        } = result
        {
            Ok(())
        } else {
            Err(format!("Expected folded `true`, got {result:?}").into())
        }
    }

    #[test]
    fn test_fold_float_eq_nan() -> Result<(), Box<dyn std::error::Error>> {
        // IEEE 754: NaN == NaN must fold to `false`.
        let ir_module = IrModule::new();
        let _ = &ir_module;
        let folder = ConstantFolder::new();
        let number_ty = ResolvedType::Primitive(PrimitiveType::I32);
        let expression = IrExpr::BinaryOp {
            left: Box::new(IrExpr::Literal {
                value: Literal::Number(f64::NAN.into()),
                ty: number_ty.clone(),
            }),
            op: BinaryOperator::Eq,
            right: Box::new(IrExpr::Literal {
                value: Literal::Number(f64::NAN.into()),
                ty: number_ty,
            }),
            ty: ResolvedType::Primitive(PrimitiveType::Boolean),
        };
        let result = folder.fold_expr(expression);
        if let IrExpr::Literal {
            value: Literal::Boolean(false),
            ..
        } = result
        {
            Ok(())
        } else {
            Err(format!("Expected folded `false`, got {result:?}").into())
        }
    }
}
