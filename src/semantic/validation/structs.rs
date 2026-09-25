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
        type_args: &[crate::ast::Type],
        args: &[(&crate::ast::Ident, &Expr)],
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

                // The written type travels with the rendered one: the
                // rendered form goes in the message, and the written
                // form is what the generic-parameter test walks.
                let field_types: Vec<(String, String, crate::ast::Type)> = def
                    .fields
                    .iter()
                    .map(|f| {
                        (
                            f.name.name.clone(),
                            Self::type_to_string(&f.ty),
                            f.ty.clone(),
                        )
                    })
                    .collect();

                let required_fields: Vec<String> = def
                    .fields
                    .iter()
                    .filter(|f| {
                        // A field is required if it has no default and is
                        // not optional. `Optional<T>` is `T?` too.
                        let spelled_optional = matches!(
                            &f.ty,
                            crate::ast::Type::Generic { name, args, .. }
                                if name.name == "Optional" && args.len() == 1
                        );
                        f.default.is_none() && !f.optional && !spelled_optional
                    })
                    .map(|f| f.name.name.clone())
                    .collect();

                (
                    field_names,
                    field_types,
                    required_fields,
                    def.generics.clone(),
                )
            } else {
                return; // Struct not found, skip validation
            }
        };

        let generic_names: Vec<String> =
            generic_params.iter().map(|g| g.name.name.clone()).collect();
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
            let Some((_, declared, declared_ty)) =
                field_types.iter().find(|(n, _, _)| n == &arg_name.name)
            else {
                continue;
            };
            // Skip the check if the declared type references a generic
            // parameter of the struct — generic substitution is handled by
            // the IR monomorphisation pass, not the string-level comparison
            // here.
            //
            // The test is on whole names. Asking whether the rendered
            // type *contains* the parameter made `String` mention a
            // parameter called `S`, and the field went unchecked.
            if super::type_names::type_mentions_any(declared_ty, &generic_names) {
                self.check_generic_field(&generic_params, type_args, declared_ty, arg_value, file);
                continue;
            }
            let inferred_sem = self.infer_type_sem(arg_value, file);
            let inferred = inferred_sem.display();
            // nil is compatible with any optional type
            let nil_to_optional = matches!(inferred_sem, SemType::Nil) && declared.ends_with('?');
            // T is compatible with T? (implicit wrapping)
            let inner_to_optional =
                declared.ends_with('?') && declared.trim_end_matches('?') == inferred.as_str();
            if !nil_to_optional
                && !inner_to_optional
                && !inferred_sem.is_indeterminate()
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

    /// Find a struct definition in the current file and module cache.
    ///
    /// The code of an inline `mod` names the module's structs by their
    /// short names, so the search starts in the module that holds the
    /// code, then goes out to each enclosing module and the top level.
    pub(super) fn find_struct_def_in_files<'a>(
        &'a self,
        struct_name: &str,
        current_file: &'a File,
    ) -> Option<&'a StructDef> {
        // Search in current file, innermost module first
        for scope in self.definitions_in_scope(current_file).iter().rev() {
            for def in scope {
                if let Definition::Struct(struct_def) = def {
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

    /// The definitions that the code under check can name, one list per
    /// level: the top level of the file first, then each module on
    /// `module_path`, down to the module that holds the code.
    fn definitions_in_scope<'a>(&self, file: &'a File) -> Vec<Vec<&'a Definition>> {
        let top: Vec<&Definition> = file
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Definition(def) => Some(&**def),
                Statement::Use(_) | Statement::Let(_) => None,
            })
            .collect();
        let mut levels = vec![top];
        for name in &self.module_path {
            let inner = levels.last().and_then(|defs| {
                defs.iter().find_map(|def| {
                    if let Definition::Module(m) = def {
                        if &m.name.name == name {
                            return Some(m.definitions.iter().collect::<Vec<_>>());
                        }
                    }
                    None
                })
            });
            match inner {
                Some(defs) => levels.push(defs),
                None => break,
            }
        }
        levels
    }

    /// Walk the struct's named arguments and run closure-escape
    /// analysis on any closure-typed field: a closure stored in a struct
    /// field escapes with the struct, so its captures must be marked
    /// consumed.
    ///
    /// Field-level mutability was removed; the previous mutability
    /// matching between the field and the caller's binding is gone.
    /// Check a field whose type names a type parameter of the struct.
    ///
    /// A field typed by a bare type parameter takes the parameter's
    /// bounds: `Wrapper(item: 1)` against `struct Wrapper<T: Named>` is
    /// refused. Written type arguments give the field its type:
    /// `Box<I32>(value: "x")` compares against `I32`.
    fn check_generic_field(
        &mut self,
        generic_params: &[crate::ast::GenericParam],
        type_args: &[crate::ast::Type],
        declared_ty: &crate::ast::Type,
        arg_value: &Expr,
        file: &File,
    ) {
        if let crate::ast::Type::Ident(ident) = declared_ty {
            if let Some(generic) = generic_params.iter().find(|g| g.name.name == ident.name) {
                self.check_generic_bounds(generic, arg_value, file);
            }
        }
        if type_args.len() != generic_params.len() || type_args.is_empty() {
            return;
        }
        let substituted = generic_params
            .iter()
            .zip(type_args)
            .fold(SemType::from_ast(declared_ty), |acc, (g, arg)| {
                acc.substitute_named(&g.name.name, &SemType::from_ast(arg))
            });
        let inferred_sem = self.infer_type_sem(arg_value, file);
        if !inferred_sem.is_indeterminate()
            && !self.value_satisfies_declared(&substituted.display(), &inferred_sem)
        {
            self.errors.push(CompilerError::TypeMismatch {
                expected: substituted.display(),
                found: inferred_sem.display(),
                span: arg_value.span(),
            });
        }
    }

    pub(super) fn validate_struct_mutability(
        &mut self,
        struct_name: &str,
        args: &[(&crate::ast::Ident, &Expr)],
        file: &File,
        _span: Span,
    ) {
        let struct_info: Option<Vec<(String, bool)>> =
            self.find_struct_def_in_files(struct_name, file).map(|def| {
                def.fields
                    .iter()
                    .map(|f| {
                        (
                            f.name.name.clone(),
                            matches!(f.ty, crate::ast::Type::Closure { .. }),
                        )
                    })
                    .collect()
            });
        let Some(fields) = struct_info else {
            return;
        };
        for (arg_name, arg_expr) in args {
            let Some((_, field_is_closure)) = fields.iter().find(|(n, _)| n == &arg_name.name)
            else {
                continue;
            };
            if *field_is_closure {
                self.escape_closure_value(arg_expr);
            }
        }
    }
}
