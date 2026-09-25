//! Removal of `let` bindings that nothing reads.
//!
//! A binding goes only when two things are true:
//!
//! - no expression in its scope names it;
//! - its value cannot have an effect or a fault. A call can reach an
//!   extern function, and a division or an index can fault, so a
//!   binding whose value holds one of these stays. Removing it could
//!   change what the program does.
//!
//! A module `let` goes only when it is private: a public one is part of
//! the module's contract, like a public struct.

use std::collections::HashSet;

use crate::ast::{BinaryOperator, Visibility};
use crate::ir::{
    walk_expr, walk_expr_children, walk_expr_children_mut, IrBlockStatement, IrExpr, IrModule,
    IrVisitor, LetId, ReferenceTarget,
};

/// Remove the local bindings that nothing reads. When
/// `remove_definitions` is true, remove the private module bindings
/// that nothing reads too: a module binding is a definition, like a
/// struct, and the caller chose whether definitions go.
pub(super) fn remove_unused_lets(module: &mut IrModule, remove_definitions: bool) {
    for_each_root_mut(module, &mut remove_unused_locals);
    if remove_definitions {
        remove_unused_module_lets(module);
    }
}

/// The names that `expr` reads. A reference counts by its whole path
/// and by its last segment, so a qualified path such as `m::x` keeps a
/// binding called `m::x` and one called `x`.
fn names_in(expr: &IrExpr, out: &mut HashSet<String>) {
    struct Names<'a>(&'a mut HashSet<String>);
    impl IrVisitor for Names<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::Reference { path, .. } = e {
                self.0.insert(path.join("::"));
                if let Some(last) = path.last() {
                    self.0.insert(last.clone());
                }
            } else if let IrExpr::LetRef { name, .. } = e {
                self.0.insert(name.clone());
            } else if let IrExpr::Closure { captures, .. } = e {
                for (_, name, _, _) in captures {
                    self.0.insert(name.clone());
                }
            }
            walk_expr_children(self, e);
        }
    }
    walk_expr(&mut Names(out), expr);
}

/// Whether evaluating `expr` can have no effect and no fault.
fn effect_free(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::Reference { .. }
        | IrExpr::LetRef { .. }
        | IrExpr::SelfFieldRef { .. }
        // A closure value runs nothing until a call.
        | IrExpr::Closure { .. } => true,
        IrExpr::StructInst { fields, .. } | IrExpr::EnumInst { fields, .. } => {
            fields.iter().all(|(_, _, e)| effect_free(e))
        }
        IrExpr::Tuple { fields, .. } => fields.iter().all(|(_, e)| effect_free(e)),
        IrExpr::Array { elements, .. } => elements.iter().all(effect_free),
        IrExpr::DictLiteral { entries, .. } => {
            entries.iter().all(|(k, v)| effect_free(k) && effect_free(v))
        }
        IrExpr::FieldAccess { object, .. } => effect_free(object),
        IrExpr::UnaryOp { operand, .. } => effect_free(operand),
        // A division or a remainder faults on a zero divisor.
        IrExpr::BinaryOp {
            left, op, right, ..
        } => !matches!(op, BinaryOperator::Div | BinaryOperator::Mod)
            && effect_free(left)
            && effect_free(right),
        IrExpr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            effect_free(condition)
                && effect_free(then_branch)
                && else_branch.as_deref().is_none_or(effect_free)
        }
        IrExpr::Block {
            statements, result, ..
        } => {
            statements.iter().all(|s| match s {
                IrBlockStatement::Let { value, .. } | IrBlockStatement::Expr(value) => {
                    effect_free(value)
                }
                IrBlockStatement::Assign { .. } => false,
            }) && effect_free(result)
        }
        IrExpr::ClosureRef { env_struct, .. } => effect_free(env_struct),
        IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::FunctionCall { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::DictAccess { .. } => false,
    }
}

/// Remove the unused local bindings in every block inside `expr`.
fn remove_unused_locals(expr: &mut IrExpr) {
    walk_expr_children_mut(expr, &mut remove_unused_locals);
    let IrExpr::Block {
        statements, result, ..
    } = expr
    else {
        return;
    };
    // Walk the block from its end. `used` holds the names that the
    // statements after the current one read. A binding that none of
    // them reads, and that has no effect, goes.
    let mut used = HashSet::new();
    names_in(result, &mut used);
    let mut keep = vec![true; statements.len()];
    for (index, stmt) in statements.iter().enumerate().rev() {
        match stmt {
            IrBlockStatement::Let { name, value, .. } => {
                if !used.contains(name) && effect_free(value) {
                    if let Some(k) = keep.get_mut(index) {
                        *k = false;
                    }
                    continue;
                }
                // The names read after this point, and called `name`,
                // mean this binding. An earlier binding of that name is
                // shadowed there.
                used.remove(name);
                names_in(value, &mut used);
            }
            IrBlockStatement::Assign { target, value, .. } => {
                names_in(target, &mut used);
                names_in(value, &mut used);
            }
            IrBlockStatement::Expr(e) => names_in(e, &mut used),
        }
    }
    let mut flags = keep.into_iter();
    statements.retain(|_| flags.next().unwrap_or(true));
}

