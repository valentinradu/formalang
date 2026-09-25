//! Name changes that the linker makes: the hidden name of an own item,
//! the path of a call, and the final names of the imported items.

use std::collections::HashMap;

use crate::ir::monomorphise::walkers::walk_expr_children_mut;
use crate::ir::{IrExpr, IrFunction, IrModule};

use super::{hidden_name, Counts, Kind};

/// Give the own item `index` of kind `kind` the hidden name of module
/// `id`.
pub(super) fn hide_own_name(module: &mut IrModule, kind: Kind, index: usize, id: u32) {
    let slot = match kind {
        Kind::Struct => module.structs.get_mut(index).map(|s| &mut s.name),
        Kind::Enum => module.enums.get_mut(index).map(|e| &mut e.name),
        Kind::Trait => module.traits.get_mut(index).map(|t| &mut t.name),
        Kind::Function => module.functions.get_mut(index).map(|f| &mut f.name),
        Kind::Let => module.lets.get_mut(index).map(|l| &mut l.name),
        Kind::Impl => None,
    };
    if let Some(name) = slot {
        *name = hidden_name(id, name);
    }
}

/// The table that gives a call path its function.
struct Calls<'a> {
    /// The name of each function now, by id.
    now: Vec<String>,
    /// The id of each function by the name it had while the module
    /// lowered. Overloads share a name; the first one is enough to
    /// give the call its new path.
    by_old_name: HashMap<&'a str, usize>,
}

impl Calls<'_> {
    /// The new path of a call with the path `path`, made in an item
    /// whose old name has the module prefix `prefix`.
    fn path_for(&self, path: &[String], function_id: Option<u32>, prefix: &str) -> Option<String> {
        if let Some(id) = function_id {
            return usize::try_from(id)
                .ok()
                .and_then(|i| self.now.get(i))
                .cloned();
        }
        let joined = path.join("::");
        let scoped = (!prefix.is_empty() && path.len() == 1).then(|| format!("{prefix}::{joined}"));
        scoped
            .as_deref()
            .and_then(|name| self.by_old_name.get(name))
            .or_else(|| self.by_old_name.get(joined.as_str()))
            .and_then(|&i| self.now.get(i))
            .cloned()
    }
}

/// The module prefix of an item name: `m` for `m::f`, empty for `f`.
fn prefix_of(name: &str) -> &str {
    name.rsplit_once("::").map_or("", |(prefix, _)| prefix)
}

/// Give each call in the module's own code the path of the function
/// it calls, by the name the function has now.
///
/// `before` holds the name of each function while the module lowered.
/// The own items are those after `linked`.
pub(super) fn retarget_calls(module: &mut IrModule, linked: &Counts, before: &[String]) {
    let calls = Calls {
        now: module.functions.iter().map(|f| f.name.clone()).collect(),
        by_old_name: before
            .iter()
            .enumerate()
            .rev()
            .map(|(i, name)| (name.as_str(), i))
            .collect(),
    };
    for (index, f) in module.functions.iter_mut().enumerate() {
        if index >= linked.get(Kind::Function) {
            let prefix = before.get(index).map_or("", |n| prefix_of(n));
            retarget_function(f, &calls, prefix);
        }
    }
    let start = linked.get(Kind::Impl);
    for imp in module.impls.iter_mut().skip(start) {
        for f in &mut imp.functions {
            retarget_function(f, &calls, "");
        }
    }
    let start = linked.get(Kind::Struct);
    for s in module.structs.iter_mut().skip(start) {
        let prefix = prefix_of(&s.name).to_string();
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                retarget_expr(default, &calls, &prefix);
            }
        }
    }
    let start = linked.get(Kind::Let);
    for l in module.lets.iter_mut().skip(start) {
        retarget_expr(&mut l.value, &calls, "");
    }
}

fn retarget_function(f: &mut IrFunction, calls: &Calls<'_>, prefix: &str) {
    for param in &mut f.params {
        if let Some(default) = &mut param.default {
            retarget_expr(default, calls, prefix);
        }
    }
    if let Some(body) = &mut f.body {
        retarget_expr(body, calls, prefix);
    }
}

fn retarget_expr(expr: &mut IrExpr, calls: &Calls<'_>, prefix: &str) {
    if let IrExpr::FunctionCall {
        path, function_id, ..
    } = expr
    {
        if let Some(name) = calls.path_for(path, function_id.map(|id| id.0), prefix) {
            *path = name.split("::").map(String::from).collect();
        }
    }
    walk_expr_children_mut(expr, &mut |child| retarget_expr(child, calls, prefix));
}

