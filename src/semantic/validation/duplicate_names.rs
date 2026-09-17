//! Reject two declarations of one name inside a single definition.
//!
//! A struct with two fields called `x`, or a function with two
//! parameters called `a`, has no answer to the question "which one does
//! `x` mean?". Nothing checked this, so both reached the IR: a lowered
//! struct carried two fields of the same name, and a backend computing
//! a field offset by name picked whichever it met first, over a layout
//! with a phantom extra slot.
//!
//! The rule covers the places a name is introduced inside a definition:
//! a struct's fields, an enum variant's payload fields, and the
//! parameters of a function, a method, or a trait method. Duplicate
//! definitions at module level, duplicate enum variants, and duplicate
//! generic parameters are each caught elsewhere.
//!
//! Shadowing is a different thing and stays legal: `let a = 1` followed
//! by `let a = 2` in one block introduces a second binding that hides
//! the first, the way it does in Rust. There the later name has a
//! defined meaning; here it does not.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, EnumDef, File, FnParam, Ident, Statement, StructDef, TraitDef};
use crate::error::CompilerError;
use std::collections::HashSet;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(in crate::semantic) fn validate_duplicate_names(&mut self, file: &File) {
        for statement in &file.statements {
            if let Statement::Definition(def) = statement {
                self.check_definition_for_duplicates(def);
            }
        }
    }

    fn check_definition_for_duplicates(&mut self, def: &Definition) {
        match def {
            Definition::Struct(s) => self.check_struct_fields_are_unique(s),
            Definition::Enum(e) => self.check_enum_fields_are_unique(e),
            Definition::Function(f) => self.check_params_are_unique(&f.name, &f.params),
            Definition::Impl(impl_def) => {
                // A method overloads by the shape of the call, the way
                // a free function does, so two of one name is not a
                // duplicate. Two with the same shape would be, but
                // deciding that belongs with overload-ambiguity
                // reporting rather than here.
                for func in &impl_def.functions {
                    self.check_params_are_unique(&func.name, &func.params);
                }
            }
            Definition::Trait(trait_def) => self.check_trait_is_unique(trait_def),
            Definition::Module(m) => {
                for nested in &m.definitions {
                    self.check_definition_for_duplicates(nested);
                }
            }
        }
    }

    fn check_struct_fields_are_unique(&mut self, struct_def: &StructDef) {
        let mut seen = HashSet::new();
        for field in &struct_def.fields {
            if !seen.insert(field.name.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!(
                        "field '{}' of struct {}",
                        field.name.name, struct_def.name.name
                    ),
                    span: field.span,
                });
            }
        }
    }

    fn check_enum_fields_are_unique(&mut self, enum_def: &EnumDef) {
        for variant in &enum_def.variants {
            let mut seen = HashSet::new();
            for field in &variant.fields {
                if !seen.insert(field.name.name.as_str()) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!(
                            "field '{}' of variant {}.{}",
                            field.name.name, enum_def.name.name, variant.name.name
                        ),
                        span: field.span,
                    });
                }
            }
        }
    }

    fn check_trait_is_unique(&mut self, trait_def: &TraitDef) {
        let mut seen = HashSet::new();
        for field in &trait_def.fields {
            if !seen.insert(field.name.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!(
                        "field '{}' of trait {}",
                        field.name.name, trait_def.name.name
                    ),
                    span: field.span,
                });
            }
        }
        for method in &trait_def.methods {
            self.check_params_are_unique(&method.name, &method.params);
        }
    }

    fn check_params_are_unique(&mut self, owner: &Ident, params: &[FnParam]) {
        // A call names an argument by its external label when the
        // parameter declares one, so both names must stay unique.
        let mut seen_names = HashSet::new();
        let mut seen_labels = HashSet::new();
        for param in params {
            if !seen_names.insert(param.name.name.as_str()) {
                self.errors.push(CompilerError::DuplicateDefinition {
                    name: format!("parameter '{}' of {}", param.name.name, owner.name),
                    span: param.span,
                });
            }
            if let Some(label) = &param.external_label {
                if !seen_labels.insert(label.name.as_str()) {
                    self.errors.push(CompilerError::DuplicateDefinition {
                        name: format!("argument label '{}' of {}", label.name, owner.name),
                        span: param.span,
                    });
                }
            }
        }
    }
}
