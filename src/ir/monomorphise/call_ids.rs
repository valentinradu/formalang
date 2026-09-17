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

    let fix = |expr: &mut IrExpr| resync_expr(expr, &by_name);

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

fn resync_expr(expr: &mut IrExpr, by_name: &HashMap<String, Vec<crate::ir::FunctionId>>) {
    for child in iter_expr_children_mut(expr) {
        resync_expr(child, by_name);
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

    // Repair the id only when the name picks out one function.
    //
    // That single condition is also what keeps overload resolution
    // intact. Where a name has several candidates there is nothing
    // here to say which the call meant, so the id the lowerer already
    // chose stands; where it has one, that one is the answer whether
    // or not the id was already correct.
    //
    // An earlier version checked first whether the id was already
    // right and returned early if so. Mutation testing showed that
    // check could be inverted without any test noticing, and the
    // reason is that it never decided anything: every path through it
    // reaches the same id as the line below. It was removed rather
    // than covered.
    if let [only] = candidates.as_slice() {
        *function_id = Some(*only);
    }
}
