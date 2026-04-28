//! Reference-path validation: single-segment lookup, multi-segment field
//! chains, and the `self` / `self.field` shortcut inside impl blocks.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::File;
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate a reference expression (path lookup)
    pub(super) fn validate_expr_reference(
        &mut self,
        path: &[crate::ast::Ident],
        span: Span,
        _file: &File,
    ) {
        if let Some(first) = path.first() {
            if self.consumed_bindings.contains(&first.name) {
                self.errors.push(CompilerError::UseAfterSink {
                    name: first.name.clone(),
                    span,
                });
                return;
            }
        }
        // Check module visibility for qualified paths (mod::item)
        if !self.check_module_visibility(path, span) {
            return;
        }
        if path.first().is_some_and(|p| p.name == "self") {
            if self.current_impl_struct.is_none() {
                self.errors.push(CompilerError::UndefinedReference {
                    name: "self".to_string(),
                    span,
                });
                return;
            }
            if path.len() == 1 {
                return;
            }
            if let Some(field_ident) = path.get(1).filter(|_| path.len() == 2) {
                let field_name = &field_ident.name;
                if let Some(ref struct_name) = self.current_impl_struct {
                    if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                        for field in &struct_info.fields {
                            if field.name == *field_name {
                                return;
                            }
                        }
                        self.errors.push(CompilerError::UndefinedReference {
                            name: format!("self.{field_name}"),
                            span,
                        });
                        return;
                    }
                }
            }
            return;
        }

        if let Some(first) = path.first().filter(|_| path.len() == 1) {
            let name = &first.name;
            if self.symbols.is_let(name) {
                return;
            }
            if self.local_let_bindings.contains_key(name) {
                return;
            }
            for scope in &self.loop_var_scopes {
                if scope.contains(name) {
                    return;
                }
            }
            for scope in &self.closure_param_scopes {
                if scope.contains(name) {
                    return;
                }
            }
            if self.symbols.is_struct(name)
                || self.symbols.is_enum(name)
                || self.symbols.is_trait(name)
                || self.symbols.functions.contains_key(name.as_str())
            {
                return;
            }
            if let Some(ref struct_name) = self.current_impl_struct.clone() {
                if let Some(struct_info) = self.symbols.get_struct(struct_name) {
                    for field in &struct_info.fields {
                        if field.name == *name {
                            return;
                        }
                    }
                }
            }
            self.errors.push(CompilerError::UndefinedReference {
                name: name.clone(),
                span,
            });
            return;
        }

        // Multi-segment paths (e.g. `p.x.y`): walk each segment as a field
        // access from the root's inferred type and surface an
        // `UnknownField` error at the first broken link. Module-qualified
        // paths (handled above by `check_module_visibility`) fall through
        // this validation without firing since no let/local binding with
        // that name will be in scope.
        if path.len() >= 2 {
            let Some(first) = path.first() else {
                return;
            };
            // Root must be something we can infer a type for.
            let root_type_string = if let Some(ty) = self.symbols.get_let_type(&first.name) {
                ty.to_string()
            } else if let Some((ty, _)) = self.local_let_bindings.get(&first.name) {
                ty.clone()
            } else {
                return;
            };
            if let Some(rest) = path.get(1..) {
                self.validate_field_chain(&root_type_string, rest, span);
            }
        }
    }

    /// Walk a chain of field accesses starting from `root_type`, emitting
    /// `UnknownField` at the first segment that does not name a field of
    /// the current struct type. Bails silently if the type cannot be
    /// resolved — type inference is best-effort and we don't want to
    /// drown the user in spurious errors when inference itself is
    /// unreliable.
    fn validate_field_chain(&mut self, root_type: &str, rest: &[crate::ast::Ident], span: Span) {
        let mut current = root_type.trim_end_matches('?').to_string();
        for seg in rest {
            let Some(struct_info) = self.symbols.get_struct(&current) else {
                return;
            };
            if let Some(field) = struct_info.fields.iter().find(|f| f.name == seg.name) {
                current = Self::type_to_string(&field.ty)
                    .trim_end_matches('?')
                    .to_string();
            } else {
                self.errors.push(CompilerError::UnknownField {
                    field: seg.name.clone(),
                    type_name: current.clone(),
                    span,
                });
                return;
            }
        }
    }
}
