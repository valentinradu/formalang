//! Span-filling for definitions (trait, struct, impl, enum, function, module)
//! and type expressions.

use super::{fill_expr_span, fill_span};
use crate::ast::{AttributeAnnotation, Definition, GenericConstraint, Type};

pub(super) fn fill_definition_span(def: &mut Definition, source: &str) {
    match def {
        Definition::Module(m) => {
            fill_span(&mut m.name.span, source);
            for def in &mut m.definitions {
                fill_definition_span(def, source);
            }
            fill_span(&mut m.span, source);
        }
        Definition::Trait(t) => fill_trait_def_spans(t, source),
        Definition::Struct(s) => fill_struct_def_spans(s, source),
        Definition::Impl(i) => fill_impl_def_spans(i, source),
        Definition::Enum(e) => fill_enum_def_spans(e, source),
        Definition::Function(f) => fill_function_def_spans(f, source),
    }
}

fn fill_attributes_spans(attributes: &mut [AttributeAnnotation], source: &str) {
    for attr in attributes {
        fill_span(&mut attr.span, source);
    }
}

fn fill_generic_params_spans(params: &mut [crate::ast::GenericParam], source: &str) {
    for param in params {
        fill_span(&mut param.name.span, source);
        for constraint in &mut param.constraints {
            match constraint {
                GenericConstraint::Trait { name, args } => {
                    fill_span(&mut name.span, source);
                    for arg in args {
                        fill_type_span(arg, source);
                    }
                }
            }
        }
        fill_span(&mut param.span, source);
    }
}

fn fill_trait_def_spans(t: &mut crate::ast::TraitDef, source: &str) {
    fill_span(&mut t.name.span, source);
    for base in &mut t.traits {
        fill_span(&mut base.span, source);
    }
    fill_generic_params_spans(&mut t.generics, source);
    for field in &mut t.fields {
        fill_span(&mut field.name.span, source);
        fill_type_span(&mut field.ty, source);
        fill_span(&mut field.span, source);
    }
    for m in &mut t.methods {
        fill_span(&mut m.name.span, source);
        fill_attributes_spans(&mut m.attributes, source);
        fill_span(&mut m.span, source);
    }
    fill_span(&mut t.span, source);
}

fn fill_struct_def_spans(s: &mut crate::ast::StructDef, source: &str) {
    fill_span(&mut s.name.span, source);
    fill_generic_params_spans(&mut s.generics, source);
    for field in &mut s.fields {
        fill_span(&mut field.name.span, source);
        fill_type_span(&mut field.ty, source);
        if let Some(default) = &mut field.default {
            fill_expr_span(default, source);
        }
        fill_span(&mut field.span, source);
    }
    fill_span(&mut s.span, source);
}

fn fill_impl_def_spans(i: &mut crate::ast::ImplDef, source: &str) {
    fill_span(&mut i.name.span, source);
    if let Some(t) = &mut i.trait_name {
        fill_span(&mut t.span, source);
    }
    for arg in &mut i.trait_args {
        fill_type_span(arg, source);
    }
    fill_generic_params_spans(&mut i.generics, source);
    for func in &mut i.functions {
        fill_span(&mut func.name.span, source);
        fill_attributes_spans(&mut func.attributes, source);
        for p in &mut func.params {
            if let Some(label) = &mut p.external_label {
                fill_span(&mut label.span, source);
            }
            fill_span(&mut p.name.span, source);
            if let Some(ty) = &mut p.ty {
                fill_type_span(ty, source);
            }
            fill_span(&mut p.span, source);
        }
        if let Some(ret) = &mut func.return_type {
            fill_type_span(ret, source);
        }
        if let Some(body) = &mut func.body {
            fill_expr_span(body, source);
        }
        fill_span(&mut func.span, source);
    }
    fill_span(&mut i.span, source);
}

fn fill_enum_def_spans(e: &mut crate::ast::EnumDef, source: &str) {
    fill_span(&mut e.name.span, source);
    fill_generic_params_spans(&mut e.generics, source);
    for variant in &mut e.variants {
        fill_span(&mut variant.name.span, source);
        for field in &mut variant.fields {
            fill_span(&mut field.name.span, source);
            fill_type_span(&mut field.ty, source);
            fill_span(&mut field.span, source);
        }
        fill_span(&mut variant.span, source);
    }
    fill_span(&mut e.span, source);
}

fn fill_function_def_spans(f: &mut crate::ast::FunctionDef, source: &str) {
    fill_span(&mut f.name.span, source);
    fill_attributes_spans(&mut f.attributes, source);
    for p in &mut f.params {
        if let Some(label) = &mut p.external_label {
            fill_span(&mut label.span, source);
        }
        fill_span(&mut p.name.span, source);
        if let Some(ty) = &mut p.ty {
            fill_type_span(ty, source);
        }
        fill_span(&mut p.span, source);
    }
    if let Some(ret) = &mut f.return_type {
        fill_type_span(ret, source);
    }
    if let Some(body) = &mut f.body {
        fill_expr_span(body, source);
    }
    fill_span(&mut f.span, source);
}

pub(super) fn fill_type_span(ty: &mut Type, source: &str) {
    match ty {
        Type::Primitive(_) => {}
        Type::Ident(ident) => fill_span(&mut ident.span, source),
        Type::Array(inner) | Type::Optional(inner) => fill_type_span(inner, source),
        Type::Tuple(fields) => {
            for field in fields {
                fill_span(&mut field.name.span, source);
                fill_type_span(&mut field.ty, source);
                fill_span(&mut field.span, source);
            }
        }
        Type::Generic { name, args, span } => {
            fill_span(&mut name.span, source);
            for arg in args {
                fill_type_span(arg, source);
            }
            fill_span(span, source);
        }
        Type::Dictionary { key, value } => {
            fill_type_span(key, source);
            fill_type_span(value, source);
        }
        Type::Closure { params, ret } => {
            for (_, param) in params {
                fill_type_span(param, source);
            }
            fill_type_span(ret, source);
        }
    }
}
