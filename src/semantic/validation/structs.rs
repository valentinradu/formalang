//! Struct instantiation field/mutability validation and lookup helpers.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Definition, Expr, File, Statement, StructDef};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate struct field requirements: all required fields must be provided, no unknown fields
    pub(super) fn validate_struct_fields(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        span: Span,
        file: &File,
    ) {
        // Find the struct definition in current file or module cache.
        // Clone the field name + declared type pairs so we can release the
        // borrow on `self` before recursing into type inference calls below.
        let (field_names, field_types, required_fields, generic_params) = {
            if let Some(def) = self.find_struct_def_in_files(struct_name, file) {
                let field_names: Vec<String> =
                    def.fields.iter().map(|f| f.name.name.clone()).collect();

                let field_types: Vec<(String, String)> = def
                    .fields
                    .iter()
                    .map(|f| (f.name.name.clone(), Self::type_to_string(&f.ty)))
                    .collect();

                let required_fields: Vec<String> = def
                    .fields
                    .iter()
                    .filter(|f| {
                        // Field is required if it has no inline default and is not optional
                        f.default.is_none() && !f.optional
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                let generic_params: Vec<String> =
                    def.generics.iter().map(|g| g.name.name.clone()).collect();

                (field_names, field_types, required_fields, generic_params)
            } else {
                return; // Struct not found, skip validation
            }
        };

        // Check all provided regular fields exist and type-check each value.
        for (arg_name, arg_value) in args {
            if !field_names.contains(&arg_name.name) {
                self.errors.push(CompilerError::UnknownField {
                    field: arg_name.name.clone(),
                    type_name: struct_name.to_string(),
                    span: arg_name.span,
                });
                continue;
            }
            let Some((_, declared)) = field_types.iter().find(|(n, _)| n == &arg_name.name) else {
                continue;
            };
            // Skip the check if the declared type references a generic
            // parameter of the struct — generic substitution is handled by
            // the IR monomorphisation pass, not the string-level comparison
            // here.
            if generic_params.iter().any(|g| declared.contains(g)) {
                continue;
            }
            let inferred_sem = self.infer_type_sem(arg_value, file);
            let inferred = inferred_sem.display();
            // nil is compatible with any optional type
            let nil_to_optional = matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
            // T is compatible with T? (implicit wrapping)
            let inner_to_optional =
                declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
            // declared can still be a string with "Unknown" in it (e.g. unresolved
            // type annotation); preserve the legacy guard for that case.
            let declared_indeterminate = declared.contains("Unknown");
            if !nil_to_optional
                && !inner_to_optional
                && !inferred_sem.is_indeterminate()
                && !declared_indeterminate
                && !self.type_strings_compatible(declared, &inferred)
            {
                self.errors.push(CompilerError::TypeMismatch {
                    expected: declared.clone(),
                    found: inferred,
                    span: arg_value.span(),
                });
            }
        }

        // Check all required regular fields are provided
        for field_name in required_fields {
            if !args.iter().any(|(name, _)| name.name == field_name) {
                self.errors.push(CompilerError::MissingField {
                    field: field_name,
                    type_name: struct_name.to_string(),
                    span,
                });
            }
        }
    }

    /// Find a struct definition in the current file and module cache
    pub(super) fn find_struct_def_in_files<'a>(
        &'a self,
        struct_name: &str,
        current_file: &'a File,
    ) -> Option<&'a StructDef> {
        // Search in current file
        for statement in &current_file.statements {
            if let Statement::Definition(def) = statement {
                if let Definition::Struct(struct_def) = &**def {
                    if struct_def.name.name == struct_name {
                        return Some(struct_def);
                    }
                }
            }
        }

        // Search in module cache
        for (file, _) in self.module_cache.values() {
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Struct(struct_def) = &**def {
                        if struct_def.name.name == struct_name {
                            return Some(struct_def);
                        }
                    }
                }
            }
        }

        None
    }

    pub(super) fn validate_struct_mutability(
        &mut self,
        struct_name: &str,
        args: &[(crate::ast::Ident, Expr)],
        file: &File,
        span: Span,
    ) {
        // Collect closure-typed field names and mutability info from the struct def,
        // dropping the borrow before mutating `self` for escape tracking.
        let struct_info: Option<Vec<(String, bool, bool)>> = {
            let mut found = None;
            for statement in &file.statements {
                if let Statement::Definition(def) = statement {
                    if let Definition::Struct(struct_def) = &**def {
                        if struct_def.name.name == struct_name {
                            let info: Vec<(String, bool, bool)> = struct_def
                                .fields
                                .iter()
                                .map(|f| {
                                    (
                                        f.name.name.clone(),
                                        f.mutable,
                                        matches!(f.ty, crate::ast::Type::Closure { .. }),
                                    )
                                })
                                .collect();
                            found = Some(info);
                            break;
                        }
                    }
                }
            }
            // Fall back to module cache if not found in current file.
            if found.is_none() {
                for (cached_file, _) in self.module_cache.values() {
                    for statement in &cached_file.statements {
                        if let Statement::Definition(def) = statement {
                            if let Definition::Struct(struct_def) = &**def {
                                if struct_def.name.name == struct_name {
                                    let info: Vec<(String, bool, bool)> = struct_def
                                        .fields
                                        .iter()
                                        .map(|f| {
                                            (
                                                f.name.name.clone(),
                                                f.mutable,
                                                matches!(f.ty, crate::ast::Type::Closure { .. }),
                                            )
                                        })
                                        .collect();
                                    found = Some(info);
                                    break;
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
            found
        };
        let Some(fields) = struct_info else {
            return;
        };
        for (arg_name, arg_expr) in args {
            let Some((_, field_mutable, field_is_closure)) =
                fields.iter().find(|(n, _, _)| n == &arg_name.name)
            else {
                continue;
            };
            if *field_mutable && !self.is_expr_mutable(arg_expr, file) {
                self.errors.push(CompilerError::MutabilityMismatch {
                    param: arg_name.name.clone(),
                    span,
                });
            }
            // Escape analysis: a closure value stored in a struct field escapes
            // with the struct — mark its captures as consumed.
            if *field_is_closure {
                self.escape_closure_value(arg_expr);
            }
        }
    }
}
