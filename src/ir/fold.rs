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

use crate::ast::{BinaryOperator, Literal, PrimitiveType};
use crate::ir::{IrExpr, IrModule, ResolvedType};

/// Constant folder that evaluates compile-time constant expressions.
pub struct ConstantFolder<'a> {
    _module: &'a IrModule,
}

impl<'a> ConstantFolder<'a> {
    /// Create a new constant folder.
    pub fn new(module: &'a IrModule) -> Self {
        Self { _module: module }
    }

    /// Fold constants in an expression, returning a potentially simplified expression.
    pub fn fold_expr(&self, expr: IrExpr) -> IrExpr {
        match expr {
            IrExpr::BinaryOp {
                left,
                op,
                right,
                ty,
            } => {
                // First recursively fold children
                let left_folded = self.fold_expr(*left);
                let right_folded = self.fold_expr(*right);

                // Try to fold if both sides are literals
                if let (
                    IrExpr::Literal {
                        value: left_val, ..
                    },
                    IrExpr::Literal {
                        value: right_val, ..
                    },
                ) = (&left_folded, &right_folded)
                {
                    if let Some(result) = self.fold_binary_op(left_val, op, right_val, &ty) {
                        return result;
                    }
                }

                // Can't fold, return with folded children
                IrExpr::BinaryOp {
                    left: Box::new(left_folded),
                    op,
                    right: Box::new(right_folded),
                    ty,
                }
            }

            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
            } => {
                let cond_folded = self.fold_expr(*condition);

                // If condition is a constant boolean, return the appropriate branch
                if let IrExpr::Literal {
                    value: Literal::Boolean(b),
                    ..
                } = &cond_folded
                {
                    if *b {
                        return self.fold_expr(*then_branch);
                    } else if let Some(else_branch) = else_branch {
                        return self.fold_expr(*else_branch);
                    }
                }

                // Can't fold, return with folded children
                IrExpr::If {
                    condition: Box::new(cond_folded),
                    then_branch: Box::new(self.fold_expr(*then_branch)),
                    else_branch: else_branch.map(|e| Box::new(self.fold_expr(*e))),
                    ty,
                }
            }

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
                mounts,
                ty,
            } => IrExpr::StructInst {
                struct_id,
                type_args,
                fields: fields
                    .into_iter()
                    .map(|(n, e)| (n, self.fold_expr(e)))
                    .collect(),
                mounts: mounts
                    .into_iter()
                    .map(|(n, e)| (n, self.fold_expr(e)))
                    .collect(),
                ty,
            },

            IrExpr::FunctionCall { path, args, ty } => IrExpr::FunctionCall {
                path,
                args: args.into_iter().map(|a| self.fold_expr(a)).collect(),
                ty,
            },

            IrExpr::MethodCall {
                receiver,
                method,
                args,
                ty,
            } => IrExpr::MethodCall {
                receiver: Box::new(self.fold_expr(*receiver)),
                method,
                args: args.into_iter().map(|a| self.fold_expr(a)).collect(),
                ty,
            },

            // Expressions that can't be folded further
            IrExpr::Literal { .. }
            | IrExpr::Reference { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::LetRef { .. } => expr,

            // Expressions with nested folding needed
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

            IrExpr::EventMapping { .. } => expr, // Event mappings are metadata, don't fold

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
        }
    }

    /// Try to fold a binary operation on two literal values.
    fn fold_binary_op(
        &self,
        left: &Literal,
        op: BinaryOperator,
        right: &Literal,
        ty: &ResolvedType,
    ) -> Option<IrExpr> {
        match (left, right) {
            // Numeric operations
            (Literal::Number(l), Literal::Number(r)) => {
                let result = match op {
                    BinaryOperator::Add => Some(Literal::Number(l + r)),
                    BinaryOperator::Sub => Some(Literal::Number(l - r)),
                    BinaryOperator::Mul => Some(Literal::Number(l * r)),
                    BinaryOperator::Div if *r != 0.0 => Some(Literal::Number(l / r)),
                    BinaryOperator::Mod if *r != 0.0 => Some(Literal::Number(l % r)),
                    BinaryOperator::Lt => Some(Literal::Boolean(l < r)),
                    BinaryOperator::Le => Some(Literal::Boolean(l <= r)),
                    BinaryOperator::Gt => Some(Literal::Boolean(l > r)),
                    BinaryOperator::Ge => Some(Literal::Boolean(l >= r)),
                    BinaryOperator::Eq => Some(Literal::Boolean((l - r).abs() < f64::EPSILON)),
                    BinaryOperator::Ne => Some(Literal::Boolean((l - r).abs() >= f64::EPSILON)),
                    _ => None,
                };

                result.map(|value| {
                    let result_ty = match &value {
                        Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
                        _ => ty.clone(),
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
                    _ => None,
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
                        value: Literal::String(format!("{}{}", l, r)),
                        ty: ResolvedType::Primitive(PrimitiveType::String),
                    })
                } else {
                    None
                }
            }

            _ => None,
        }
    }
}

