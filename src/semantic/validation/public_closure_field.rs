//! Reject closure-typed fields on `pub` structs and `pub` enum variants.
//!
//! Closures are an internal abstraction: their representation is not
//! stable across module or backend boundaries, so any field that
//! escapes a `pub` definition through a closure type is a portability
//! hole. The rule is identical regardless of the consuming backend, so
//! it lives in the frontend rather than each backend's preflight.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, EnumDef, File, Statement, StructDef, Type, Visibility};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(in crate::semantic) fn validate_public_closure_fields(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.check_definition(def);
            }
        }
    }

    fn check_definition(&mut self, def: &Definition) {
        match def {
            Definition::Struct(s) => self.check_struct(s),
            Definition::Enum(e) => self.check_enum(e),
            Definition::Module(m) => {
                for nested in &m.definitions {
                    self.check_definition(nested);
                }
            }
            Definition::Trait(_) | Definition::Impl(_) | Definition::Function(_) => {}
        }
    }

    fn check_struct(&mut self, struct_def: &StructDef) {
        if !matches!(struct_def.visibility, Visibility::Public) {
            return;
        }
        for field in &struct_def.fields {
            if holds_a_closure(&field.ty) {
                self.errors.push(CompilerError::PublicClosureField {
                    owner: format!("struct {}", struct_def.name.name),
                    field: field.name.name.clone(),
                    span: field.span,
                });
            }
        }
    }

    fn check_enum(&mut self, enum_def: &EnumDef) {
        if !matches!(enum_def.visibility, Visibility::Public) {
            return;
        }
        for variant in &enum_def.variants {
            for field in &variant.fields {
                if holds_a_closure(&field.ty) {
                    self.errors.push(CompilerError::PublicClosureField {
                        owner: format!("enum {} variant {}", enum_def.name.name, variant.name.name),
                        field: field.name.name.clone(),
                        span: field.span,
                    });
                }
            }
        }
    }
}

/// True when `ty` is a closure type or holds one: in an array, an
/// optional, a tuple, a dictionary or a type argument. A named type has
/// its own check where it is declared.
fn holds_a_closure(ty: &Type) -> bool {
    match ty {
        Type::Closure { .. } => true,
        Type::Array(inner) | Type::Optional(inner) => holds_a_closure(inner),
        Type::Tuple(fields) => fields.iter().any(|f| holds_a_closure(&f.ty)),
        Type::Dictionary { key, value } => holds_a_closure(key) || holds_a_closure(value),
        Type::Generic { args, .. } => args.iter().any(holds_a_closure),
        Type::Primitive(_) | Type::Ident(_) => false,
    }
}
