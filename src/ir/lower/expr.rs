//! Expression lowering helpers for the IR lowering pass.

use super::IrLowerer;
use crate::ast::{
    self, BinaryOperator, BindingPattern, BlockStatement, ClosureParam, Expr, Literal,
    PrimitiveType, UnaryOperator,
};
use crate::builtins::resolve_method_type;
use crate::ir::{
    EventBindingSource, EventFieldBinding, IrBlockStatement, IrExpr, IrMatchArm, ResolvedType,
};

impl IrLowerer<'_> {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> IrExpr {
        match expr {
            Expr::Literal(lit) => IrExpr::Literal {
                value: lit.clone(),
                ty: Self::literal_type(lit),
            },
            Expr::Invocation { path, type_args, args, mounts, .. } => {
                self.lower_invocation(path, type_args, args, mounts)
            }
            Expr::EnumInstantiation { enum_name, variant, data, .. } => {
                self.lower_enum_instantiation(&enum_name.name, &variant.name, data)
            }
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                self.lower_inferred_enum_instantiation(&variant.name, data)
            }
            Expr::Array { elements, .. } => self.lower_array_expr(elements),
            Expr::Tuple { fields, .. } => self.lower_tuple_expr(fields),
            Expr::Reference { path, .. } => self.lower_reference(path),
            Expr::BinaryOp { left, op, right, .. } => self.lower_binary_op_expr(left, *op, right),
            Expr::UnaryOp { op, operand, .. } => self.lower_unary_op_expr(*op, operand),
            Expr::IfExpr { condition, then_branch, else_branch, .. } => {
                self.lower_if_expr(condition, then_branch, else_branch.as_deref())
            }
            Expr::ForExpr { var, collection, body, .. } => {
                self.lower_for_expr(var, collection, body)
            }
            Expr::MatchExpr { scrutinee, arms, .. } => self.lower_match_expr(scrutinee, arms),
            Expr::Group { expr, .. } => self.lower_expr(expr),
            Expr::LetExpr { body, .. } => self.lower_expr(body),
            Expr::DictLiteral { entries, .. } => self.lower_dict_literal(entries),
            Expr::DictAccess { dict, key, .. } => self.lower_dict_access(dict, key),
            Expr::ClosureExpr { params, body, .. } => self.lower_closure(params, body),
            Expr::FieldAccess { object, field, .. } => {
                let object_ir = self.lower_expr(object);
                let ty = self.resolve_field_type(object_ir.ty(), &field.name);
                IrExpr::FieldAccess { object: Box::new(object_ir), field: field.name.clone(), ty }
            }
            Expr::MethodCall { receiver, method, args, .. } => {
                self.lower_method_call(receiver, &method.name, args)
            }
            Expr::Block { statements, result, .. } => self.lower_block_expr(statements, result),
        }
    }

    fn lower_invocation(
        &mut self,
        path: &[crate::ast::Ident],
        type_args: &[crate::ast::Type],
        args: &[(Option<crate::ast::Ident>, Expr)],
        mounts: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let name = path.iter().map(|id| id.name.as_str()).collect::<Vec<_>>().join("::");
        let type_args_resolved: Vec<ResolvedType> =
            type_args.iter().map(|t| self.lower_type(t)).collect();

        if let Some(id) = self.module.struct_id(&name) {
            let ty = if type_args_resolved.is_empty() {
                ResolvedType::Struct(id)
            } else {
                ResolvedType::Generic { base: id, args: type_args_resolved.clone() }
            };
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt.as_ref().map(|n| (n.name.clone(), self.lower_expr(expr)))
                })
                .collect();
            IrExpr::StructInst {
                struct_id: Some(id),
                type_args: type_args_resolved,
                fields: named_fields,
                mounts: mounts.iter().map(|(n, e)| (n.name.clone(), self.lower_expr(e))).collect(),
                ty,
            }
        } else if let Some(external_ty) = self.try_external_type(&name, type_args_resolved.clone()) {
            let named_fields: Vec<(String, IrExpr)> = args
                .iter()
                .filter_map(|(name_opt, expr)| {
                    name_opt.as_ref().map(|n| (n.name.clone(), self.lower_expr(expr)))
                })
                .collect();
            IrExpr::StructInst {
                struct_id: None,
                type_args: type_args_resolved,
                fields: named_fields,
                mounts: mounts.iter().map(|(n, e)| (n.name.clone(), self.lower_expr(e))).collect(),
                ty: external_ty,
            }
        } else {
            let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();
            let lowered_args: Vec<(Option<String>, IrExpr)> = args
                .iter()
                .map(|(name_opt, expr)| (name_opt.as_ref().map(|n| n.name.clone()), self.lower_expr(expr)))
                .collect();
            let fn_name = path_strs.last().map_or("", std::string::String::as_str);
            let ty = self.resolve_function_return_type(fn_name, &lowered_args);
            IrExpr::FunctionCall { path: path_strs, args: lowered_args, ty }
        }
    }

    fn lower_enum_instantiation(
        &mut self,
        enum_name: &str,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let (enum_id, ty) = self.module.enum_id(enum_name).map_or_else(
            || {
                self.try_external_type(enum_name, vec![]).map_or_else(
                    || (None, ResolvedType::TypeParam(enum_name.to_string())),
                    |external_ty| (None, external_ty),
                )
            },
            |id| (Some(id), ResolvedType::Enum(id)),
        );
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            fields: data.iter().map(|(n, e)| (n.name.clone(), self.lower_expr(e))).collect(),
            ty,
        }
    }

    fn lower_inferred_enum_instantiation(
        &mut self,
        variant: &str,
        data: &[(crate::ast::Ident, Expr)],
    ) -> IrExpr {
        let (enum_id, ty) = self.current_function_return_type.clone().map_or_else(
            || (None, ResolvedType::TypeParam("InferredEnum".to_string())),
            |return_type_name| {
                self.module.enum_id(&return_type_name).map_or_else(
                    || {
                        self.try_external_type(&return_type_name, vec![]).map_or_else(
                            || (None, ResolvedType::TypeParam("InferredEnum".to_string())),
                            |external_ty| (None, external_ty),
                        )
                    },
                    |id| (Some(id), ResolvedType::Enum(id)),
                )
            },
        );
        IrExpr::EnumInst {
            enum_id,
            variant: variant.to_string(),
            fields: data.iter().map(|(n, e)| (n.name.clone(), self.lower_expr(e))).collect(),
            ty,
        }
    }

    fn lower_array_expr(&mut self, elements: &[Expr]) -> IrExpr {
        let lowered: Vec<IrExpr> = elements.iter().map(|e| self.lower_expr(e)).collect();
        let elem_ty = lowered
            .first()
            .map_or_else(|| ResolvedType::TypeParam("UnknownElement".to_string()), |e| e.ty().clone());
        IrExpr::Array { elements: lowered, ty: ResolvedType::Array(Box::new(elem_ty)) }
    }

    fn lower_tuple_expr(&mut self, fields: &[(crate::ast::Ident, Expr)]) -> IrExpr {
        let lowered: Vec<(String, IrExpr)> =
            fields.iter().map(|(n, e)| (n.name.clone(), self.lower_expr(e))).collect();
        let tuple_types: Vec<(String, ResolvedType)> =
            lowered.iter().map(|(n, e)| (n.clone(), e.ty().clone())).collect();
        IrExpr::Tuple { fields: lowered, ty: ResolvedType::Tuple(tuple_types) }
    }

    fn lower_reference(&mut self, path: &[crate::ast::Ident]) -> IrExpr {
        let path_strs: Vec<String> = path.iter().map(|i| i.name.clone()).collect();

        // Check for self.field pattern — bounds verified by len() == 2 check
        #[expect(clippy::indexing_slicing, reason = "len == 2 check above guarantees indices 0 and 1")]
        if path_strs.len() == 2 && path_strs[0] == "self" {
            let field_name = &path_strs[1];
            let ty = self.resolve_self_field_type(field_name);
            return IrExpr::SelfFieldRef { field: field_name.clone(), ty };
        }

        // Check for bare "self" in impl context — bounds verified by len() == 1 check
        #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
        if path_strs.len() == 1 && path_strs[0] == "self" {
            if let Some(ref impl_name) = self.current_impl_struct {
                let ty = self.resolve_impl_self_type(impl_name);
                return IrExpr::Reference { path: path_strs, ty };
            }
        }

        // Check for module-level let binding reference
        if path_strs.len() == 1 {
            #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
            let name = &path_strs[0];
            if let Some(let_type) = self.symbols.get_let_type(name) {
                let ty = self.string_to_resolved_type(let_type);
                return IrExpr::LetRef { name: name.clone(), ty };
            }
        }

        let ty = if path_strs.len() == 1 {
            #[expect(clippy::indexing_slicing, reason = "len == 1 check above guarantees index 0")]
            let t = ResolvedType::TypeParam(path_strs[0].clone());
            t
        } else {
            ResolvedType::TypeParam(path_strs.join("."))
        };
        IrExpr::Reference { path: path_strs, ty }
    }

    fn lower_binary_op_expr(&mut self, left: &Expr, op: BinaryOperator, right: &Expr) -> IrExpr {
        let left_ir = self.lower_expr(left);
        let right_ir = self.lower_expr(right);
        let ty = match op {
            BinaryOperator::Eq
            | BinaryOperator::Ne
            | BinaryOperator::Lt
            | BinaryOperator::Le
            | BinaryOperator::Gt
            | BinaryOperator::Ge
            | BinaryOperator::And
            | BinaryOperator::Or => ResolvedType::Primitive(PrimitiveType::Boolean),
            BinaryOperator::Add
            | BinaryOperator::Sub
            | BinaryOperator::Mul
            | BinaryOperator::Div
            | BinaryOperator::Mod
            | BinaryOperator::Range => left_ir.ty().clone(),
        };
        IrExpr::BinaryOp { left: Box::new(left_ir), op, right: Box::new(right_ir), ty }
    }

    fn lower_unary_op_expr(&mut self, op: UnaryOperator, operand: &Expr) -> IrExpr {
        let operand_ir = self.lower_expr(operand);
        let ty = match op {
            UnaryOperator::Not => ResolvedType::Primitive(PrimitiveType::Boolean),
            UnaryOperator::Neg => operand_ir.ty().clone(),
        };
        IrExpr::UnaryOp { op, operand: Box::new(operand_ir), ty }
    }

    fn lower_if_expr(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
    ) -> IrExpr {
        let then_ir = self.lower_expr(then_branch);
        let ty = then_ir.ty().clone();
        IrExpr::If {
            condition: Box::new(self.lower_expr(condition)),
            then_branch: Box::new(then_ir),
            else_branch: else_branch.map(|e| Box::new(self.lower_expr(e))),
            ty,
        }
    }

    fn lower_for_expr(
        &mut self,
        var: &crate::ast::Ident,
        collection: &Expr,
        body: &Expr,
    ) -> IrExpr {
        let collection_ir = self.lower_expr(collection);
        let body_ir = self.lower_expr(body);
        let var_ty = match collection_ir.ty() {
            ResolvedType::Array(inner) => (**inner).clone(),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::EventMapping { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => ResolvedType::TypeParam("UnknownElement".to_string()),
        };
        IrExpr::For {
            var: var.name.clone(),
            var_ty,
            collection: Box::new(collection_ir),
            body: Box::new(body_ir.clone()),
            ty: ResolvedType::Array(Box::new(body_ir.ty().clone())),
        }
    }

    fn lower_match_expr(&mut self, scrutinee: &Expr, arms: &[crate::ast::MatchArm]) -> IrExpr {
        let scrutinee_ir = self.lower_expr(scrutinee);
        let arms_ir: Vec<IrMatchArm> = arms
            .iter()
            .map(|arm| {
                let bindings = self.extract_pattern_bindings(&arm.pattern, &scrutinee_ir);
                IrMatchArm {
                    variant: match &arm.pattern {
                        ast::Pattern::Variant { name, .. } => name.name.clone(),
                        ast::Pattern::Wildcard => String::new(),
                    },
                    is_wildcard: matches!(&arm.pattern, ast::Pattern::Wildcard),
                    bindings,
                    body: self.lower_expr(&arm.body),
                }
            })
            .collect();
        let ty = arms_ir
            .first()
            .map_or_else(|| ResolvedType::TypeParam("Unknown".to_string()), |a| a.body.ty().clone());
        IrExpr::Match { scrutinee: Box::new(scrutinee_ir), arms: arms_ir, ty }
    }

    fn lower_dict_literal(&mut self, entries: &[(Expr, Expr)]) -> IrExpr {
        let lowered_entries: Vec<(IrExpr, IrExpr)> =
            entries.iter().map(|(k, v)| (self.lower_expr(k), self.lower_expr(v))).collect();
        let ty = if let Some((k, v)) = lowered_entries.first() {
            ResolvedType::Dictionary {
                key_ty: Box::new(k.ty().clone()),
                value_ty: Box::new(v.ty().clone()),
            }
        } else {
            ResolvedType::Dictionary {
                key_ty: Box::new(ResolvedType::TypeParam("K".to_string())),
                value_ty: Box::new(ResolvedType::TypeParam("V".to_string())),
            }
        };
        IrExpr::DictLiteral { entries: lowered_entries, ty }
    }

    fn lower_dict_access(&mut self, dict: &Expr, key: &Expr) -> IrExpr {
        let dict_ir = self.lower_expr(dict);
        let key_ir = self.lower_expr(key);
        let ty = match dict_ir.ty() {
            ResolvedType::Dictionary { value_ty, .. } => (**value_ty).clone(),
            ResolvedType::Primitive(_)
            | ResolvedType::Struct(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::EventMapping { .. }
            | ResolvedType::Closure { .. } => ResolvedType::TypeParam("DictValue".to_string()),
        };
        IrExpr::DictAccess { dict: Box::new(dict_ir), key: Box::new(key_ir), ty }
    }

    fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method_name: &str,
        args: &[Expr],
    ) -> IrExpr {
        let receiver_ir = self.lower_expr(receiver);
        let lowered_args: Vec<(Option<String>, IrExpr)> =
            args.iter().map(|expr| (None, self.lower_expr(expr))).collect();
        let ty = self.resolve_method_return_type(receiver_ir.ty(), method_name);
        IrExpr::MethodCall {
            receiver: Box::new(receiver_ir),
            method: method_name.to_string(),
            args: lowered_args,
            ty,
        }
    }

    fn lower_block_expr(&mut self, statements: &[BlockStatement], result: &Expr) -> IrExpr {
        let ir_statements: Vec<IrBlockStatement> =
            statements.iter().map(|stmt| self.lower_block_statement(stmt)).collect();
        let ir_result = self.lower_expr(result);
        let ty = ir_result.ty().clone();
        if ir_statements.is_empty() {
            return ir_result;
        }
        IrExpr::Block { statements: ir_statements, result: Box::new(ir_result), ty }
    }

    /// Lower an AST block statement to an IR block statement.
    pub(super) fn lower_block_statement(&mut self, stmt: &BlockStatement) -> IrBlockStatement {
        match stmt {
            BlockStatement::Let {
                mutable,
                pattern,
                ty,
                value,
                ..
            } => {
                // Handle binding patterns
                let name = match pattern {
                    BindingPattern::Simple(ident) => ident.name.clone(),
                    BindingPattern::Tuple { elements, .. } => {
                        // For tuple destructuring, extract first simple name or use placeholder
                        elements
                            .iter()
                            .find_map(|p| match p {
                                BindingPattern::Simple(ident) => Some(ident.name.clone()),
                                BindingPattern::Array { .. }
                                | BindingPattern::Struct { .. }
                                | BindingPattern::Tuple { .. } => None,
                            })
                            .unwrap_or_else(|| "_tuple".to_string())
                    }
                    BindingPattern::Struct { fields, .. } => {
                        // For struct destructuring, use first field name or placeholder
                        fields
                            .first().map_or_else(|| "_struct".to_string(), |f| f.name.name.clone())
                    }
                    BindingPattern::Array { elements, .. } => {
                        // For array destructuring, use first binding name or placeholder
                        elements
                            .iter()
                            .find_map(|elem| match elem {
                                crate::ast::ArrayPatternElement::Binding(
                                    BindingPattern::Simple(ident),
                                ) => Some(ident.name.clone()),
                                crate::ast::ArrayPatternElement::Binding(_)
                                | crate::ast::ArrayPatternElement::Rest(_)
                                | crate::ast::ArrayPatternElement::Wildcard => None,
                            })
                            .unwrap_or_else(|| "_array".to_string())
                    }
                };
                let ir_ty = ty.as_ref().map(|t| self.lower_type(t));
                let ir_value = self.lower_expr(value);

                IrBlockStatement::Let {
                    name,
                    mutable: *mutable,
                    ty: ir_ty,
                    value: ir_value,
                }
            }
            BlockStatement::Assign { target, value, .. } => {
                let ir_target = self.lower_expr(target);
                let ir_value = self.lower_expr(value);

                IrBlockStatement::Assign {
                    target: ir_target,
                    value: ir_value,
                }
            }
            BlockStatement::Expr(expr) => {
                let ir_expr = self.lower_expr(expr);
                IrBlockStatement::Expr(ir_expr)
            }
        }
    }

    pub(super) fn literal_type(lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(_) => ResolvedType::Primitive(PrimitiveType::Number),
            Literal::UnsignedInt(_) => ResolvedType::Primitive(PrimitiveType::U32),
            Literal::SignedInt(_) => ResolvedType::Primitive(PrimitiveType::I32),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            Literal::Nil => ResolvedType::TypeParam("Nil".to_string()),
        }
    }

    /// Resolve the type of a field access on an expression.
    ///
    /// Handles:
    /// 1. Vector component access (vec2.x, vec3.y, etc.) -> f32/i32/u32
    /// 2. Struct field access -> field type
    pub(super) fn resolve_field_type(&self, object_ty: &ResolvedType, field_name: &str) -> ResolvedType {
        match object_ty {
            // Vector component access
            ResolvedType::Primitive(PrimitiveType::Vec2 | PrimitiveType::Vec3 |
PrimitiveType::Vec4) => match field_name {
                "x" | "y" | "z" | "w" | "r" | "g" | "b" | "a" => {
                    ResolvedType::Primitive(PrimitiveType::F32)
                }
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            ResolvedType::Primitive(PrimitiveType::IVec2 | PrimitiveType::IVec3 |
PrimitiveType::IVec4) => match field_name {
                "x" | "y" | "z" | "w" => ResolvedType::Primitive(PrimitiveType::I32),
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            ResolvedType::Primitive(PrimitiveType::UVec2 | PrimitiveType::UVec3 |
PrimitiveType::UVec4) => match field_name {
                "x" | "y" | "z" | "w" => ResolvedType::Primitive(PrimitiveType::U32),
                _ => ResolvedType::TypeParam(field_name.to_string()),
            },
            // Struct field access
            ResolvedType::Struct(struct_id) => {
                if let Some(struct_def) = self.module.get_struct(*struct_id) {
                    for field in &struct_def.fields {
                        if field.name == field_name {
                            return field.ty.clone();
                        }
                    }
                }
                ResolvedType::TypeParam(field_name.to_string())
            }
            // Default: return a placeholder type
            ResolvedType::Primitive(_)
            | ResolvedType::Trait(_)
            | ResolvedType::Enum(_)
            | ResolvedType::Array(_)
            | ResolvedType::Optional(_)
            | ResolvedType::Tuple(_)
            | ResolvedType::Generic { .. }
            | ResolvedType::TypeParam(_)
            | ResolvedType::External { .. }
            | ResolvedType::EventMapping { .. }
            | ResolvedType::Dictionary { .. }
            | ResolvedType::Closure { .. } => ResolvedType::TypeParam(field_name.to_string()),
        }
    }

    /// Resolve the return type of a method call.
    ///
    /// Handles:
    /// 1. Builtin methods on GPU types (e.g., `vec3.normalize()` -> Vec3)
    /// 2. User-defined methods in impl blocks
    pub(super) fn resolve_method_return_type(
        &self,
        receiver_ty: &ResolvedType,
        method_name: &str,
    ) -> ResolvedType {
        // Try builtin method resolution for primitive types
        if let ResolvedType::Primitive(prim) = receiver_ty {
            if let Some(return_prim) = resolve_method_type(*prim, method_name) {
                return ResolvedType::Primitive(return_prim);
            }
        }

        // Try to find method in impl blocks for struct types
        if let ResolvedType::Struct(struct_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.struct_id() == Some(*struct_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            // Return the function's return type, or the body type if unspecified
                            return func
                                .return_type
                                .clone()
                                .unwrap_or_else(|| func.body.ty().clone());
                        }
                    }
                }
            }
        }

        // Try to find method in impl blocks for enum types
        if let ResolvedType::Enum(enum_id) = receiver_ty {
            for impl_block in &self.module.impls {
                if impl_block.enum_id() == Some(*enum_id) {
                    for func in &impl_block.functions {
                        if func.name == method_name {
                            return func
                                .return_type
                                .clone()
                                .unwrap_or_else(|| func.body.ty().clone());
                        }
                    }
                }
            }
        }

        // Fallback: placeholder type
        ResolvedType::TypeParam(format!("{method_name}Result"))
    }

    /// Resolve the return type of a function call.
    ///
    /// Handles:
    /// 1. User-defined standalone functions in `IrModule::functions`
    /// 2. Builtin functions (math, WGSL intrinsics, etc.)
    /// 3. Falls back to void for unknown functions
    pub(super) fn resolve_function_return_type(
        &self,
        fn_name: &str,
        _args: &[(Option<String>, IrExpr)],
    ) -> ResolvedType {
        // Check if it's a user-defined function
        if let Some(func_id) = self.module.function_id(fn_name) {
            if let Some(func) = self.module.get_function(func_id) {
                // Return the declared return type, or infer from body
                return func
                    .return_type
                    .clone()
                    .unwrap_or_else(|| func.body.ty().clone());
            }
        }

        // Check builtin functions registry
        if let Some(return_ty) = Self::resolve_builtin_function_type(fn_name) {
            return return_ty;
        }

        // Fallback: void type for unknown functions
        ResolvedType::Primitive(PrimitiveType::Never)
    }

    /// Resolve the return type of a builtin function.
    ///
    /// Returns the appropriate type for common builtin/intrinsic functions.
    fn resolve_builtin_function_type(fn_name: &str) -> Option<ResolvedType> {
        use PrimitiveType::{Number, Vec2, Vec3, Vec4, IVec2, IVec3, IVec4, UVec2, UVec3, UVec4, Mat2, Mat3, Mat4, I32, U32, Boolean};

        // Math functions (return same type as input, typically f32)
        let math_float_fns = [
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "exp",
            "exp2",
            "log",
            "log2",
            "sqrt",
            "inverseSqrt",
            "abs",
            "sign",
            "floor",
            "ceil",
            "round",
            "trunc",
            "fract",
            "saturate",
            "radians",
            "degrees",
        ];
        if math_float_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Number));
        }

        // Two-argument math functions
        let math_binary_fns = ["pow", "min", "max", "step", "mod", "atan2"];
        if math_binary_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Number));
        }

        // Vector constructors
        match fn_name {
            "vec2" => return Some(ResolvedType::Primitive(Vec2)),
            "vec3" => return Some(ResolvedType::Primitive(Vec3)),
            "vec4" => return Some(ResolvedType::Primitive(Vec4)),
            "ivec2" => return Some(ResolvedType::Primitive(IVec2)),
            "ivec3" => return Some(ResolvedType::Primitive(IVec3)),
            "ivec4" => return Some(ResolvedType::Primitive(IVec4)),
            "uvec2" => return Some(ResolvedType::Primitive(UVec2)),
            "uvec3" => return Some(ResolvedType::Primitive(UVec3)),
            "uvec4" => return Some(ResolvedType::Primitive(UVec4)),
            "mat2" => return Some(ResolvedType::Primitive(Mat2)),
            "mat3" => return Some(ResolvedType::Primitive(Mat3)),
            "mat4" => return Some(ResolvedType::Primitive(Mat4)),
            _ => {}
        }

        // Type casts
        match fn_name {
            "f32" | "float" => return Some(ResolvedType::Primitive(Number)),
            "i32" | "int" => return Some(ResolvedType::Primitive(I32)),
            "u32" | "uint" => return Some(ResolvedType::Primitive(U32)),
            "bool" => return Some(ResolvedType::Primitive(Boolean)),
            _ => {}
        }

        // Vector operations that return scalars
        match fn_name {
            "length" | "distance" | "dot" => return Some(ResolvedType::Primitive(Number)),
            _ => {}
        }

        // Vector operations that return vectors (input-dependent, approximate as Vec3)
        let vec_to_vec_fns = ["normalize", "cross", "reflect", "refract", "faceforward"];
        if vec_to_vec_fns.contains(&fn_name) {
            return Some(ResolvedType::Primitive(Vec3));
        }

        // Mix/lerp returns same type as input
        if fn_name == "mix" || fn_name == "lerp" || fn_name == "smoothstep" || fn_name == "clamp" {
            return Some(ResolvedType::Primitive(Number));
        }

        None
    }

    /// Lower a closure expression.
    ///
    /// Closures are classified into two types:
    /// 1. Event mappings: 0-1 params, body is enum instantiation → `EventMapping`
    /// 2. General closures: arbitrary params/body → `Closure`
    fn lower_closure(&mut self, params: &[ClosureParam], body: &Expr) -> IrExpr {
        // Check if this is an event mapping (enum body with 0-1 params)
        let is_event_mapping = params.len() <= 1
            && matches!(
                body,
                Expr::EnumInstantiation { .. } | Expr::InferredEnumInstantiation { .. }
            );

        if is_event_mapping {
            return self.lower_event_mapping(params, body);
        }

        // General closure: lower params and body
        let lowered_params: Vec<(String, ResolvedType)> = params
            .iter()
            .map(|p| {
                let ty =
                    p.ty.as_ref()
                        .map_or_else(|| ResolvedType::TypeParam("Unknown".to_string()), |t| self.lower_type(t));
                (p.name.name.clone(), ty)
            })
            .collect();

        let body_ir = self.lower_expr(body);
        let return_ty = body_ir.ty().clone();

        let ty = ResolvedType::Closure {
            param_tys: lowered_params.iter().map(|(_, t)| t.clone()).collect(),
            return_ty: Box::new(return_ty),
        };

        IrExpr::Closure {
            params: lowered_params,
            body: Box::new(body_ir),
            ty,
        }
    }

    /// Lower a closure expression to an event mapping.
    ///
    /// Event mappings are restricted closures that:
    /// - Have zero or one parameter
    /// - Return an enum variant instantiation
    /// - Cannot capture variables from outer scope
    ///
    /// # Examples
    ///
    /// - `() -> .submit` → `EventMapping` with no param, variant "submit"
    /// - `x -> .changed(value: x)` → `EventMapping` with param "x", variant "changed", binding value→x
    fn lower_event_mapping(&mut self, params: &[ClosureParam], body: &Expr) -> IrExpr {
        // Validate: 0 or 1 parameter
        if params.len() > 1 {
            // For now, return a placeholder for invalid event mappings
            return IrExpr::Literal {
                value: Literal::Nil,
                ty: ResolvedType::TypeParam("InvalidEventMapping".to_string()),
            };
        }

        // Extract parameter name and type
        let param = params.first().map(|p| p.name.name.clone());
        let param_ty = params
            .first()
            .and_then(|p| p.ty.as_ref())
            .map(|t| Box::new(self.lower_type(t)));

        // Body must be an enum variant instantiation
        match body {
            Expr::EnumInstantiation {
                enum_name,
                variant,
                data,
                ..
            } => {
                // Resolve the enum type
                let (enum_id, return_ty) = self.resolve_event_enum_type(&enum_name.name);

                // Extract field bindings - check if they reference the parameter
                let field_bindings = Self::extract_event_field_bindings(data, param.as_deref());

                // Build the event mapping type
                let ty = ResolvedType::EventMapping {
                    param_ty,
                    return_ty: Box::new(return_ty),
                };

                IrExpr::EventMapping {
                    enum_id,
                    variant: variant.name.clone(),
                    param,
                    field_bindings,
                    ty,
                }
            }
            // Inferred enum instantiation: .variant or .variant(field: value)
            Expr::InferredEnumInstantiation { variant, data, .. } => {
                // Extract field bindings
                let field_bindings = Self::extract_event_field_bindings(data, param.as_deref());

                let ty = ResolvedType::EventMapping {
                    param_ty,
                    return_ty: Box::new(ResolvedType::TypeParam("InferredEvent".to_string())),
                };

                IrExpr::EventMapping {
                    enum_id: None,
                    variant: variant.name.clone(),
                    param,
                    field_bindings,
                    ty,
                }
            }
            Expr::Literal(_)
            | Expr::Invocation { .. }
            | Expr::Array { .. }
            | Expr::Tuple { .. }
            | Expr::Reference { .. }
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::ForExpr { .. }
            | Expr::IfExpr { .. }
            | Expr::MatchExpr { .. }
            | Expr::Group { .. }
            | Expr::DictLiteral { .. }
            | Expr::DictAccess { .. }
            | Expr::FieldAccess { .. }
            | Expr::ClosureExpr { .. }
            | Expr::LetExpr { .. }
            | Expr::MethodCall { .. }
            | Expr::Block { .. } => {
                // Invalid: body is not an enum variant
                IrExpr::Literal {
                    value: Literal::Nil,
                    ty: ResolvedType::TypeParam("InvalidEventMapping".to_string()),
                }
            }
        }
    }

    /// Resolve enum type for event mapping, returning (`enum_id`, `resolved_type`).
    fn resolve_event_enum_type(&self, enum_name: &str) -> (Option<super::super::EnumId>, ResolvedType) {
        self.module.enum_id(enum_name).map_or_else(
            || (None, ResolvedType::TypeParam(enum_name.to_string())),
            |enum_id| (Some(enum_id), ResolvedType::Enum(enum_id)),
        )
    }

    /// Extract field bindings from enum variant fields.
    ///
    /// Checks if field values reference the event mapping parameter.
    fn extract_event_field_bindings(
        fields: &[(ast::Ident, Expr)],
        param_name: Option<&str>,
    ) -> Vec<EventFieldBinding> {
        fields
            .iter()
            .map(|(field_name, value)| {
                let source = match value {
                    // Field references the parameter: `value: x`
                    // path[0] is bounds-safe: guarded by path.len() == 1 in the match guard
                    #[expect(clippy::indexing_slicing, reason = "len == 1 guard above guarantees index 0")]
                    Expr::Reference { path, .. }
                        if path.len() == 1 && param_name.is_some_and(|p| path[0].name == p) =>
                    {
                        EventBindingSource::Param(path[0].name.clone())
                    }
                    // Field has a literal value: `value: 42`
                    Expr::Literal(lit) => EventBindingSource::Literal(lit.clone()),
                    // For other expressions, treat as referencing param (best effort)
                    Expr::Invocation { .. }
                    | Expr::EnumInstantiation { .. }
                    | Expr::InferredEnumInstantiation { .. }
                    | Expr::Array { .. }
                    | Expr::Tuple { .. }
                    | Expr::Reference { .. }
                    | Expr::BinaryOp { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::ForExpr { .. }
                    | Expr::IfExpr { .. }
                    | Expr::MatchExpr { .. }
                    | Expr::Group { .. }
                    | Expr::DictLiteral { .. }
                    | Expr::DictAccess { .. }
                    | Expr::FieldAccess { .. }
                    | Expr::ClosureExpr { .. }
                    | Expr::LetExpr { .. }
                    | Expr::MethodCall { .. }
                    | Expr::Block { .. } => {
                        param_name.map_or(
                            EventBindingSource::Literal(Literal::Nil),
                            |p| EventBindingSource::Param(p.to_string()),
                        )
                    }
                };

                EventFieldBinding {
                    field_name: field_name.name.clone(),
                    source,
                }
            })
            .collect()
    }

    pub(super) fn extract_pattern_bindings(
        &self,
        pattern: &ast::Pattern,
        scrutinee: &IrExpr,
    ) -> Vec<(String, ResolvedType)> {
        match pattern {
            ast::Pattern::Variant { name, bindings } => {
                // Try to find variant field types from the enum
                let variant_fields = self.get_variant_fields(scrutinee.ty(), &name.name);

                bindings
                    .iter()
                    .enumerate()
                    .map(|(i, ident)| {
                        let ty = variant_fields
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| ResolvedType::TypeParam("Unknown".to_string()));
                        (ident.name.clone(), ty)
                    })
                    .collect()
            }
            ast::Pattern::Wildcard => {
                // Wildcard has no bindings
                Vec::new()
            }
        }
    }
}
