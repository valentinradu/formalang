//! Reference-path validation: single-segment lookup, multi-segment field
//! chains, and the `self` / `self.field` shortcut inside impl blocks.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::sem_type::SemType;
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
            if !self.names_a_value_in_scope(&first.name) {
                self.errors.push(CompilerError::UndefinedReference {
                    name: first.name.clone(),
                    span,
                });
            }
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
            // Root must be something we can infer a type for. Both module-level
            // lets and local bindings carry a structural `SemType`; render with
            // `display()` so the chain validator sees a uniform name shape.
            let root_type = if let Some(ty) = self.symbols.get_let_type(&first.name) {
                ty.clone()
            } else if let Some((ty, _)) = self.local_let_bindings.get(&first.name) {
                ty.clone()
            } else {
                // A root with no known type can still be in scope, for
                // example a loop variable. A root that is not in scope
                // is an error here, and not only in IR lowering.
                if !self.names_a_value_in_scope(&first.name) {
                    self.errors.push(CompilerError::UndefinedReference {
                        name: first.name.clone(),
                        span: first.span,
                    });
                }
                return;
            };
            if let Some(rest) = path.get(1..) {
                self.validate_field_chain(&root_type, rest, span);
            }
        }
    }

    /// True when `name` is a binding, a field of the current impl's
    /// struct, a type, a trait or a function that is in scope.
    fn names_a_value_in_scope(&self, name: &str) -> bool {
        if self.symbols.is_let(name) || self.local_let_bindings.contains_key(name) {
            return true;
        }
        if self.loop_var_scopes.iter().any(|s| s.contains_key(name))
            || self.closure_param_scopes.iter().any(|s| s.contains(name))
        {
            return true;
        }
        if self.symbols.is_struct(name)
            || self.symbols.is_enum(name)
            || self.symbols.is_trait(name)
            || self.symbols.functions.contains_key(name)
        {
            return true;
        }
        self.current_impl_struct
            .as_ref()
            .and_then(|s| self.symbols.get_struct(s))
            .is_some_and(|info| info.fields.iter().any(|f| f.name == name))
    }

    /// Walk a chain of field accesses starting from `root_type`, emitting
    /// `UnknownField` at the first segment that does not name a field of
    /// the current type. Bails silently if the type cannot be
    /// resolved — type inference is best-effort and we don't want to
    /// drown the user in spurious errors when inference itself is
    /// unreliable.
    ///
    /// A tuple names its fields the way a struct does, so it takes the
    /// same walk. Without it the chain stopped at the tuple and the IR
    /// lowering pass reported the missing field as an internal error,
    /// which told the user to file a bug for a typo in their own
    /// program.
    fn validate_field_chain(
        &mut self,
        root_type: &SemType,
        rest: &[crate::ast::Ident],
        span: Span,
    ) {
        let mut current = root_type.clone();
        for seg in rest {
            // An optional is unwrapped elsewhere; here it only stands
            // between the walk and the fields underneath.
            if let SemType::Optional(inner) = current {
                current = *inner;
            }

            let next = match &current {
                SemType::Tuple(fields) => fields
                    .iter()
                    .find(|(name, _)| name == &seg.name)
                    .map(|(_, ty)| ty.clone()),
                SemType::Primitive(_) | SemType::Closure { .. } | SemType::Nil => {
                    // A number, a boolean or a closure carries no
                    // fields, so this link is wrong however it is
                    // spelled. Reported here rather than left to IR
                    // lowering, which called it an internal error.
                    None
                }
                SemType::Named(_)
                | SemType::Array(_)
                | SemType::Optional(_)
                | SemType::Generic { .. }
                | SemType::Dictionary { .. }
                | SemType::Unknown
                | SemType::InferredEnum => {
                    let owner = Self::field_owner_name(&current);
                    let Some(struct_info) = self.symbols.get_struct(&owner) else {
                        // An enum value carries no fields: a payload
                        // is read by a `match` arm. Any other
                        // unresolved name may be an import, so the
                        // walk leaves it alone.
                        if self.symbols.get_enum_qualified(&owner).is_some() {
                            self.errors.push(CompilerError::UnknownField {
                                field: seg.name.clone(),
                                type_name: owner,
                                span,
                            });
                        }
                        return;
                    };
                    struct_info
                        .fields
                        .iter()
                        .find(|f| f.name == seg.name)
                        .map(|f| SemType::from_ast(&f.ty))
                }
            };

            let Some(next) = next else {
                self.errors.push(CompilerError::UnknownField {
                    field: seg.name.clone(),
                    type_name: current.display(),
                    span,
                });
                return;
            };
            current = next;
        }
    }
}
