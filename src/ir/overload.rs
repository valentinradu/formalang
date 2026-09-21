//! Choosing between definitions that share a name.
//!
//! A free function overloads by the labels and the count of the
//! arguments a call gives it, and so does a method. The rule is the
//! same for both, so it lives here rather than twice.
//!
//! A method needed it later than a free function did. A call to a free
//! function carries a `function_id` that says which definition it
//! means, resolved at lowering time. A method call carries a
//! [`MethodIdx`](crate::ir::MethodIdx) instead — the position of the
//! method inside its impl block or trait — and that index was filled
//! in by taking the first method of the right *name*. So a type
//! declaring two methods of one name had its second one silently
//! unreachable: the call compiled, and answered from the first.

use crate::ir::IrFunctionParam;

/// Whether a call with these labels and this many arguments fits a
/// definition with these parameters.
///
/// `self` is not counted: a method call supplies its receiver
/// separately.
#[must_use]
pub(crate) fn call_fits(
    params: &[IrFunctionParam],
    arg_labels: &[Option<String>],
    arg_count: usize,
) -> bool {
    let params: Vec<&IrFunctionParam> = params.iter().filter(|p| p.name != "self").collect();

    // Every label the call gives has to name a parameter.
    let labels_fit = arg_labels.iter().flatten().all(|label| {
        params
            .iter()
            .any(|p| p.name == *label || p.external_label.as_ref() == Some(label))
    });
    if !labels_fit {
        return false;
    }

    // A parameter with a default may be left out, so the count has to
    // land between what is required and what is declared.
    let required = params.iter().filter(|p| p.default.is_none()).count();
    arg_count >= required && arg_count <= params.len()
}

/// How many defaults a call would fire against these parameters.
///
/// The measure that settles a tie: of the definitions a call fits, the
/// one that leaves fewest parameters to their defaults is the one it
/// means. `fn add(n: I32)` and `fn add(n: I32, m: I32 = 0)` both fit
/// `add(n: 1)`, and the first is meant.
#[must_use]
fn defaults_fired(params: &[IrFunctionParam], arg_count: usize) -> usize {
    let declared = params.iter().filter(|p| p.name != "self").count();
    declared.saturating_sub(arg_count)
}

/// Pick the definition a call means, from several sharing a name.
///
/// Returns the index into `candidates`. `None` when nothing fits,
/// which leaves the caller to report the call rather than guess.
#[must_use]
pub(crate) fn choose<'a, T>(
    candidates: impl Iterator<Item = (usize, &'a T)>,
    params_of: impl Fn(&'a T) -> &'a [IrFunctionParam],
    arg_labels: &[Option<String>],
    arg_count: usize,
) -> Option<usize>
where
    T: 'a,
{
    let mut best: Option<(usize, usize)> = None;
    for (index, candidate) in candidates {
        let params = params_of(candidate);
        if !call_fits(params, arg_labels, arg_count) {
            continue;
        }
        let fired = defaults_fired(params, arg_count);
        if best.is_none_or(|(fewest, _)| fired < fewest) {
            best = Some((fired, index));
        }
    }
    best.map(|(_, index)| index)
}

/// The index of the method a call means, inside the impl block or the
/// trait that declares it.
///
/// [`choose`] decides between the methods of that name. When none of
/// them fits the call, the index goes to the first method of that name
/// instead: the call still points at something a diagnostic can name,
/// rather than at whatever method sits at index zero.
///
/// The semantic analyser rejects a call that fits no method before
/// lowering runs, so a module built by hand is the only way to reach
/// that fallback. `ResolveReferencesPass` reads such modules.
///
/// Two places computed this index, and each held its own copy of the
/// rule: lowering writes it, and `ResolveReferencesPass` writes it
/// again over a module it did not lower. One copy is enough.
#[must_use]
pub(crate) fn method_index<'a, T>(
    methods: &'a [T],
    name_of: impl Fn(&'a T) -> &'a str,
    params_of: impl Fn(&'a T) -> &'a [IrFunctionParam],
    method_name: &str,
    arg_labels: &[Option<String>],
    arg_count: usize,
) -> Option<usize> {
    let named = methods
        .iter()
        .enumerate()
        .filter(|&(_, m)| name_of(m) == method_name);
    choose(named, params_of, arg_labels, arg_count)
        .or_else(|| methods.iter().position(|m| name_of(m) == method_name))
}

#[cfg(test)]
mod tests {
    use super::{method_index, IrFunctionParam};
    use crate::ir::BindingId;

    /// A stand-in for a method: a name and its parameters. The rule
    /// under test is generic over the method type, so the test does
    /// not have to build a whole `IrFunction`.
    struct Method {
        name: &'static str,
        params: Vec<IrFunctionParam>,
    }

    fn param(name: &str, default: bool) -> IrFunctionParam {
        IrFunctionParam {
            binding_id: BindingId(0),
            name: name.to_string(),
            external_label: None,
            ty: None,
            default: default.then(|| crate::ir::IrExpr::Literal {
                value: crate::ast::Literal::Nil,
                ty: crate::ir::ResolvedType::Primitive(crate::ast::PrimitiveType::I32),
                span: crate::ir::IrSpan::default(),
            }),
            convention: crate::ast::ParamConvention::Let,
            span: crate::ir::IrSpan::default(),
        }
    }

    fn methods() -> Vec<Method> {
        vec![
            Method {
                name: "zero",
                params: vec![param("self", false)],
            },
            Method {
                name: "at",
                params: vec![param("self", false), param("row", false)],
            },
            Method {
                name: "at",
                params: vec![
                    param("self", false),
                    param("row", false),
                    param("col", false),
                ],
            },
        ]
    }

    /// The index that a call of this name, these labels and this many
    /// arguments takes.
    fn index_of(method_name: &str, labels: &[&str], arg_count: usize) -> Option<usize> {
        let all = methods();
        let labels: Vec<Option<String>> = labels.iter().map(|l| Some((*l).to_string())).collect();
        method_index(
            &all,
            |m| m.name,
            |m| m.params.as_slice(),
            method_name,
            &labels,
            arg_count,
        )
    }

    #[test]
    fn a_call_takes_the_overload_it_fits() {
        assert_eq!(index_of("at", &["row"], 1), Some(1));
        assert_eq!(index_of("at", &["row", "col"], 2), Some(2));
    }

    /// The fallback. No method of that name fits the call, so the
    /// index goes to the first method of that name — index 1 — and not
    /// to index 0, which holds a method of another name.
    #[test]
    fn a_call_that_fits_no_overload_takes_the_first_of_its_name() {
        assert_eq!(index_of("at", &[], 0), Some(1));
        assert_eq!(index_of("at", &["row"], 3), Some(1));
        assert_eq!(index_of("at", &["nosuch"], 1), Some(1));
    }

    #[test]
    fn a_name_no_method_carries_has_no_index() {
        assert_eq!(index_of("missing", &[], 0), None);
    }
}
