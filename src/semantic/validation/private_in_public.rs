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
//! These positions carry a type across the boundary, and this file
//! checks each one: the parameters, the return type and the trait
//! bounds of a public function, the fields of a public struct, the
//! payload fields of a public enum, the fields and the method
//! signatures of a public trait, and the type of a public `let`,
//! written or inferred. The check goes into inline modules, and
//! through containers: `[Hidden]` and `Hidden?` name `Hidden` too.
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
            match statement {
                Statement::Definition(def) => self.check_definition_for_private_types(def),
                Statement::Let(binding) => self.check_public_let(binding, file),
                Statement::Use(_) => {}
            }
        }
    }

    /// A public `let` carries its type across the boundary, written or
    /// inferred.
    fn check_public_let(&mut self, binding: &crate::ast::LetBinding, file: &File) {
        if !matches!(binding.visibility, Visibility::Public) {
            return;
        }
        let ty = binding.type_annotation.clone().or_else(|| {
            self.infer_type_sem(&binding.value, file)
                .to_ast(binding.span)
        });
        if let Some(ty) = ty {
            self.report_private_types_in(&ty, "a public let", binding.span, &[]);
        }
    }

    fn check_definition_for_private_types(&mut self, def: &Definition) {
        match def {
            Definition::Struct(s) => self.check_public_struct_fields(s),
            Definition::Enum(e) => self.check_public_enum_fields(e),
            Definition::Function(f) => self.check_public_function(f),
            Definition::Module(m) => {
                // The module's own names are in scope inside it.
                let shadowed = self.enter_module_scope(m);
                for nested in &m.definitions {
                    self.check_definition_for_private_types(nested);
                }
                self.leave_module_scope(shadowed);
            }
            Definition::Trait(t) => self.check_public_trait(t),
            // An impl block's methods are reached through their type,
            // whose own visibility already decided the question.
            Definition::Impl(_) => {}
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
                &struct_def.generics,
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
                    &enum_def.generics,
                );
            }
        }
    }

    /// A public trait shows its fields and its method signatures.
    fn check_public_trait(&mut self, trait_def: &crate::ast::TraitDef) {
        if !matches!(trait_def.visibility, Visibility::Public) {
            return;
        }
        let name = &trait_def.name.name;
        for field in &trait_def.fields {
            self.report_private_types_in(
                &field.ty,
                &format!("field '{}' of trait {name}", field.name.name),
                field.span,
                &trait_def.generics,
            );
        }
        for method in &trait_def.methods {
            for param in &method.params {
                if let Some(ty) = &param.ty {
                    self.report_private_types_in(
                        ty,
                        &format!(
                            "parameter '{}' of {name}.{}",
                            param.name.name, method.name.name
                        ),
                        param.span,
                        &trait_def.generics,
                    );
                }
            }
            if let Some(ret) = &method.return_type {
                self.report_private_types_in(
                    ret,
                    &format!("the return type of {name}.{}", method.name.name),
                    method.span,
                    &trait_def.generics,
                );
            }
        }
    }

    fn check_public_function(&mut self, func: &FunctionDef) {
        if !matches!(func.visibility, Visibility::Public) {
            return;
        }
        // A bound names a trait that a caller must be able to meet.
        for generic in &func.generics {
            for constraint in &generic.constraints {
                let crate::ast::GenericConstraint::Trait { name, .. } = constraint;
                if self.type_is_private(&name.name) {
                    self.errors.push(CompilerError::PrivateTypeInPublic {
                        type_name: name.name.clone(),
                        position: format!("a bound of {}", func.name.name),
                        span: name.span,
                    });
                }
            }
        }
        for param in &func.params {
            if let Some(ty) = &param.ty {
                self.report_private_types_in(
                    ty,
                    &format!("parameter '{}' of {}", param.name.name, func.name.name),
                    param.span,
                    &func.generics,
                );
            }
        }
        if let Some(ret) = &func.return_type {
            self.report_private_types_in(
                ret,
                &format!("the return type of {}", func.name.name),
                func.span,
                &func.generics,
            );
        }
    }

    /// Report every private type named anywhere inside `ty`.
    ///
    /// The walk goes through containers, because `[Hidden]` and
    /// `Hidden?` leak the same name that a bare `Hidden` does.
    ///
    /// A type parameter in `generics` hides a definition of the same
    /// name: in `pub enum Optional<T> { some(value: T) }`, `T` is the
    /// parameter, not a private trait `T` of the program.
    fn report_private_types_in(
        &mut self,
        ty: &Type,
        position: &str,
        span: Span,
        generics: &[crate::ast::GenericParam],
    ) {
        let mut found = Vec::new();
        super::type_names::for_each_named_type(ty, &mut |name| {
            let is_a_parameter = generics.iter().any(|g| g.name.name == name);
            if !is_a_parameter && self.type_is_private(name) {
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
        if let Some(info) = self.symbols.get_trait(name) {
            return matches!(info.visibility, Visibility::Private);
        }
        false
    }
}