/// Remove the private module bindings that nothing reads. Removing one
/// can leave another unread, so repeat until nothing more goes.
fn remove_unused_module_lets(module: &mut IrModule) {
    loop {
        let mut used_names = HashSet::new();
        let mut used_ids = HashSet::new();
        for root in roots(module) {
            names_in(root, &mut used_names);
            let_ids_in(root, &mut used_ids);
        }
        let remove: Vec<bool> = module
            .lets
            .iter()
            .enumerate()
            .map(|(index, l)| {
                let id = u32::try_from(index).ok().map(LetId);
                let last = l.name.rsplit("::").next().unwrap_or(&l.name);
                l.visibility != Visibility::Public
                    && !used_names.contains(&l.name)
                    && !used_names.contains(last)
                    && id.is_none_or(|id| !used_ids.contains(&id))
                    && effect_free(&l.value)
            })
            .collect();
        if !remove.contains(&true) {
            return;
        }
        // The new id of each binding that stays.
        let mut next: u32 = 0;
        let remap: Vec<Option<LetId>> = remove
            .iter()
            .map(|&gone| {
                if gone {
                    None
                } else {
                    let id = LetId(next);
                    next = next.saturating_add(1);
                    Some(id)
                }
            })
            .collect();
        let mut flags = remove.into_iter();
        module.lets.retain(|_| !flags.next().unwrap_or(false));
        for_each_root_mut(module, &mut |root| remap_let_ids(root, &remap));
        module.rebuild_indices();
    }
}

/// The module bindings that `expr` names by id.
fn let_ids_in(expr: &IrExpr, out: &mut HashSet<LetId>) {
    struct Ids<'a>(&'a mut HashSet<LetId>);
    impl IrVisitor for Ids<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExpr::Reference {
                target: ReferenceTarget::ModuleLet(id),
                ..
            } = e
            {
                self.0.insert(*id);
            }
            walk_expr_children(self, e);
        }
    }
    walk_expr(&mut Ids(out), expr);
}

/// Point each reference to a module binding at its new id.
fn remap_let_ids(expr: &mut IrExpr, remap: &[Option<LetId>]) {
    if let IrExpr::Reference {
        target: ReferenceTarget::ModuleLet(id),
        ..
    } = expr
    {
        if let Some(Some(new)) = remap.get(id.0 as usize) {
            *id = *new;
        }
    }
    walk_expr_children_mut(expr, &mut |child| remap_let_ids(child, remap));
}

/// Every expression that is not inside another one: bodies, defaults and
/// module binding values.
fn roots(module: &IrModule) -> Vec<&IrExpr> {
    let functions = module
        .functions
        .iter()
        .chain(module.impls.iter().flat_map(|i| i.functions.iter()));
    let mut out = Vec::new();
    for f in functions {
        out.extend(f.body.as_ref());
        out.extend(f.params.iter().filter_map(|p| p.default.as_ref()));
    }
    let fields = module
        .structs
        .iter()
        .flat_map(|s| s.fields.iter())
        .chain(
            module
                .enums
                .iter()
                .flat_map(|e| e.variants.iter().flat_map(|v| v.fields.iter())),
        )
        .chain(module.traits.iter().flat_map(|t| t.fields.iter()));
    out.extend(fields.filter_map(|f| f.default.as_ref()));
    out.extend(module.lets.iter().map(|l| &l.value));
    out
}

/// Call `visit` on every root expression, as [`roots`] lists them.
fn for_each_root_mut(module: &mut IrModule, visit: &mut impl FnMut(&mut IrExpr)) {
    let functions = module
        .functions
        .iter_mut()
        .chain(module.impls.iter_mut().flat_map(|i| i.functions.iter_mut()));
    for f in functions {
        if let Some(body) = f.body.as_mut() {
            visit(body);
        }
        for p in &mut f.params {
            if let Some(default) = p.default.as_mut() {
                visit(default);
            }
        }
    }
    let fields = module
        .structs
        .iter_mut()
        .flat_map(|s| s.fields.iter_mut())
        .chain(
            module
                .enums
                .iter_mut()
                .flat_map(|e| e.variants.iter_mut().flat_map(|v| v.fields.iter_mut())),
        )
        .chain(module.traits.iter_mut().flat_map(|t| t.fields.iter_mut()));
    for f in fields {
        if let Some(default) = f.default.as_mut() {
            visit(default);
        }
    }
    for l in &mut module.lets {
        visit(&mut l.value);
    }
}
