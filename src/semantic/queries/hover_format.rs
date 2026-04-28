//! Hover-signature formatters that convert symbol-table entries into
//! `HoverInfo` values. Kept separate from `QueryProvider` because the
//! formatting is bulky and call-site logic is uniform across kinds.

use super::{HoverInfo, SymbolKind};
use crate::ast::Visibility;
use crate::semantic::symbol_table::{EnumInfo, FunctionInfo, LetInfo, StructInfo, TraitInfo};

pub(super) fn trait_info_to_hover(name: &str, info: &TraitInfo, kind: SymbolKind) -> HoverInfo {
    HoverInfo {
        symbol_name: name.to_string(),
        kind,
        signature: format!(
            "{}trait {name}{}",
            vis_prefix(info.visibility),
            format_generics(&info.generics)
        ),
        documentation: info.doc.clone(),
        source_span: info.span,
    }
}

pub(super) fn struct_info_to_hover(name: &str, info: &StructInfo) -> HoverInfo {
    HoverInfo {
        symbol_name: name.to_string(),
        kind: SymbolKind::Struct,
        signature: format!(
            "{}struct {name}{}",
            vis_prefix(info.visibility),
            format_generics(&info.generics)
        ),
        documentation: info.doc.clone(),
        source_span: info.span,
    }
}

pub(super) fn enum_info_to_hover(name: &str, info: &EnumInfo) -> HoverInfo {
    HoverInfo {
        symbol_name: name.to_string(),
        kind: SymbolKind::Enum,
        signature: format!(
            "{}enum {name}{}",
            vis_prefix(info.visibility),
            format_generics(&info.generics)
        ),
        documentation: info.doc.clone(),
        source_span: info.span,
    }
}

/// Build a hover signature for a function. Overloaded names show only the
/// first overload — full overload resolution is a future LSP enhancement.
pub(super) fn function_info_to_hover(name: &str, info: &FunctionInfo) -> HoverInfo {
    let params = info
        .params
        .iter()
        .map(|p| {
            let label = p
                .external_label
                .as_ref()
                .map(|l| format!("{} ", l.name))
                .unwrap_or_default();
            let ty = p.ty.as_ref().map_or_else(String::new, format_type_brief);
            if ty.is_empty() {
                format!("{label}{}", p.name.name)
            } else {
                format!("{label}{}: {ty}", p.name.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret = info
        .return_type
        .as_ref()
        .map(|t| format!(" -> {}", format_type_brief(t)))
        .unwrap_or_default();

    HoverInfo {
        symbol_name: name.to_string(),
        kind: SymbolKind::Function,
        signature: format!(
            "{}fn {name}{}({params}){ret}",
            vis_prefix(info.visibility),
            format_generics(&info.generics)
        ),
        documentation: info.doc.clone(),
        source_span: info.span,
    }
}

pub(super) fn let_info_to_hover(name: &str, info: &LetInfo) -> HoverInfo {
    let vis = vis_prefix(info.visibility);
    let signature = info.inferred_type.as_ref().map_or_else(
        || format!("{vis}let {name}"),
        |ty| format!("{vis}let {name}: {ty}"),
    );

    HoverInfo {
        symbol_name: name.to_string(),
        kind: SymbolKind::Let,
        signature,
        documentation: info.doc.clone(),
        source_span: info.span,
    }
}

const fn vis_prefix(visibility: Visibility) -> &'static str {
    if matches!(visibility, Visibility::Public) {
        "pub "
    } else {
        ""
    }
}

fn format_generics(generics: &[crate::ast::GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
    let names: Vec<String> = generics.iter().map(|g| g.name.name.clone()).collect();
    format!("<{}>", names.join(", "))
}

/// Minimal type formatter for hover signatures. Mirrors the analyser's
/// `type_to_string` but works without a full analyser instance.
fn format_type_brief(ty: &crate::ast::Type) -> String {
    use crate::ast::{PrimitiveType, Type};
    match ty {
        Type::Primitive(p) => match p {
            PrimitiveType::String => "String".to_string(),
            PrimitiveType::I32 => "I32".to_string(),
            PrimitiveType::I64 => "I64".to_string(),
            PrimitiveType::F32 => "F32".to_string(),
            PrimitiveType::F64 => "F64".to_string(),
            PrimitiveType::Boolean => "Boolean".to_string(),
            PrimitiveType::Path => "Path".to_string(),
            PrimitiveType::Regex => "Regex".to_string(),
            PrimitiveType::Never => "Never".to_string(),
        },
        Type::Ident(ident) => ident.name.clone(),
        Type::Array(inner) => format!("[{}]", format_type_brief(inner)),
        Type::Optional(inner) => format!("{}?", format_type_brief(inner)),
        Type::Tuple(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name.name, format_type_brief(&f.ty)))
                .collect();
            format!("({})", parts.join(", "))
        }
        Type::Generic { name, args, .. } => {
            if args.is_empty() {
                name.name.clone()
            } else {
                let arg_strs: Vec<String> = args.iter().map(format_type_brief).collect();
                format!("{}<{}>", name.name, arg_strs.join(", "))
            }
        }
        Type::Dictionary { key, value } => {
            format!("[{}: {}]", format_type_brief(key), format_type_brief(value))
        }
        Type::Closure { params, ret } => {
            let parts: Vec<String> = params.iter().map(|(_, p)| format_type_brief(p)).collect();
            if parts.is_empty() {
                format!("() -> {}", format_type_brief(ret))
            } else {
                format!("{} -> {}", parts.join(", "), format_type_brief(ret))
            }
        }
    }
}
