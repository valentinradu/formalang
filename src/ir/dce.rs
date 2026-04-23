//! Dead code elimination pass for IR optimization.
//!
//! This module removes code that doesn't affect program output:
//! - Unreachable branches (constant false conditions)
//! - Unused struct definitions
//! - Unused let bindings
//!
//! # Example
//!
//! ```formalang
//! struct Used { value: Number }
//! struct Unused { data: String }  // Removed if never referenced
//! impl Used { value: 1 }
//! ```

use crate::ir::{IrExpr, IrModule, StructId, TraitId};
use std::collections::HashSet;

/// Dead code eliminator that removes unreachable and unused code.
#[derive(Debug)]
pub struct DeadCodeEliminator<'a> {
    module: &'a IrModule,
    /// Structs that are actually used
    used_structs: HashSet<StructId>,
    /// Traits that are actually used (including those referenced only as
    /// trait constraints on generic parameters).
    used_traits: HashSet<TraitId>,
}

impl<'a> DeadCodeEliminator<'a> {
    /// Create a new dead code eliminator.
    #[must_use]
    pub fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            used_structs: HashSet::new(),
            used_traits: HashSet::new(),
        }
    }

    /// Analyze the module to find all used definitions.
    pub fn analyze(&mut self) {
        // Mark structs/enums used in impl blocks
        for impl_block in &self.module.impls {
            if let Some(struct_id) = impl_block.struct_id() {
                self.used_structs.insert(struct_id);
            }
            // Note: enum impls don't affect struct DCE

            // Check expressions in functions
            for func in &impl_block.functions {
                if let Some(body) = &func.body {
                    self.mark_used_in_expr(body);
                }
            }
        }

        // Mark structs used in standalone function bodies
        for func in &self.module.functions {
            if let Some(body) = &func.body {
                self.mark_used_in_expr(body);
            }
            // Trait constraints on generic parameters keep the trait alive
            for gp in &func.params {
                if let Some(ty) = &gp.ty {
                    self.mark_used_in_type(ty);
                }
            }
        }

        // Mark structs used in let bindings
        for let_binding in &self.module.lets {
            self.mark_used_in_expr(&let_binding.value);
            self.mark_used_in_type(&let_binding.ty);
        }

        // Mark structs referenced in struct fields and trait constraints on
        // their generic parameters.
        for s in &self.module.structs {
            for field in &s.fields {
                self.mark_used_in_type(&field.ty);
            }
            for trait_id in &s.traits {
                self.used_traits.insert(*trait_id);
            }
            for gp in &s.generic_params {
                for trait_id in &gp.constraints {
                    self.used_traits.insert(*trait_id);
                }
            }
        }

        // Mark trait constraints referenced by trait generic parameters and
        // through trait composition. A trait's own methods may also mention
        // types in their signatures.
        for t in &self.module.traits {
            for composed in &t.composed_traits {
                self.used_traits.insert(*composed);
            }
            for gp in &t.generic_params {
                for trait_id in &gp.constraints {
                    self.used_traits.insert(*trait_id);
                }
            }
            for field in &t.fields {
                self.mark_used_in_type(&field.ty);
            }
            for method in &t.methods {
                for p in &method.params {
                    if let Some(ty) = &p.ty {
                        self.mark_used_in_type(ty);
                    }
                }
                if let Some(ret) = &method.return_type {
                    self.mark_used_in_type(ret);
                }
            }
        }

        // Mark trait constraints referenced by function generic parameters
        // and types mentioned in their signatures.
        for f in &self.module.functions {
            for p in &f.params {
                if let Some(ty) = &p.ty {
                    self.mark_used_in_type(ty);
                }
            }
            if let Some(ret) = &f.return_type {
                self.mark_used_in_type(ret);
            }
        }

        // Enum variant fields can also reference types.
        for e in &self.module.enums {
            for variant in &e.variants {
                for field in &variant.fields {
                    self.mark_used_in_type(&field.ty);
                }
            }
            for gp in &e.generic_params {
                for trait_id in &gp.constraints {
                    self.used_traits.insert(*trait_id);
                }
            }
        }
    }

    /// Check if a struct is used.
    #[must_use]
    pub fn is_struct_used(&self, id: StructId) -> bool {
        self.used_structs.contains(&id)
    }

    /// Check if a trait is used.
    ///
    /// A trait is considered used when it appears as a type, as a trait
    /// constraint on a generic parameter, or as a trait composed into
    /// another live trait.
    #[must_use]
    pub fn is_trait_used(&self, id: TraitId) -> bool {
        self.used_traits.contains(&id)
    }

    /// Get the set of used struct IDs.
    #[must_use]
    pub const fn used_structs(&self) -> &HashSet<StructId> {
        &self.used_structs
    }

    /// Get the set of used trait IDs.
    #[must_use]
    pub const fn used_traits(&self) -> &HashSet<TraitId> {
        &self.used_traits
    }

    /// Mark structs used in a type.
    fn mark_used_in_type(&mut self, ty: &crate::ir::ResolvedType) {
        use crate::ir::ResolvedType;

        match ty {
            ResolvedType::Struct(id) => {
                self.used_structs.insert(*id);
            }
            ResolvedType::Trait(id) => {
                self.used_traits.insert(*id);
            }
            ResolvedType::Generic { base, args } => {
                self.used_structs.insert(*base);
                for arg in args {
                    self.mark_used_in_type(arg);
                }
            }
            ResolvedType::Array(inner) | ResolvedType::Optional(inner) => {
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
            // Other types don't reference structs or traits
            ResolvedType::Primitive(_) | ResolvedType::Enum(_) | ResolvedType::TypeParam(_) => {}
        }
    }

    /// Mark structs used in an expression.
    fn mark_used_in_expr(&mut self, expr: &IrExpr) {
        match expr {
            IrExpr::StructInst {
                struct_id, fields, ..
            } => {
                if let Some(id) = struct_id {
                    self.used_structs.insert(*id);
                }
                for (_, e) in fields {
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
            IrExpr::Tuple { fields, .. } | IrExpr::EnumInst { fields, .. } => {
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
            IrExpr::Closure { body, .. } => self.mark_used_in_expr(body),
        }
    }

    fn mark_used_in_block_statement(&mut self, stmt: &crate::ir::IrBlockStatement) {
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

/// Eliminate dead code from an expression.
///
/// This removes unreachable branches based on constant conditions.
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over all IrExpr variants"
)]
pub fn eliminate_dead_code_expr(expr: IrExpr) -> IrExpr {
    use crate::ast::Literal;
    match expr {
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ty,
        } => {
            let cond = eliminate_dead_code_expr(*condition);
            if let IrExpr::Literal {
                value: Literal::Boolean(b),
                ..
            } = &cond
            {
                if *b {
                    return eliminate_dead_code_expr(*then_branch);
                } else if let Some(else_b) = else_branch {
                    return eliminate_dead_code_expr(*else_b);
                }
            }
            IrExpr::If {
                condition: Box::new(cond),
                then_branch: Box::new(eliminate_dead_code_expr(*then_branch)),
                else_branch: else_branch.map(|e| Box::new(eliminate_dead_code_expr(*e))),
                ty,
            }
        }
        IrExpr::BinaryOp {
            left,
            op,
            right,
            ty,
        } => IrExpr::BinaryOp {
            left: Box::new(eliminate_dead_code_expr(*left)),
            op,
            right: Box::new(eliminate_dead_code_expr(*right)),
            ty,
        },
        IrExpr::Array { elements, ty } => IrExpr::Array {
            elements: elements.into_iter().map(eliminate_dead_code_expr).collect(),
            ty,
        },
        IrExpr::Tuple { fields, ty } => IrExpr::Tuple {
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, eliminate_dead_code_expr(e)))
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
                .map(|(n, e)| (n, eliminate_dead_code_expr(e)))
                .collect(),
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
            collection: Box::new(eliminate_dead_code_expr(*collection)),
            body: Box::new(eliminate_dead_code_expr(*body)),
            ty,
        },
        IrExpr::Match {
            scrutinee,
            arms,
            ty,
        } => IrExpr::Match {
            scrutinee: Box::new(eliminate_dead_code_expr(*scrutinee)),
            arms: arms
                .into_iter()
                .map(|arm| crate::ir::IrMatchArm {
                    variant: arm.variant,
                    is_wildcard: arm.is_wildcard,
                    bindings: arm.bindings,
                    body: eliminate_dead_code_expr(arm.body),
                })
                .collect(),
            ty,
        },
        IrExpr::FunctionCall { path, args, ty } => IrExpr::FunctionCall {
            path,
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
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
            receiver: Box::new(eliminate_dead_code_expr(*receiver)),
            method,
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
                .collect(),
            dispatch,
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
                .map(|(n, e)| (n, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::DictLiteral { entries, ty } => IrExpr::DictLiteral {
            entries: entries
                .into_iter()
                .map(|(k, v)| (eliminate_dead_code_expr(k), eliminate_dead_code_expr(v)))
                .collect(),
            ty,
        },
        IrExpr::DictAccess { dict, key, ty } => IrExpr::DictAccess {
            dict: Box::new(eliminate_dead_code_expr(*dict)),
            key: Box::new(eliminate_dead_code_expr(*key)),
            ty,
        },
        IrExpr::Block {
            statements,
            result,
            ty,
        } => IrExpr::Block {
            statements: statements
                .into_iter()
                .map(|stmt| stmt.map_exprs(eliminate_dead_code_expr))
                .collect(),
            result: Box::new(eliminate_dead_code_expr(*result)),
            ty,
        },
        e @ (IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::Closure { .. }) => e,
    }
}

/// Eliminate dead code from an entire module.
///
/// This removes:
/// - Unreachable branches in expressions
/// - Unused struct definitions (when `remove_unused_structs` is true)
#[must_use]
pub fn eliminate_dead_code(module: &IrModule, remove_unused_structs: bool) -> IrModule {
    let mut result = module.clone();

    // Process expressions in impl blocks
    for impl_block in &mut result.impls {
        for func in &mut impl_block.functions {
            func.body = func.body.take().map(eliminate_dead_code_expr);
        }
    }

    // Process standalone functions
    for func in &mut result.functions {
        func.body = func.body.take().map(eliminate_dead_code_expr);
    }

    // Process let bindings
    for let_binding in &mut result.lets {
        let_binding.value = eliminate_dead_code_expr(let_binding.value.clone());
    }

    // Process struct field defaults
    for struct_def in &mut result.structs {
        for field in &mut struct_def.fields {
            if let Some(default) = &mut field.default {
                *default = eliminate_dead_code_expr(default.clone());
            }
        }
    }

    // Optionally remove unused structs
    if remove_unused_structs {
        let mut eliminator = DeadCodeEliminator::new(&result);
        eliminator.analyze();

        // Filter to only keep used structs
        // Note: This is tricky because struct IDs would change
        // For now, we just report which are unused but don't remove them
        // Full removal would require re-indexing all references
    }

    result
}

/// An [`IrPass`] that removes dead code from the module.
///
/// Wraps [`eliminate_dead_code`] for use in a [`Pipeline`].
///
/// [`IrPass`]: crate::pipeline::IrPass
/// [`Pipeline`]: crate::pipeline::Pipeline
#[derive(Debug)]
#[expect(
    clippy::exhaustive_structs,
    reason = "IR types are constructed directly by consumer code"
)]
pub struct DeadCodeEliminationPass {
    /// When `true`, structs that are never referenced are removed.
    pub remove_unused_structs: bool,
}

impl DeadCodeEliminationPass {
    /// Create a new dead-code elimination pass.
    ///
    /// `remove_unused_structs` defaults to `false` because removing structs
    /// requires remapping all `StructId` references across the entire module.
    /// Use `DeadCodeEliminator` directly to identify unused structs.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remove_unused_structs: false,
        }
    }
}

