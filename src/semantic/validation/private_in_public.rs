//! Reject a private type named in a public signature.
//!
//! A `pub` definition is what another module sees. Naming a private
//! type in one hands the reader a value whose type they cannot write
//! down: they can call `pub fn f() -> Hidden`, but they cannot declare
//! a binding for the result, pass it on, or name it in their own
//! signature.
//!
//! This is the same portability hole that
//! [`super::public_closure_field`] closes for closure-typed fields, one
//! step out: there the escaping type has no stable representation, here
//! it has no name the caller may use. Rust reports it as E0446, Swift
//! as "cannot be declared public".
//!
//! Three positions carry a type across the boundary: a public
//! function's parameters and return type, a public struct's fields, and
//! a public enum variant's payload fields. Each is checked here.
//!
//! A type this file cannot resolve is left alone. It may come from an
//! import, and guessing would reject a correct program.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, EnumDef, File, FunctionDef, Statement, StructDef, Type, Visibility};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(in crate::semantic) fn validate_private_in_public(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.check_definition_for_private_types(def);
            }
        }
    }

    fn check_definition_for_private_types(&mut self, def: &Definition) {
        match def {
            Definition::Struct(s) => self.check_public_struct_fields(s),
            Definition::Enum(e) => self.check_public_enum_fields(e),
            Definition::Function(f) => self.check_public_function(f),
            Definition::Module(m) => {
                for nested in &m.definitions {
                    self.check_definition_for_private_types(nested);
                }
            }
            // An impl block's methods are reached through their type,
            // whose own visibility already decided the question. A
            // trait's methods travel with the trait the same way.
            Definition::Impl(_) | Definition::Trait(_) => {}
        }
    }

    fn check_public_struct_fields(&mut self, struct_def: &StructDef) {
        if !matches!(struct_def.visibility, Visibility::Public) {
            return;
        }
        for field in &struct_def.fields {
            self.report_private_types_in(
                &field.ty,
                &format!(
                    "field '{}' of struct {}",
                    field.name.name, struct_def.name.name
                ),
                field.span,
            );
        }
    }

    fn check_public_enum_fields(&mut self, enum_def: &EnumDef) {
        if !matches!(enum_def.visibility, Visibility::Public) {
            return;
        }
        for variant in &enum_def.variants {
            for field in &variant.fields {
                self.report_private_types_in(
                    &field.ty,
                    &format!(
                        "field '{}' of variant {}.{}",
                        field.name.name, enum_def.name.name, variant.name.name
                    ),
                    field.span,
                );
            }
        }
    }

    fn check_public_function(&mut self, func: &FunctionDef) {
        if !matches!(func.visibility, Visibility::Public) {
            return;
        }
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.report_private_types_in(
                    ty,
                    &format!("parameter '{}' of {}", param.name.name, func.name.name),
                    param.span,
                );
            }
        }
        if let Some(ret) = &func.return_type {
            self.report_private_types_in(
                ret,
                &format!("the return type of {}", func.name.name),
                func.span,
            );
        }
    }

    /// Report every private type named anywhere inside `ty`.
    ///
    /// The walk goes through containers, because `[Hidden]` and
    /// `Hidden?` leak the same name that a bare `Hidden` does.
    fn report_private_types_in(&mut self, ty: &Type, position: &str, span: Span) {
        let mut found = Vec::new();
        super::type_names::for_each_named_type(ty, &mut |name| {
            if self.type_is_private(name) {
                found.push(name.to_string());
            }
        });
        for name in found {
            self.errors.push(CompilerError::PrivateTypeInPublic {
                type_name: name,
                position: position.to_string(),
                span,
            });
        }
    }

    /// Whether this name belongs to a definition that is not `pub`.
    ///
    /// A name that resolves to nothing is not private: it may be an
    /// import, or a generic parameter, and a guess here would reject a
    /// correct program.
    fn type_is_private(&self, name: &str) -> bool {
        if let Some(info) = self.symbols.get_struct(name) {
            return matches!(info.visibility, Visibility::Private);
        }
        if let Some(info) = self.symbols.get_enum_qualified(name) {
            return matches!(info.visibility, Visibility::Private);
        }
        false
    }
}