/// Replace each hidden module prefix in `name` by its final path.
fn renamed(name: &str, prefixes: &HashMap<String, String>) -> Option<String> {
    let (head, rest) = name.split_once("::")?;
    prefixes.get(head).map(|path| {
        if path.is_empty() {
            rest.to_string()
        } else {
            format!("{path}::{rest}")
        }
    })
}

fn rename_in_place(name: &mut String, prefixes: &HashMap<String, String>) {
    if let Some(new) = renamed(name, prefixes) {
        *name = new;
    }
}

fn rename_path(path: &mut Vec<String>, prefixes: &HashMap<String, String>) {
    if let Some(path_str) = path.first().and_then(|head| prefixes.get(head)) {
        let mut new: Vec<String> = if path_str.is_empty() {
            Vec::new()
        } else {
            path_str.split("::").map(String::from).collect()
        };
        new.extend(path.drain(1..));
        *path = new;
    }
}

/// Give each item with a hidden name its final name, and each
/// reference to it the same name. `prefixes` maps a hidden prefix,
/// `@3`, to the final module path, `utils::helpers`.
///
/// A hidden prefix cannot appear in the source, so a name that starts
/// with one always names an imported item, and no scope rule applies.
pub(super) fn rename_prefixes(module: &mut IrModule, prefixes: &HashMap<String, String>) {
    for s in &mut module.structs {
        rename_in_place(&mut s.name, prefixes);
    }
    for e in &mut module.enums {
        rename_in_place(&mut e.name, prefixes);
    }
    for t in &mut module.traits {
        rename_in_place(&mut t.name, prefixes);
    }
    for l in &mut module.lets {
        rename_in_place(&mut l.name, prefixes);
    }
    for f in &mut module.functions {
        rename_in_place(&mut f.name, prefixes);
    }
    let mut visit = |expr: &mut IrExpr| rename_expr(expr, prefixes);
    for_each_body(module, &mut visit);
    module.rebuild_indices();
}

fn rename_expr(expr: &mut IrExpr, prefixes: &HashMap<String, String>) {
    match expr {
        IrExpr::FunctionCall { path, .. } | IrExpr::Reference { path, .. } => {
            rename_path(path, prefixes);
        }
        IrExpr::ClosureRef { funcref, .. } => rename_path(funcref, prefixes),
        IrExpr::LetRef { name, .. } => rename_in_place(name, prefixes),
        IrExpr::Closure { captures, .. } => {
            for (_, name, _, _) in captures {
                rename_in_place(name, prefixes);
            }
        }
        IrExpr::Literal { .. }
        | IrExpr::StructInst { .. }
        | IrExpr::EnumInst { .. }
        | IrExpr::Array { .. }
        | IrExpr::Tuple { .. }
        | IrExpr::SelfFieldRef { .. }
        | IrExpr::FieldAccess { .. }
        | IrExpr::BinaryOp { .. }
        | IrExpr::UnaryOp { .. }
        | IrExpr::If { .. }
        | IrExpr::For { .. }
        | IrExpr::Match { .. }
        | IrExpr::CallClosure { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::DictLiteral { .. }
        | IrExpr::DictAccess { .. }
        | IrExpr::Block { .. } => {}
    }
    walk_expr_children_mut(expr, &mut |child| rename_expr(child, prefixes));
}

/// Call `visit` on each top expression of the module: each body, each
/// default value and each `let` value.
fn for_each_body(module: &mut IrModule, visit: &mut impl FnMut(&mut IrExpr)) {
    let function = |f: &mut IrFunction, visit: &mut dyn FnMut(&mut IrExpr)| {
        for param in &mut f.params {
            if let Some(default) = &mut param.default {
                visit(default);
            }
        }
        if let Some(body) = &mut f.body {
            visit(body);
        }
    };
    for f in &mut module.functions {
        function(f, visit);
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            function(f, visit);
        }
    }
    for s in &mut module.structs {
        for field in &mut s.fields {
            if let Some(default) = &mut field.default {
                visit(default);
            }
        }
    }
    for e in &mut module.enums {
        for variant in &mut e.variants {
            for field in &mut variant.fields {
                if let Some(default) = &mut field.default {
                    visit(default);
                }
            }
        }
    }
    for t in &mut module.traits {
        for field in &mut t.fields {
            if let Some(default) = &mut field.default {
                visit(default);
            }
        }
        for sig in &mut t.methods {
            for param in &mut sig.params {
                if let Some(default) = &mut param.default {
                    visit(default);
                }
            }
        }
    }
    for l in &mut module.lets {
        visit(&mut l.value);
    }
}
