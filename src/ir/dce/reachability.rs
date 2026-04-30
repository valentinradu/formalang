//! Reachability walkers used by the DCE pass.
//!
//! The walkers traverse [`crate::ir::ResolvedType`] and [`IrExpr`] values,
//! marking any [`StructId`], [`TraitId`], or [`EnumId`] they encounter as
//! live in the parent [`super::DeadCodeEliminator`].

use crate::ir::IrExpr;

use super::DeadCodeEliminator;

impl DeadCodeEliminator<'_> {
    /// Mark structs used in a type.
    pub(super) fn mark_used_in_type(&mut self, ty: &crate::ir::ResolvedType) {
        use crate::ir::ResolvedType;

        match ty {
            ResolvedType::Struct(id) => {
                self.used_structs.insert(*id);
            }
            ResolvedType::Trait(id) => {
                self.used_traits.insert(*id);
            }
            ResolvedType::Generic { base, args } => {
                match base {
                    crate::ir::GenericBase::Struct(id) => {
                        self.used_structs.insert(*id);
                    }
                    crate::ir::GenericBase::Enum(id) => {
                        self.used_enums.insert(*id);
                    }
                    crate::ir::GenericBase::Trait(id) => {
                        self.used_traits.insert(*id);
                    }
                }
                for arg in args {
                    self.mark_used_in_type(arg);
                }
            }
            ResolvedType::Array(inner)
            | ResolvedType::Range(inner)
            | ResolvedType::Optional(inner) => {
                self.mark_used_in_type(inner);
            }
            ResolvedType::Tuple(fields) => {
                for (_, field_ty) in fields {
                    self.mark_used_in_type(field_ty);
                }
            }
            ResolvedType::Dictionary { key_ty, value_ty } => {
                self.mark_used_in_type(key_ty);
                self.mark_used_in_type(value_ty);
            }
            ResolvedType::Closure {
                param_tys,
                return_ty,
            } => {
                for (_, pty) in param_tys {
                    self.mark_used_in_type(pty);
                }
                self.mark_used_in_type(return_ty);
            }
            ResolvedType::External { type_args, .. } => {
                for arg in type_args {
                    self.mark_used_in_type(arg);
                }
            }
            ResolvedType::Enum(id) => {
                self.used_enums.insert(*id);
            }
            // Placeholder types do not reference any definition.
            ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
        }
    }

    /// Mark structs used in an expression.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match over every IrExpr variant; splitting would hide the walk"
    )]
    pub(super) fn mark_used_in_expr(&mut self, expr: &IrExpr) {
        match expr {
            IrExpr::StructInst {
                struct_id,
                fields,
                ty,
                type_args,
                ..
            } => {
                if let Some(id) = struct_id {
                    self.used_structs.insert(*id);
                }
                // walk the resolved type and any explicit
                // type-arguments so a local struct/enum used as a generic
                // arg of an external receiver (e.g. `Box<LocalThing>` from
                // an imported module) is marked used.
                self.mark_used_in_type(ty);
                for arg in type_args {
                    self.mark_used_in_type(arg);
                }
                for (_, _, e) in fields {
                    self.mark_used_in_expr(e);
                }
            }
            IrExpr::BinaryOp { left, right, .. } => {
                self.mark_used_in_expr(left);
                self.mark_used_in_expr(right);
            }
            IrExpr::UnaryOp { operand, .. } => self.mark_used_in_expr(operand),
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.mark_used_in_expr(condition);
                self.mark_used_in_expr(then_branch);
                if let Some(else_b) = else_branch {
                    self.mark_used_in_expr(else_b);
                }
            }
            IrExpr::Array { elements, .. } => {
                for e in elements {
                    self.mark_used_in_expr(e);
                }
            }
            IrExpr::EnumInst {
                enum_id,
                fields,
                ty,
                ..
            } => {
                if let Some(id) = enum_id {
                    self.used_enums.insert(*id);
                }
                // walk the resolved type so a local
                // struct/enum used as a generic arg of an external
                // enum (e.g. `Result<LocalErr, OK>`) is marked used.
                self.mark_used_in_type(ty);
                for (_, _, e) in fields {
                    self.mark_used_in_expr(e);
                }
            }
            IrExpr::Tuple { fields, .. } => {
                for (_, e) in fields {
                    self.mark_used_in_expr(e);
                }
            }
            IrExpr::For {
                collection, body, ..
            } => {
                self.mark_used_in_expr(collection);
                self.mark_used_in_expr(body);
            }
            IrExpr::Match {
                scrutinee, arms, ..
            } => {
                self.mark_used_in_expr(scrutinee);
                for arm in arms {
                    self.mark_used_in_expr(&arm.body);
                }
            }
            IrExpr::FunctionCall { args, .. } => {
                for (_, arg) in args {
                    self.mark_used_in_expr(arg);
                }
            }
            IrExpr::CallClosure {
                closure, args, ty, ..
            } => {
                self.mark_used_in_type(ty);
                self.mark_used_in_expr(closure);
                for (_, arg) in args {
                    self.mark_used_in_expr(arg);
                }
            }
            IrExpr::MethodCall {
                receiver,
                args,
                dispatch,
                ..
            } => {
                self.mark_used_in_expr(receiver);
                for (_, arg) in args {
                    self.mark_used_in_expr(arg);
                }
                // Virtual dispatch keeps its trait alive.
                if let crate::ir::DispatchKind::Virtual { trait_id, .. } = dispatch {
                    self.used_traits.insert(*trait_id);
                }
                // Static dispatch points at an impl whose target struct/enum
                // is already reached via the receiver's type.
            }
            IrExpr::DictLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.mark_used_in_expr(k);
                    self.mark_used_in_expr(v);
                }
            }
            IrExpr::DictAccess { dict, key, .. } => {
                self.mark_used_in_expr(dict);
                self.mark_used_in_expr(key);
            }
            IrExpr::Block {
                statements, result, ..
            } => {
                for stmt in statements {
                    self.mark_used_in_block_statement(stmt);
                }
                self.mark_used_in_expr(result);
            }
            IrExpr::Literal { .. }
            | IrExpr::Reference { .. }
            | IrExpr::SelfFieldRef { .. }
            | IrExpr::LetRef { .. } => {}
            IrExpr::FieldAccess { object, .. } => self.mark_used_in_expr(object),
            IrExpr::Closure {
                params,
                captures,
                body,
                ..
            } => {
                for (_, _, _, ty) in params {
                    self.mark_used_in_type(ty);
                }
                for (_, _, _, ty) in captures {
                    self.mark_used_in_type(ty);
                }
                self.mark_used_in_expr(body);
            }
            IrExpr::ClosureRef { env_struct, ty, .. } => {
                self.mark_used_in_type(ty);
                self.mark_used_in_expr(env_struct);
            }
        }
    }

    pub(super) fn mark_used_in_block_statement(&mut self, stmt: &crate::ir::IrBlockStatement) {
        use crate::ir::IrBlockStatement;
        match stmt {
            IrBlockStatement::Let { value, .. } => {
                self.mark_used_in_expr(value);
            }
            IrBlockStatement::Assign { target, value } => {
                self.mark_used_in_expr(target);
                self.mark_used_in_expr(value);
            }
            IrBlockStatement::Expr(expr) => {
                self.mark_used_in_expr(expr);
            }
        }
    }
}
