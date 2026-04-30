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
//! struct Used { value: I32 }
//! struct Unused { data: String }  // Removed if never referenced
//! impl Used { value: 1 }
//! ```

mod filtering;
mod reachability;
mod remap;

#[cfg(test)]
mod tests;

use crate::ir::{EnumId, IrExpr, IrModule, StructId, TraitId};
use std::collections::HashSet;

use remap::remove_unused_definitions;

/// Dead code eliminator that removes unreachable and unused code.
#[derive(Debug)]
pub struct DeadCodeEliminator<'a> {
    pub(super) module: &'a IrModule,
    /// Structs that are actually used
    pub(super) used_structs: HashSet<StructId>,
    /// Traits that are actually used (including those referenced only as
    /// trait constraints on generic parameters).
    pub(super) used_traits: HashSet<TraitId>,
    /// Enums that are actually used (including those referenced only in
    /// field types or variant constructions).
    pub(super) used_enums: HashSet<EnumId>,
}

impl<'a> DeadCodeEliminator<'a> {
    /// Create a new dead code eliminator.
    #[must_use]
    pub fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            used_structs: HashSet::new(),
            used_traits: HashSet::new(),
            used_enums: HashSet::new(),
        }
    }

    /// Analyze the module to find all used definitions.
    pub fn analyze(&mut self) {
        // Walk impl-method bodies for references. Do NOT mark the impl's
        // target type as used purely because an impl block exists; an impl
        // is dead code if nothing else references its target. Impls whose
        // target is removed are pruned by `remove_unused_definitions`.
        for impl_block in &self.module.impls {
            for func in &impl_block.functions {
                if let Some(body) = &func.body {
                    self.mark_used_in_expr(body);
                }
                for p in &func.params {
                    if let Some(ty) = &p.ty {
                        self.mark_used_in_type(ty);
                    }
                }
                if let Some(ret) = &func.return_type {
                    self.mark_used_in_type(ret);
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
            for trait_ref in &s.traits {
                self.used_traits.insert(trait_ref.trait_id);
                for arg in &trait_ref.args {
                    self.mark_used_in_type(arg);
                }
            }
            for gp in &s.generic_params {
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
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
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
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
                for constraint in &gp.constraints {
                    self.used_traits.insert(constraint.trait_id);
                    for arg in &constraint.args {
                        self.mark_used_in_type(arg);
                    }
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

    /// Get the set of used enum IDs.
    #[must_use]
    pub const fn used_enums_set(&self) -> &HashSet<EnumId> {
        &self.used_enums
    }

    /// Check if an enum is used.
    #[must_use]
    pub fn is_enum_used(&self, id: EnumId) -> bool {
        self.used_enums.contains(&id)
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
                .map(|(n, idx, e)| (n, idx, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::For {
            var,
            var_ty,
            var_binding_id,
            collection,
            body,
            ty,
        } => IrExpr::For {
            var,
            var_ty,
            var_binding_id,
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
                    variant_idx: arm.variant_idx,
                    is_wildcard: arm.is_wildcard,
                    bindings: arm.bindings,
                    body: eliminate_dead_code_expr(arm.body),
                })
                .collect(),
            ty,
        },
        IrExpr::FunctionCall {
            path,
            function_id,
            args,
            ty,
        } => IrExpr::FunctionCall {
            path,
            function_id,
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
                .collect(),
            ty,
        },
        IrExpr::CallClosure { closure, args, ty } => IrExpr::CallClosure {
            closure: Box::new(eliminate_dead_code_expr(*closure)),
            args: args
                .into_iter()
                .map(|(name, e)| (name, eliminate_dead_code_expr(e)))
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
            receiver: Box::new(eliminate_dead_code_expr(*receiver)),
            method,
            method_idx,
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
            variant_idx,
            fields,
            ty,
        } => IrExpr::EnumInst {
            enum_id,
            variant,
            variant_idx,
            fields: fields
                .into_iter()
                .map(|(n, idx, e)| (n, idx, eliminate_dead_code_expr(e)))
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
        | IrExpr::Closure { .. }
        | IrExpr::ClosureRef { .. }) => e,
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

    // Physically remove unused structs/traits/enums, then rewrite every ID
    // reference so the module stays internally consistent.
    if remove_unused_structs {
        let mut eliminator = DeadCodeEliminator::new(&result);
        eliminator.analyze();
        let used_structs = eliminator.used_structs.clone();
        let used_traits = eliminator.used_traits.clone();
        let used_enums = eliminator.used_enums.clone();
        drop(eliminator);
        remove_unused_definitions(&mut result, &used_structs, &used_traits, &used_enums);
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
    /// Physically removes unused struct, trait, and enum definitions (and any
    /// impl blocks whose target is removed), then rewrites every surviving ID
    /// reference across the module and rebuilds the name-to-ID indices.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remove_unused_structs: true,
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
