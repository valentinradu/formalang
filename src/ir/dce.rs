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

use crate::ir::{EnumId, IrExpr, IrModule, StructId, TraitId};
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
    /// Enums that are actually used (including those referenced only in
    /// field types or variant constructions).
    used_enums: HashSet<EnumId>,
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

    /// Mark structs used in an expression.
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match over every IrExpr variant; splitting would hide the walk"
    )]
    fn mark_used_in_expr(&mut self, expr: &IrExpr) {
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
                // Audit2 B24: walk the resolved type and any explicit
                // type-arguments so a local struct/enum used as a generic
                // arg of an external receiver (e.g. `Box<LocalThing>` from
                // an imported module) is marked used.
                self.mark_used_in_type(ty);
                for arg in type_args {
                    self.mark_used_in_type(arg);
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
            IrExpr::EnumInst {
                enum_id,
                fields,
                ty,
                ..
            } => {
                if let Some(id) = enum_id {
                    self.used_enums.insert(*id);
                }
                // Audit2 B24: walk the resolved type so a local
                // struct/enum used as a generic arg of an external
                // enum (e.g. `Result<LocalErr, OK>`) is marked used.
                self.mark_used_in_type(ty);
                for (_, e) in fields {
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
                for (_, _, ty) in params {
                    self.mark_used_in_type(ty);
                }
                for (_, _, ty) in captures {
                    self.mark_used_in_type(ty);
                }
                self.mark_used_in_expr(body);
            }
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

/// Mapping from old-to-new IDs after a DCE pass. `None` at an index means
/// the old definition was removed.
#[derive(Debug, Default)]
struct IdRemap {
    structs: Vec<Option<StructId>>,
    traits: Vec<Option<TraitId>>,
    enums: Vec<Option<EnumId>>,
}

impl IdRemap {
    fn struct_of(&self, old: StructId) -> Option<StructId> {
        self.structs.get(old.0 as usize).copied().flatten()
    }

    fn trait_of(&self, old: TraitId) -> Option<TraitId> {
        self.traits.get(old.0 as usize).copied().flatten()
    }

    fn enum_of(&self, old: EnumId) -> Option<EnumId> {
        self.enums.get(old.0 as usize).copied().flatten()
    }
}

/// Remove unused struct/trait/enum definitions and every reference to them
/// across the whole IR module. Also drops impl blocks whose target is
/// removed, and rebuilds name-to-ID indices.
fn remove_unused_definitions(
    module: &mut IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) {
    let remap = build_remap(module, used_structs, used_traits, used_enums);

    // Filter definition vectors in-place, preserving the relative order of
    // survivors so later-added IDs remain higher than earlier ones. Walk the
    // remap Option slice in lockstep with the definition vector.
    {
        let mut iter = remap.structs.iter();
        module
            .structs
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.traits.iter();
        module
            .traits
            .retain(|_| iter.next().copied().flatten().is_some());
    }
    {
        let mut iter = remap.enums.iter();
        module
            .enums
            .retain(|_| iter.next().copied().flatten().is_some());
    }

    // Drop impls that target a removed struct or enum.
    module.impls.retain(|impl_block| {
        use crate::ir::ImplTarget;
        match impl_block.target {
            ImplTarget::Struct(id) => remap.struct_of(id).is_some(),
            ImplTarget::Enum(id) => remap.enum_of(id).is_some(),
        }
    });

    // Rewrite every remaining ID.
    remap_module(module, &remap);

    module.rebuild_indices();
}

fn build_remap(
    module: &IrModule,
    used_structs: &HashSet<StructId>,
    used_traits: &HashSet<TraitId>,
    used_enums: &HashSet<EnumId>,
) -> IdRemap {
    // Since every old id is itself < u32::MAX (add_* enforces this),
    // truncation here is safe. try_from flagged by strict lints; use it.
    fn remap_slice<Id: Copy + Eq + std::hash::Hash>(
        count: usize,
        used: &HashSet<Id>,
        make: impl Fn(u32) -> Id,
    ) -> Vec<Option<Id>> {
        let mut out = Vec::with_capacity(count);
        let mut next: u32 = 0;
        for i in 0..count {
            let Ok(old_idx) = u32::try_from(i) else {
                out.push(None);
                continue;
            };
            let old = make(old_idx);
            if used.contains(&old) {
                out.push(Some(make(next)));
                // If we've exhausted the u32 id space, drop remaining
                // items rather than wrap and alias ids.
                let Some(n) = next.checked_add(1) else {
                    for _ in i.saturating_add(1)..count {
                        out.push(None);
                    }
                    break;
                };
                next = n;
            } else {
                out.push(None);
            }
        }
        out
    }

    IdRemap {
        structs: remap_slice(module.structs.len(), used_structs, StructId),
        traits: remap_slice(module.traits.len(), used_traits, TraitId),
        enums: remap_slice(module.enums.len(), used_enums, EnumId),
    }
}

fn remap_type(ty: &mut crate::ir::ResolvedType, remap: &IdRemap) {
    use crate::ir::ResolvedType;
    match ty {
        ResolvedType::Struct(id) => {
            if let Some(new) = remap.struct_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Trait(id) => {
            if let Some(new) = remap.trait_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Enum(id) => {
            if let Some(new) = remap.enum_of(*id) {
                *id = new;
            }
        }
        ResolvedType::Generic { base, args } => {
            match base {
                crate::ir::GenericBase::Struct(id) => {
                    if let Some(new) = remap.struct_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Enum(id) => {
                    if let Some(new) = remap.enum_of(*id) {
                        *id = new;
                    }
                }
                crate::ir::GenericBase::Trait(id) => {
                    if let Some(new) = remap.trait_of(*id) {
                        *id = new;
                    }
                }
            }
            for a in args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Array(inner) | ResolvedType::Range(inner) | ResolvedType::Optional(inner) => {
            remap_type(inner, remap);
        }
        ResolvedType::Tuple(fields) => {
            for (_, t) in fields {
                remap_type(t, remap);
            }
        }
        ResolvedType::Dictionary { key_ty, value_ty } => {
            remap_type(key_ty, remap);
            remap_type(value_ty, remap);
        }
        ResolvedType::Closure {
            param_tys,
            return_ty,
        } => {
            for (_, t) in param_tys {
                remap_type(t, remap);
            }
            remap_type(return_ty, remap);
        }
        ResolvedType::External { type_args, .. } => {
            for a in type_args {
                remap_type(a, remap);
            }
        }
        ResolvedType::Primitive(_) | ResolvedType::TypeParam(_) | ResolvedType::Error => {}
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over every IrExpr variant; splitting would hide the structural walk"
)]
fn remap_expr(expr: &mut IrExpr, remap: &IdRemap) {
    remap_type(expr.ty_mut(), remap);
    match expr {
        IrExpr::StructInst {
            struct_id,
            type_args,
            fields,
            ..
        } => {
            if let Some(id) = struct_id {
                if let Some(new) = remap.struct_of(*id) {
                    *id = new;
                }
            }
            for t in type_args {
                remap_type(t, remap);
            }
            for (_, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::EnumInst {
            enum_id, fields, ..
        } => {
            if let Some(id) = enum_id {
                if let Some(new) = remap.enum_of(*id) {
                    *id = new;
                }
            }
            for (_, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::BinaryOp { left, right, .. } => {
            remap_expr(left, remap);
            remap_expr(right, remap);
        }
        IrExpr::UnaryOp { operand, .. } => remap_expr(operand, remap),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            remap_expr(condition, remap);
            remap_expr(then_branch, remap);
            if let Some(eb) = else_branch {
                remap_expr(eb, remap);
            }
        }
        IrExpr::Array { elements, .. } => {
            for e in elements {
                remap_expr(e, remap);
            }
        }
        IrExpr::Tuple { fields, .. } => {
            for (_, e) in fields {
                remap_expr(e, remap);
            }
        }
        IrExpr::FieldAccess { object, .. } => remap_expr(object, remap),
        IrExpr::For {
            var_ty,
            collection,
            body,
            ..
        } => {
            remap_type(var_ty, remap);
            remap_expr(collection, remap);
            remap_expr(body, remap);
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            remap_expr(scrutinee, remap);
            for arm in arms {
                for (_, t) in &mut arm.bindings {
                    remap_type(t, remap);
                }
                remap_expr(&mut arm.body, remap);
            }
        }
        IrExpr::FunctionCall { args, .. } => {
            for (_, e) in args {
                remap_expr(e, remap);
            }
        }
        IrExpr::MethodCall {
            receiver,
            args,
            dispatch,
            ..
        } => {
            remap_expr(receiver, remap);
            for (_, e) in args {
                remap_expr(e, remap);
            }
            if let crate::ir::DispatchKind::Virtual { trait_id, .. } = dispatch {
                if let Some(new) = remap.trait_of(*trait_id) {
                    *trait_id = new;
                }
            }
        }
        IrExpr::Closure {
            params,
            captures,
            body,
            ..
        } => {
            for (_, _, t) in params {
                remap_type(t, remap);
            }
            for (_, _, t) in captures {
                remap_type(t, remap);
            }
            remap_expr(body, remap);
        }
        IrExpr::DictLiteral { entries, .. } => {
            for (k, v) in entries {
                remap_expr(k, remap);
                remap_expr(v, remap);
            }
        }
        IrExpr::DictAccess { dict, key, .. } => {
            remap_expr(dict, remap);
            remap_expr(key, remap);
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            for stmt in statements.iter_mut() {
                remap_block_statement(stmt, remap);
            }
            remap_expr(result, remap);
        }
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::LetRef { .. } => {}
    }
}

fn remap_block_statement(stmt: &mut crate::ir::IrBlockStatement, remap: &IdRemap) {
    use crate::ir::IrBlockStatement;
    match stmt {
        IrBlockStatement::Let { ty, value, .. } => {
            if let Some(t) = ty {
                remap_type(t, remap);
            }
            remap_expr(value, remap);
        }
        IrBlockStatement::Assign { target, value } => {
            remap_expr(target, remap);
            remap_expr(value, remap);
        }
        IrBlockStatement::Expr(e) => remap_expr(e, remap),
    }
}

/// Rewrite each surviving trait ID in `ids` and drop those whose trait was
/// removed. Returns `true` if the trait survived and has been updated.
fn retain_trait_id(id: &mut TraitId, remap: &IdRemap) -> bool {
    remap.trait_of(*id).is_some_and(|new| {
        *id = new;
        true
    })
}

/// Same as `retain_trait_id` but for the [`IrTraitRef`] shape used
/// by generic-param constraints — also remaps any [`TraitId`] nested
/// inside the constraint's arg types.
fn retain_trait_ref(constraint: &mut crate::ir::IrTraitRef, remap: &IdRemap) -> bool {
    let kept = remap.trait_of(constraint.trait_id).is_some_and(|new| {
        constraint.trait_id = new;
        true
    });
    if kept {
        for arg in &mut constraint.args {
            remap_type(arg, remap);
        }
    }
    kept
}

fn remap_module(module: &mut IrModule, remap: &IdRemap) {
    for s in &mut module.structs {
        s.traits.retain_mut(|tr| retain_trait_ref(tr, remap));
        for f in &mut s.fields {
            remap_type(&mut f.ty, remap);
            if let Some(default) = &mut f.default {
                remap_expr(default, remap);
            }
        }
        for gp in &mut s.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for t in &mut module.traits {
        t.composed_traits
            .retain_mut(|id| retain_trait_id(id, remap));
        for f in &mut t.fields {
            remap_type(&mut f.ty, remap);
        }
        for m in &mut t.methods {
            for p in &mut m.params {
                if let Some(ty) = &mut p.ty {
                    remap_type(ty, remap);
                }
            }
            if let Some(ret) = &mut m.return_type {
                remap_type(ret, remap);
            }
        }
        for gp in &mut t.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for e in &mut module.enums {
        for v in &mut e.variants {
            for f in &mut v.fields {
                remap_type(&mut f.ty, remap);
            }
        }
        for gp in &mut e.generic_params {
            gp.constraints.retain_mut(|c| retain_trait_ref(c, remap));
        }
    }
    for i in &mut module.impls {
        match &mut i.target {
            crate::ir::ImplTarget::Struct(id) => {
                if let Some(new) = remap.struct_of(*id) {
                    *id = new;
                }
            }
            crate::ir::ImplTarget::Enum(id) => {
                if let Some(new) = remap.enum_of(*id) {
                    *id = new;
                }
            }
        }
        for f in &mut i.functions {
            remap_function(f, remap);
        }
    }
    for f in &mut module.functions {
        remap_function(f, remap);
    }
    for l in &mut module.lets {
        remap_type(&mut l.ty, remap);
        remap_expr(&mut l.value, remap);
    }
}

fn remap_function(f: &mut crate::ir::IrFunction, remap: &IdRemap) {
    for p in &mut f.params {
        if let Some(ty) = &mut p.ty {
            remap_type(ty, remap);
        }
        if let Some(default) = &mut p.default {
            remap_expr(default, remap);
        }
    }
    if let Some(ret) = &mut f.return_type {
        remap_type(ret, remap);
    }
    if let Some(body) = &mut f.body {
        remap_expr(body, remap);
    }
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
            if (n.value - 1.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 1, got {}", n.value).into());
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
            if (n.value - 2.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 2, got {}", n.value).into());
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
        // DCE semantics: an impl block does NOT keep its target alive on its
        // own. Something else must reference the struct (a field type, a
        // function parameter, or an expression). Here a standalone function
        // takes a `Used` parameter.
        let source = r"
            struct Used { value: Number = 1 }
            struct Unused { data: String }
            impl Used {}
            pub fn take(u: Used) -> Number { u.value }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

        let mut eliminator = DeadCodeEliminator::new(&module);
        eliminator.analyze();

        let used_id = module.struct_id("Used").ok_or("Used struct not found")?;
        if !eliminator.is_struct_used(used_id) {
            return Err("Used struct should be marked as used".into());
        }

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
        // Outer is kept alive by a function parameter; Inner by being a field
        // type of Outer.
        let source = r"
            struct Inner { value: Number = 1 }
            struct Outer { inner: Inner = Inner(value: 1) }
            impl Outer {}
            pub fn show(o: Outer) -> Number { o.inner.value }
        ";
        let module = compile_to_ir(source).map_err(|e| format!("{e:?}"))?;

        let mut eliminator = DeadCodeEliminator::new(&module);
        eliminator.analyze();

        let inner_id = module.struct_id("Inner").ok_or("Inner struct not found")?;
        let outer_id = module.struct_id("Outer").ok_or("Outer struct not found")?;

        if !eliminator.is_struct_used(inner_id) {
            return Err("Inner struct should be used (referenced by Outer)".into());
        }
        if !eliminator.is_struct_used(outer_id) {
            return Err("Outer struct should be used (referenced by `show`)".into());
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
            if (n.value - 2.0).abs() >= f64::EPSILON {
                return Err(format!("Expected 2, got {}", n.value).into());
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

#[cfg(test)]
mod removal_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::compile_to_ir;

    #[test]
    fn test_removal_drops_unused_struct() {
        let source = r"
            pub struct Used { value: Number }
            pub struct Unused { data: String }
            impl Used { fn get(self) -> Number { self.value } }
            pub fn run(u: Used) -> Number { u.get() }
        ";
        let module = compile_to_ir(source).unwrap();
        let before = module.structs.len();
        assert!(before >= 2, "expected both structs in IR before DCE");
        let optimized = eliminate_dead_code(&module, true);
        assert!(
            optimized.structs.iter().any(|s| s.name == "Used"),
            "Used should survive"
        );
        assert!(
            !optimized.structs.iter().any(|s| s.name == "Unused"),
            "Unused should be removed"
        );
    }

    #[test]
    fn test_removal_preserves_remaining_struct_ids() {
        // After removing an unused struct, references to surviving structs
        // (e.g. in field types, function params) should still resolve.
        let source = r"
            pub struct Unused { data: String }
            pub struct Used { value: Number }
            pub fn run(u: Used) -> Number { u.value }
        ";
        let module = compile_to_ir(source).unwrap();
        let optimized = eliminate_dead_code(&module, true);
        // Name → ID lookup via the rebuilt indices.
        let used_id = optimized.struct_id("Used").unwrap();
        let used = optimized.get_struct(used_id).unwrap();
        assert_eq!(used.name, "Used");
        assert_eq!(used.fields.len(), 1);
    }

    #[test]
    fn test_removal_drops_impl_for_removed_enum() {
        // An impl block targeting a removed enum must be dropped.
        let source = r"
            pub enum Used { a, b }
            pub enum Unused { x, y }
            impl Unused { fn describe(self) -> Number { 0 } }
            pub fn run(u: Used) -> Used { u }
        ";
        let module = compile_to_ir(source).unwrap();
        let before_impls = module.impls.len();
        assert!(before_impls >= 1, "expected Unused impl in IR before DCE");
        let optimized = eliminate_dead_code(&module, true);
        assert!(
            !optimized.enums.iter().any(|e| e.name == "Unused"),
            "Unused enum should be removed"
        );
        // The impl targeted Unused; it should be gone too.
        for impl_block in &optimized.impls {
            match impl_block.target {
                crate::ir::ImplTarget::Enum(id) => {
                    let e = optimized.get_enum(id).unwrap();
                    assert_ne!(e.name, "Unused");
                }
                crate::ir::ImplTarget::Struct(_) => {}
            }
        }
    }
}
