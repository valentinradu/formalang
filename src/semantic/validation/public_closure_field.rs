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
            if matches!(field.ty, Type::Closure { .. }) {
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
                if matches!(field.ty, Type::Closure { .. }) {
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
