//! Build the tag enum and the dispatch function for each closure shape.

use std::collections::HashMap;

use crate::ast::{ParamConvention, Visibility};
use crate::error::CompilerError;
use crate::ir::{
    BindingId, EnumId, IrEnum, IrEnumVariant, IrExpr, IrField, IrFunction, IrFunctionParam,
    IrMatchArm, IrModule, IrSpan, ResolvedType, VariantIdx,
};

use super::collect::Shape;
use super::{CALL_FN_PREFIX, ENV_FIELD_NAME, FN_ENUM_PREFIX};

/// What the rewrite needs to know about one closure shape.
pub(super) struct ShapePlan {
    /// The enum that replaces the closure type.
    pub(super) enum_id: EnumId,
    /// Enum type, ready to drop into a `ty` slot.
    pub(super) enum_ty: ResolvedType,
    /// Name of the dispatch function for this shape.
    pub(super) call_fn: String,
    /// Lifted-function name to its variant name and index.
    pub(super) variants: HashMap<String, (String, VariantIdx)>,
}

/// Everything the rewrite needs, keyed by closure type.
pub(super) struct Plan {
    pub(super) shapes: Vec<(ResolvedType, ShapePlan)>,
}

impl Plan {
    /// The plan for `ty`, when `ty` is a closure type this pass built
    /// an enum for.
    pub(super) fn for_type(&self, ty: &ResolvedType) -> Option<&ShapePlan> {
        self.shapes.iter().find(|(t, _)| t == ty).map(|(_, p)| p)
    }
}

/// Add one enum and one dispatch function per shape.
pub(super) fn synthesise(
    module: &mut IrModule,
    shapes: Vec<Shape>,
) -> Result<Plan, Vec<CompilerError>> {
    let mut planned = Vec::new();
    for (index, shape) in shapes.into_iter().enumerate() {
        let enum_name = format!("{FN_ENUM_PREFIX}{index}");
        let call_fn = format!("{CALL_FN_PREFIX}{index}");

        let mut variants = HashMap::new();
        let mut ir_variants = Vec::new();
        for (position, target) in shape.targets.iter().enumerate() {
            // The lifted function's own name is unique per closure
            // site, so it names the variant too. A backend reading the
            // IR can tie the two together without a side table.
            let variant_name = target.funcref.clone();
            let fields = target.env_ty.as_ref().map_or_else(Vec::new, |env_ty| {
                vec![IrField {
                    name: ENV_FIELD_NAME.to_string(),
                    ty: env_ty.clone(),
                    mutable: false,
                    optional: false,
                    convention: ParamConvention::Let,
                    default: None,
                    doc: None,
                    span: IrSpan::default(),
                }]
            });
            let idx = u32::try_from(position).map_err(|_| {
                vec![CompilerError::TooManyDefinitions {
                    kind: "enum variant",
                    span: crate::location::Span::default(),
                }]
            })?;
            variants.insert(
                target.funcref.clone(),
                (variant_name.clone(), VariantIdx(idx)),
            );
            ir_variants.push(IrEnumVariant {
                name: variant_name,
                fields,
                span: IrSpan::default(),
            });
        }

        let enum_def = IrEnum {
            name: enum_name.clone(),
            visibility: Visibility::Private,
            variants: ir_variants,
            generic_params: Vec::new(),
            doc: None,
            span: IrSpan::default(),
        };
        let enum_id = module.add_enum(enum_name, enum_def).map_err(|e| vec![e])?;
        let enum_ty = ResolvedType::Enum(enum_id);

        let dispatch = build_dispatch(&call_fn, &shape, enum_id, &enum_ty);
        module
            .add_function(call_fn.clone(), dispatch)
            .map_err(|e| vec![e])?;

        planned.push((
            shape.closure_ty,
            ShapePlan {
                enum_id,
                enum_ty,
                call_fn,
                variants,
            },
        ));
    }
    Ok(Plan { shapes: planned })
}

