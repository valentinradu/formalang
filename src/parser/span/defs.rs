//! Span-filling for definitions (trait, struct, impl, enum, function, module)
//! and type expressions.

use super::{fill_expr_span, fill_span};
use crate::ast::{AttributeAnnotation, Definition, GenericConstraint, Type};
use crate::location::LineIndex;

pub(super) fn fill_definition_span(def: &mut Definition, index: &LineIndex<'_>) {
    match def {
        Definition::Module(m) => {
            fill_span(&mut m.name.span, index);
            for def in &mut m.definitions {
                fill_definition_span(def, index);
            }
            fill_span(&mut m.span, index);
        }
        Definition::Trait(t) => fill_trait_def_spans(t, index),
        Definition::Struct(s) => fill_struct_def_spans(s, index),
        Definition::Impl(i) => fill_impl_def_spans(i, index),
        Definition::Enum(e) => fill_enum_def_spans(e, index),
        Definition::Function(f) => fill_function_def_spans(f, index),
    }
}

fn fill_attributes_spans(attributes: &mut [AttributeAnnotation], index: &LineIndex<'_>) {
    for attr in attributes {
        fill_span(&mut attr.span, index);
    }
}

fn fill_generic_params_spans(params: &mut [crate::ast::GenericParam], index: &LineIndex<'_>) {
    for param in params {
        fill_span(&mut param.name.span, index);
        for constraint in &mut param.constraints {
            match constraint {
                GenericConstraint::Trait { name, args } => {
                    fill_span(&mut name.span, index);
                    for arg in args {
                        fill_type_span(arg, index);
                    }
                }
            }
        }
        fill_span(&mut param.span, index);
    }
}

fn fill_trait_def_spans(t: &mut crate::ast::TraitDef, index: &LineIndex<'_>) {
    fill_span(&mut t.name.span, index);
    for base in &mut t.traits {
        fill_span(&mut base.span, index);
    }
    fill_generic_params_spans(&mut t.generics, index);
    for field in &mut t.fields {
        fill_span(&mut field.name.span, index);
        fill_type_span(&mut field.ty, index);
        fill_span(&mut field.span, index);
    }
    for m in &mut t.methods {
        fill_span(&mut m.name.span, index);
        fill_attributes_spans(&mut m.attributes, index);
        fill_span(&mut m.span, index);
    }
    fill_span(&mut t.span, index);
}

fn fill_struct_def_spans(s: &mut crate::ast::StructDef, index: &LineIndex<'_>) {
    fill_span(&mut s.name.span, index);
    fill_generic_params_spans(&mut s.generics, index);
    for field in &mut s.fields {
        fill_span(&mut field.name.span, index);
        fill_type_span(&mut field.ty, index);
        if let Some(default) = &mut field.default {
            fill_expr_span(default, index);
        }
        fill_span(&mut field.span, index);
    }
    fill_span(&mut s.span, index);
}

fn fill_impl_def_spans(i: &mut crate::ast::ImplDef, index: &LineIndex<'_>) {
    fill_span(&mut i.name.span, index);
    if let Some(t) = &mut i.trait_name {
        fill_span(&mut t.span, index);
    }
    for arg in &mut i.trait_args {
        fill_type_span(arg, index);
    }
    fill_generic_params_spans(&mut i.generics, index);
    for func in &mut i.functions {
        fill_span(&mut func.name.span, index);
        fill_attributes_spans(&mut func.attributes, index);
        for p in &mut func.params {
            if let Some(label) = &mut p.external_label {
                fill_span(&mut label.span, index);
            }
            fill_span(&mut p.name.span, index);
            if let Some(ty) = &mut p.ty {
                fill_type_span(ty, index);
            }
            fill_span(&mut p.span, index);
        }
        if let Some(ret) = &mut func.return_type {
            fill_type_span(ret, index);
        }
        if let Some(body) = &mut func.body {
            fill_expr_span(body, index);
        }
        fill_span(&mut func.span, index);
    }
    fill_span(&mut i.span, index);
}

fn fill_enum_def_spans(e: &mut crate::ast::EnumDef, index: &LineIndex<'_>) {
    fill_span(&mut e.name.span, index);
    fill_generic_params_spans(&mut e.generics, index);
    for variant in &mut e.variants {
        fill_span(&mut variant.name.span, index);
        for field in &mut variant.fields {
            fill_span(&mut field.name.span, index);
            fill_type_span(&mut field.ty, index);
            fill_span(&mut field.span, index);
        }
        fill_span(&mut variant.span, index);
    }
    fill_span(&mut e.span, index);
}

fn fill_function_def_spans(f: &mut crate::ast::FunctionDef, index: &LineIndex<'_>) {
    fill_span(&mut f.name.span, index);
    fill_attributes_spans(&mut f.attributes, index);
    for p in &mut f.params {
        if let Some(label) = &mut p.external_label {
            fill_span(&mut label.span, index);
        }
        fill_span(&mut p.name.span, index);
        if let Some(ty) = &mut p.ty {
            fill_type_span(ty, index);
        }
        fill_span(&mut p.span, index);
    }
    if let Some(ret) = &mut f.return_type {
        fill_type_span(ret, index);
    }
    if let Some(body) = &mut f.body {
        fill_expr_span(body, index);
    }
    fill_span(&mut f.span, index);
}

pub(super) fn fill_type_span(ty: &mut Type, index: &LineIndex<'_>) {
    match ty {
        Type::Primitive(_) => {}
        Type::Ident(ident) => fill_span(&mut ident.span, index),
        Type::Array(inner) | Type::Optional(inner) => fill_type_span(inner, index),
        Type::Tuple(fields) => {
            for field in fields {
                fill_span(&mut field.name.span, index);
                fill_type_span(&mut field.ty, index);
                fill_span(&mut field.span, index);
            }
        }
        Type::Generic { name, args, span } => {
            fill_span(&mut name.span, index);
            for arg in args {
                fill_type_span(arg, index);
            }
            fill_span(span, index);
        }
        Type::Dictionary { key, value } => {
            fill_type_span(key, index);
            fill_type_span(value, index);
        }
        Type::Closure { params, ret } => {
            for (_, param) in params {
                fill_type_span(param, index);
            }
            fill_type_span(ret, index);
        }
    }
}
