//! Synthesis of the capture-environment [`IrStruct`] that pairs with
//! every lifted closure function.

use crate::ast::{ParamConvention, Visibility};
use crate::ir::{IrField, IrStruct, ResolvedType};

/// Build the capture-environment struct for a closure with the given
/// captures. Each capture becomes a private field carrying the
/// captured value's type and the capture's [`ParamConvention`]. The
/// convention drives `mutable` (`true` for `Mut`, otherwise `false`)
/// so backends without convention awareness still get the right
/// mutability hint, while convention-aware backends can read
/// `field.convention` directly to distinguish `Let` (copy / borrow),
/// `Mut` (caller-frame reference), and `Sink` (move ownership).
pub(super) fn synthesize_env_struct(
    name: String,
    captures: &[(String, ParamConvention, ResolvedType)],
) -> IrStruct {
    let fields = captures
        .iter()
        .map(|(field_name, convention, ty)| IrField {
            name: field_name.clone(),
            ty: ty.clone(),
            mutable: matches!(convention, ParamConvention::Mut),
            optional: false,
            default: None,
            doc: None,
            convention: *convention,
        })
        .collect();

    IrStruct {
        name,
        visibility: Visibility::Private,
        traits: Vec::new(),
        fields,
        generic_params: Vec::new(),
        doc: Some(
            "Auto-generated capture environment for a lifted closure. Produced by `ClosureConversionPass`."
                .to_string(),
        ),
    }
}
