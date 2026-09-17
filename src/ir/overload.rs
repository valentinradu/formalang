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
