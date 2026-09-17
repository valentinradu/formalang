//! Keeping a call's `function_id` in step with its path.
//!
//! Split out of `functions.rs` to keep each file under the line ceiling
//! that `scripts/check_file_sizes.sh` enforces.

use std::collections::HashMap;

use crate::ir::{IrExpr, IrModule};

use super::expr_walk::iter_expr_children_mut;

/// Make every call's `function_id` agree with its path.
///
/// Specialisation renames a call's path to the specialised function,
/// and compaction then drops the generic originals and renumbers what
/// is left. Neither step touches `function_id`, so a call could name
/// `identity__I32` in its path while its id still pointed at whatever
/// now sits at the old index — `probe` itself, in the case that found
/// this. A backend dispatching on the id, which is what the field is
/// for, emitted a function calling itself.
///
/// Run after `rebuild_indices`, when the name table is accurate.
pub(in crate::ir::monomorphise) fn resync_call_ids(module: &mut IrModule) {
    // Every function with a given name, not just one of them. A name
    // can belong to several overloads, and collapsing them into one
    // entry would re-point every call to whichever overload came last.
    let mut by_name: HashMap<String, Vec<crate::ir::FunctionId>> = HashMap::new();
    for (index, f) in module.functions.iter().enumerate() {
        if let Ok(i) = u32::try_from(index) {
            by_name
                .entry(f.name.clone())
                .or_default()
                .push(crate::ir::FunctionId(i));
        }
    }
    let names: Vec<String> = module.functions.iter().map(|f| f.name.clone()).collect();

    let fix = |expr: &mut IrExpr| resync_expr(expr, &by_name, &names);

    for f in &mut module.functions {
        if let Some(body) = &mut f.body {
            fix(body);
        }
        for param in &mut f.params {
            if let Some(default) = &mut param.default {
                fix(default);
            }
        }
    }
    for imp in &mut module.impls {
        for f in &mut imp.functions {
            if let Some(body) = &mut f.body {
                fix(body);
            }
            for param in &mut f.params {
                if let Some(default) = &mut param.default {
                    fix(default);
                }
            }
        }
    }
    for l in &mut module.lets {
        fix(&mut l.value);
    }
}

fn resync_expr(
    expr: &mut IrExpr,
    by_name: &HashMap<String, Vec<crate::ir::FunctionId>>,
    names: &[String],
) {
    for child in iter_expr_children_mut(expr) {
        resync_expr(child, by_name, names);
    }
    let IrExpr::FunctionCall {
        path, function_id, ..
    } = expr
    else {
        return;
    };

    // A module-qualified call is registered under its joined name; a
    // single-segment one under the bare name. Try both.
    let joined = path.join("::");
    let Some(candidates) = by_name
        .get(&joined)
        .or_else(|| path.last().and_then(|last| by_name.get(last)))
    else {
        return;
    };

    // The id is only wrong when it no longer names what the path says.
    // Leaving a correct id alone is what keeps overload resolution
    // intact: the lowerer already chose which overload this call means,
    // and specialisation did not rename it.
    if let Some(current) = function_id {
        if names
            .get(current.0 as usize)
            .is_some_and(|name| *name == joined || Some(name) == path.last())
        {
            return;
        }
    }

    // The id is stale. Repair it only when the name picks out one
    // function: with several overloads there is nothing here to say
    // which was meant, and a guess would be worse than leaving the
    // call for `ResolveReferencesPass` to rebind by path.
    if let [only] = candidates.as_slice() {
        *function_id = Some(*only);
    }
}