/// Build `fn __call_Fn<K>(f, p0, p1, ...) -> R` as a `match` over the
/// tag, one direct call per arm.
fn build_dispatch(
    call_fn: &str,
    shape: &Shape,
    enum_id: EnumId,
    enum_ty: &ResolvedType,
) -> IrFunction {
    let ResolvedType::Closure {
        param_tys,
        return_ty,
    } = &shape.closure_ty
    else {
        // `Shape::closure_ty` comes from a `ClosureRef`'s `ty`, which
        // is a closure type by construction.
        return placeholder_dispatch(call_fn);
    };

    let mut params = vec![IrFunctionParam {
        binding_id: BindingId(0),
        name: "f".to_string(),
        external_label: None,
        ty: Some(enum_ty.clone()),
        default: None,
        // The tag is consumed by the dispatch: it names one call and
        // that call happens once.
        convention: ParamConvention::Sink,
        span: IrSpan::default(),
    }];
    for (position, (convention, ty)) in param_tys.iter().enumerate() {
        params.push(IrFunctionParam {
            binding_id: BindingId(0),
            name: format!("p{position}"),
            external_label: None,
            ty: Some(ty.clone()),
            default: None,
            convention: *convention,
            span: IrSpan::default(),
        });
    }

    let arms = shape
        .targets
        .iter()
        .enumerate()
        .map(|(position, target)| {
            let idx = u32::try_from(position).unwrap_or(u32::MAX);
            let bindings = target.env_ty.as_ref().map_or_else(Vec::new, |env_ty| {
                vec![(ENV_FIELD_NAME.to_string(), BindingId(0), env_ty.clone())]
            });
            // The lifted function takes its env first, then the
            // closure's own parameters in order.
            let mut args: Vec<(Option<String>, IrExpr)> = Vec::new();
            if let Some(env_ty) = &target.env_ty {
                args.push((
                    Some("__env".to_string()),
                    IrExpr::Reference {
                        path: vec![ENV_FIELD_NAME.to_string()],
                        target: crate::ir::ReferenceTarget::Unresolved,
                        ty: env_ty.clone(),
                        span: IrSpan::default(),
                    },
                ));
            }
            for (position, (_, ty)) in param_tys.iter().enumerate() {
                let name = format!("p{position}");
                args.push((
                    Some(name.clone()),
                    IrExpr::Reference {
                        path: vec![name],
                        target: crate::ir::ReferenceTarget::Unresolved,
                        ty: ty.clone(),
                        span: IrSpan::default(),
                    },
                ));
            }
            IrMatchArm {
                variant: target.funcref.clone(),
                variant_idx: VariantIdx(idx),
                is_wildcard: false,
                bindings,
                body: IrExpr::FunctionCall {
                    path: vec![target.funcref.clone()],
                    function_id: None,
                    args,
                    ty: (**return_ty).clone(),
                    span: IrSpan::default(),
                },
            }
        })
        .collect();

    let body = IrExpr::Match {
        scrutinee: Box::new(IrExpr::Reference {
            path: vec!["f".to_string()],
            target: crate::ir::ReferenceTarget::Unresolved,
            ty: ResolvedType::Enum(enum_id),
            span: IrSpan::default(),
        }),
        arms,
        ty: (**return_ty).clone(),
        span: IrSpan::default(),
    };

    IrFunction {
        name: call_fn.to_string(),
        visibility: Visibility::Private,
        generic_params: Vec::new(),
        params,
        return_type: Some((**return_ty).clone()),
        body: Some(body),
        extern_abi: None,
        attributes: Vec::new(),
        doc: None,
        span: IrSpan::default(),
    }
}

/// Unreachable in practice; keeps `build_dispatch` total without an
/// `unwrap`.
fn placeholder_dispatch(call_fn: &str) -> IrFunction {
    IrFunction {
        name: call_fn.to_string(),
        visibility: Visibility::Private,
        generic_params: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: None,
        extern_abi: None,
        attributes: Vec::new(),
        doc: None,
        span: IrSpan::default(),
    }
}
