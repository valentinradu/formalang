use super::ParamInfo;
use crate::ast::Type;

/// Normalised, span-free signature string for overload deduplication.
///
/// Includes the call-site label (external label if present, else the internal
/// name) so that overloads distinguished purely by label are not treated as
/// duplicates, matching the overload resolution rules.
pub(super) fn param_signature(params: &[ParamInfo]) -> String {
    let mut out = String::new();
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let label = p
            .external_label
            .as_ref()
            .map_or(p.name.name.as_str(), |l| l.name.as_str());
        out.push_str(label);
        out.push(':');
        match &p.ty {
            Some(t) => out.push_str(&ty_shape(t)),
            None => out.push('_'),
        }
    }
    out
}

fn ty_shape(ty: &Type) -> String {
    match ty {
        Type::Primitive(p) => format!("{p:?}"),
        Type::Ident(i) => i.name.clone(),
        Type::Generic { name, args, .. } => {
            let parts: Vec<String> = args.iter().map(ty_shape).collect();
            format!("{}<{}>", name.name, parts.join(","))
        }
        Type::Array(inner) => format!("[{}]", ty_shape(inner)),
        Type::Optional(inner) => format!("{}?", ty_shape(inner)),
        Type::Tuple(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| format!("{}:{}", f.name.name, ty_shape(&f.ty)))
                .collect();
            format!("({})", parts.join(","))
        }
        Type::Dictionary { key, value } => {
            format!("[{}:{}]", ty_shape(key), ty_shape(value))
        }
        Type::Closure { params, ret } => {
            let parts: Vec<String> = params
                .iter()
                .map(|(c, t)| format!("{c:?}_{}", ty_shape(t)))
                .collect();
            format!("({})->{}", parts.join(","), ty_shape(ret))
        }
    }
}
