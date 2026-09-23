//! Walking a written type, and asking what names it mentions.
//!
//! Several checks need to know whether a declared type names one of a
//! definition's generic parameters. Testing the rendered string for the
//! parameter as a substring is wrong: `String` contains `S`, `I64`
//! contains `I`, `Vec<T>` contains `V`. A function declared
//! `fn pick<S>(item: S, label: String)` then skipped the argument check
//! on `label`, and `pick(item: 1, label: 99)` compiled.
//!
//! So the test walks the type and compares whole names.

use crate::ast::Type;

/// Call `visit` with every type name written inside `ty`.
pub(in crate::semantic) fn for_each_named_type(ty: &Type, visit: &mut impl FnMut(&str)) {
    match ty {
        Type::Ident(ident) => visit(&ident.name),
        Type::Array(inner) | Type::Optional(inner) => for_each_named_type(inner, visit),
        Type::Tuple(fields) => {
            for field in fields {
                for_each_named_type(&field.ty, visit);
            }
        }
        Type::Generic { name, args, .. } => {
            visit(&name.name);
            for arg in args {
                for_each_named_type(arg, visit);
            }
        }
        Type::Dictionary { key, value } => {
            for_each_named_type(key, visit);
            for_each_named_type(value, visit);
        }
        Type::Closure { params, ret } => {
            for (_, param) in params {
                for_each_named_type(param, visit);
            }
            for_each_named_type(ret, visit);
        }
        Type::Primitive(_) => {}
    }
}

/// Visit each name that stands as the key of a dictionary type inside
/// `ty`: `K` in `[K: V]` and in `Dictionary<K, V>`.
pub(in crate::semantic) fn for_each_key_name(ty: &Type, visit: &mut impl FnMut(&str)) {
    match ty {
        Type::Dictionary { key, value } => {
            if let Type::Ident(ident) = &**key {
                visit(&ident.name);
            }
            for_each_key_name(key, visit);
            for_each_key_name(value, visit);
        }
        Type::Generic { name, args, .. } => {
            if name.name == "Dictionary" {
                if let Some(Type::Ident(ident)) = args.first() {
                    visit(&ident.name);
                }
            }
            for arg in args {
                for_each_key_name(arg, visit);
            }
        }
        Type::Array(inner) | Type::Optional(inner) => for_each_key_name(inner, visit),
        Type::Tuple(fields) => {
            for field in fields {
                for_each_key_name(&field.ty, visit);
            }
        }
        Type::Closure { params, ret } => {
            for (_, param) in params {
                for_each_key_name(param, visit);
            }
            for_each_key_name(ret, visit);
        }
        Type::Ident(_) | Type::Primitive(_) => {}
    }
}

/// Whether `ty` names any of `names`, as a whole name rather than as a
/// substring of a longer one.
pub(in crate::semantic) fn type_mentions_any(ty: &Type, names: &[String]) -> bool {
    let mut found = false;
    for_each_named_type(ty, &mut |name| {
        if names.iter().any(|candidate| candidate == name) {
            found = true;
        }
    });
    found
}