/// Fold constants in an entire IR module.
///
/// This creates a new module with constant expressions folded.
pub fn fold_constants(module: &IrModule) -> IrModule {
    let folder = ConstantFolder::new(module);
    let mut result = module.clone();

    // Fold constants in impl block expressions
    for impl_block in &mut result.impls {
        for (_, expr) in &mut impl_block.defaults {
            *expr = folder.fold_expr(expr.clone());
        }
        for func in &mut impl_block.functions {
            func.body = folder.fold_expr(func.body.clone());
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_fold_numeric_addition() {
        let source = r#"
            struct Config { scale: Number }
            impl Config { scale: 1 + 2 }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        // Check the default was folded
        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            assert!((n - 3.0).abs() < f64::EPSILON, "Expected 3, got {}", n);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_numeric_multiplication() {
        let source = r#"
            struct Config { scale: Number }
            impl Config { scale: 2 * 3 }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            assert!((n - 6.0).abs() < f64::EPSILON, "Expected 6, got {}", n);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_chained_arithmetic() {
        let source = r#"
            struct Config { value: Number }
            impl Config { value: 2 + 3 * 4 }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        // 2 + 3 * 4 = 2 + 12 = 14
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            assert!((n - 14.0).abs() < f64::EPSILON, "Expected 14, got {}", n);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_boolean_and() {
        let source = r#"
            struct Config { flag: Boolean }
            impl Config { flag: true && false }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            assert!(!b, "Expected false, got {}", b);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_boolean_or() {
        let source = r#"
            struct Config { flag: Boolean }
            impl Config { flag: true || false }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            assert!(*b, "Expected true, got {}", b);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_comparison() {
        let source = r#"
            struct Config { result: Boolean }
            impl Config { result: 1 < 2 }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Boolean(b),
            ..
        } = expr
        {
            assert!(*b, "Expected true, got {}", b);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_fold_if_constant_condition() {
        let source = r#"
            struct Config { value: Number }
            impl Config { value: if true { 1 } else { 2 } }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            assert!((n - 1.0).abs() < f64::EPSILON, "Expected 1, got {}", n);
        } else {
            panic!("Expected folded literal, got {:?}", expr);
        }
    }

    #[test]
    fn test_no_fold_non_constant() {
        let source = r#"
            struct Point { x: Number, y: Number }
            impl Point { x: 1, y: self.x + 1 }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        // The y field references self.x, so it shouldn't be folded
        let impl_block = &folded.impls[0];
        let (name, expr) = &impl_block.defaults[1];
        assert_eq!(name, "y");

        // Should still be a BinaryOp (not folded)
        assert!(
            matches!(expr, IrExpr::BinaryOp { .. }),
            "Expected BinaryOp, got {:?}",
            expr
        );
    }

    #[test]
    fn test_fold_string_concat() {
        let source = r#"
            struct Config { name: String }
            impl Config { name: "Hello" + " World" }
        "#;
        let module = compile_to_ir(source).unwrap();
        let folded = fold_constants(&module);

        let impl_block = &folded.impls[0];
        let (_, expr) = &impl_block.defaults[0];

        if let IrExpr::Literal {
            value: Literal::String(s),
            ..
        } = expr
        {
            assert_eq!(s, "Hello World");
        } else {
            panic!("Expected folded string literal, got {:?}", expr);
        }
    }
}
