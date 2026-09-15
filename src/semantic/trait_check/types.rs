use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{PrimitiveType, Type};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Replace any `Type::Ident(name)` whose name is a key in
    /// `subs` with the corresponding concrete type, recursively. Used
    /// by the trait-method check to substitute trait generic params
    /// with the impl's `trait_args` before comparing signatures.
    pub(in crate::semantic) fn substitute_type_params(
        ty: &mut Type,
        subs: &std::collections::HashMap<String, Type>,
    ) {
        match ty {
            Type::Ident(ident) => {
                if let Some(concrete) = subs.get(&ident.name) {
                    *ty = concrete.clone();
                }
            }
            Type::Array(inner) | Type::Optional(inner) => {
                Self::substitute_type_params(inner, subs);
            }
            Type::Tuple(fields) => {
                for f in fields {
                    Self::substitute_type_params(&mut f.ty, subs);
                }
            }
            Type::Generic { args, .. } => {
                for a in args {
                    Self::substitute_type_params(a, subs);
                }
            }
            Type::Dictionary { key, value } => {
                Self::substitute_type_params(key, subs);
                Self::substitute_type_params(value, subs);
            }
            Type::Closure { params, ret } => {
                for (_, p) in params {
                    Self::substitute_type_params(p, subs);
                }
                Self::substitute_type_params(ret, subs);
            }
            Type::Primitive(_) => {}
        }
    }

    /// Check if two types match (structural equality)
    pub(in crate::semantic) fn types_match(ty1: &Type, ty2: &Type) -> bool {
        match (ty1, ty2) {
            (Type::Primitive(p1), Type::Primitive(p2)) => p1 == p2,
            (Type::Ident(i1), Type::Ident(i2)) => i1.name == i2.name,
            (Type::Array(elem1), Type::Array(elem2)) => Self::types_match(elem1, elem2),
            (Type::Optional(inner1), Type::Optional(inner2)) => Self::types_match(inner1, inner2),
            (
                Type::Generic {
                    name: n1, args: a1, ..
                },
                Type::Generic {
                    name: n2, args: a2, ..
                },
            ) => {
                // Generic types match if they have the same base type and matching arguments
                n1.name == n2.name
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2.iter())
                        .all(|(t1, t2)| Self::types_match(t1, t2))
            }
            (Type::Dictionary { key: k1, value: v1 }, Type::Dictionary { key: k2, value: v2 }) => {
                Self::types_match(k1, k2) && Self::types_match(v1, v2)
            }
            (
                Type::Closure {
                    params: p1,
                    ret: r1,
                },
                Type::Closure {
                    params: p2,
                    ret: r2,
                },
            ) => {
                p1.len() == p2.len()
                    && p1
                        .iter()
                        .zip(p2.iter())
                        .all(|((c1, a), (c2, b))| c1 == c2 && Self::types_match(a, b))
                    && Self::types_match(r1, r2)
            }
            _ => false,
        }
    }

    /// Convert a type to a string for error messages
    pub(in crate::semantic) fn type_to_string(ty: &Type) -> String {
        match ty {
            Type::Primitive(prim) => match prim {
                PrimitiveType::String => "String".to_string(),
                PrimitiveType::I32 => "I32".to_string(),
                PrimitiveType::I64 => "I64".to_string(),
                PrimitiveType::F32 => "F32".to_string(),
                PrimitiveType::F64 => "F64".to_string(),
                PrimitiveType::Boolean => "Boolean".to_string(),
                PrimitiveType::Never => "Never".to_string(),
            },
            Type::Ident(ident) => ident.name.clone(),
            Type::Array(element_type) => {
                format!("[{}]", Self::type_to_string(element_type))
            }
            Type::Optional(inner_type) => {
                format!("{}?", Self::type_to_string(inner_type))
            }
            Type::Tuple(fields) => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name.name, Self::type_to_string(&f.ty)))
                    .collect();
                format!("({})", field_types.join(", "))
            }
            Type::Generic { name, args, .. } => {
                if args.is_empty() {
                    name.name.clone()
                } else {
                    let arg_types: Vec<String> =
                        args.iter().map(|arg| Self::type_to_string(arg)).collect();
                    format!("{}<{}>", name.name, arg_types.join(", "))
                }
            }
            Type::Dictionary { key, value } => {
                format!(
                    "[{}: {}]",
                    Self::type_to_string(key),
                    Self::type_to_string(value)
                )
            }
            Type::Closure { params, ret } => {
                let param_types: Vec<String> = params
                    .iter()
                    .map(|(_, p)| Self::type_to_string(p))
                    .collect();
                format!(
                    "({}) -> {}",
                    param_types.join(", "),
                    Self::type_to_string(ret)
                )
            }
        }
    }
}