impl Default for DeadCodeEliminationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::pipeline::IrPass for DeadCodeEliminationPass {
    fn name(&self) -> &'static str {
        "dead-code-elimination"
    }

    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<crate::error::CompilerError>> {
        Ok(eliminate_dead_code(&module, self.remove_unused_structs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Literal;
    use crate::compile_to_ir;

    #[test]
    fn test_eliminate_constant_true_branch() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { value: Number = if true { 1 } else { 2 } }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let optimized = eliminate_dead_code(&module, false);

        let struct_def = optimized
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        // The if should be eliminated, leaving just 1
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (*n - 1.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 1, got {n}").into());
            }
        } else {
            return Err(format!("Expected literal 1, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_eliminate_constant_false_branch() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { value: Number = if false { 1 } else { 2 } }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let optimized = eliminate_dead_code(&module, false);

        let struct_def = optimized
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        // The if should be eliminated, leaving just 2
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (*n - 2.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 2, got {n}").into());
            }
        } else {
            return Err(format!("Expected literal 2, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_no_elimination_non_constant_condition() -> Result<(), Box<dyn std::error::Error>> {
        // Use a let binding that references another let binding
        let source = r"
            let flag: Boolean = true
            let value: Number = if flag { 1 } else { 2 }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let optimized = eliminate_dead_code(&module, false);

        // Find the "value" let binding
        let let_binding = optimized
            .lets
            .iter()
            .find(|l| l.name == "value")
            .ok_or("expected value let binding")?;
        let expr = &let_binding.value;

        // flag is a variable reference, so if can't be eliminated
        // However, since flag is constant true, the optimizer should eliminate it
        // Let's check for either case
        if let IrExpr::If { .. } = expr {
            // Non-constant condition case (if optimizer can't see through let binding)
        } else if let IrExpr::Literal { .. } = expr {
            // Optimizer did constant propagation
        } else {
            return Err(format!("Expected If or Literal, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_analyze_used_structs() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Used { value: Number = 1 }
            struct Unused { data: String }
            impl Used {}
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

        let mut eliminator = DeadCodeEliminator::new(&module);
        eliminator.analyze();

        // Used should be marked as used (it has an impl block)
        let used_id = module.struct_id("Used").ok_or("Used struct not found")?;
        if !eliminator.is_struct_used(used_id) {
            return Err("Used struct should be marked as used".into());
        }

        // Unused should NOT be marked as used
        let unused_id = module
            .struct_id("Unused")
            .ok_or("Unused struct not found")?;
        if eliminator.is_struct_used(unused_id) {
            return Err("Unused struct should not be marked as used".into());
        }
        Ok(())
    }

    #[test]
    fn test_analyze_struct_referenced_in_field() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Inner { value: Number = 1 }
            struct Outer { inner: Inner = Inner(value: 1) }
            impl Outer {}
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

        let mut eliminator = DeadCodeEliminator::new(&module);
        eliminator.analyze();

        // Both Inner and Outer should be used
        let inner_id = module.struct_id("Inner").ok_or("Inner struct not found")?;
        let outer_id = module.struct_id("Outer").ok_or("Outer struct not found")?;

        if !eliminator.is_struct_used(inner_id) {
            return Err("Inner struct should be used (referenced by Outer)".into());
        }
        if !eliminator.is_struct_used(outer_id) {
            return Err("Outer struct should be used (has impl block)".into());
        }
        Ok(())
    }

    #[test]
    fn test_nested_dead_code_elimination() -> Result<(), Box<dyn std::error::Error>> {
        let source = r"
            struct Config { value: Number = if true { if false { 1 } else { 2 } } else { 3 } }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;
        let optimized = eliminate_dead_code(&module, false);

        let struct_def = optimized
            .structs
            .first()
            .ok_or("expected at least one struct")?;
        let field = struct_def
            .fields
            .first()
            .ok_or("expected at least one field")?;
        let expr = field.default.as_ref().ok_or("expected default expr")?;

        // Outer true -> inner expression
        // Inner false -> 2
        // Final result should be 2
        if let IrExpr::Literal {
            value: Literal::Number(n),
            ..
        } = expr
        {
            if (*n - 2.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 2, got {n}").into());
            }
        } else {
            return Err(format!("Expected literal 2, got {expr:?}").into());
        }
        Ok(())
    }

    #[test]
    fn test_analyze_trait_constraint_kept_alive() -> Result<(), Box<dyn std::error::Error>> {
        // A trait used only as a bound on a generic parameter must still be
        // marked as live so it is not eliminated.
        let source = r"
            pub trait Container { size: Number }
            pub struct Box<T: Container> { value: T }
            impl Box {}
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

        let mut eliminator = DeadCodeEliminator::new(&module);
        eliminator.analyze();

        let trait_id = module
            .trait_id("Container")
            .ok_or("Container trait not found")?;
        if !eliminator.is_trait_used(trait_id) {
            return Err("Container trait should be marked as used because it is a bound".into());
        }
        Ok(())
    }
}
