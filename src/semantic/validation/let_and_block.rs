//! Let-binding and block-statement validation: declared/inferred type
//! agreement, closure-binding registration, and per-block scope save/restore
//! of consumption flags.

use super::super::collect_bindings_from_pattern;
use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::{Expr, File, Type};
use crate::error::CompilerError;
use std::collections::{HashMap, HashSet};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// True for `Seq<T>`, the type a `for` expression produces.
    pub(in crate::semantic) fn is_sequence(ty: &SemType) -> bool {
        matches!(ty, SemType::Generic { base, .. } if base == "Seq")
    }

    /// Type-check a closure literal against a declared closure type.
    ///
    /// Pushes the declared parameter types into the inference scope —
    /// covering a literal parameter with no annotation of its own —
    /// and checks the body's inferred type against the declared return
    /// type. Without it an untyped closure parameter resolves to
    /// `Unknown`, which unifies with anything and hides a real
    /// mismatch.
    ///
    /// Both `let` paths call this. Only the module-level one did
    /// before, so `let c: () -> I32 = () -> "text"` inside a function
    /// body compiled.
    pub(super) fn check_closure_literal_against_annotation(
        &mut self,
        type_ann: Option<&Type>,
        value: &Expr,
        span: crate::location::Span,
        file: &File,
    ) {
        let (
            Some(Type::Closure {
                params: declared_params,
                ret: declared_ret,
            }),
            Expr::ClosureExpr {
                params: lit_params,
                body,
                return_type: lit_ret,
                ..
            },
        ) = (type_ann, value)
        else {
            return;
        };

        if lit_params.len() != declared_params.len() {
            return;
        }

        let mut seed: HashMap<String, SemType> = HashMap::new();
        for (lit, (_, dty)) in lit_params.iter().zip(declared_params.iter()) {
            let sem = lit
                .ty
                .as_ref()
                .map_or_else(|| SemType::from_ast(dty), SemType::from_ast);
            seed.insert(lit.name.name.clone(), sem);
        }
        self.inference_scope_stack.borrow_mut().push(seed);
        let inferred_body_sem = self.infer_type_sem(body, file);
        self.inference_scope_stack.borrow_mut().pop();

        let expected_ret = lit_ret
            .as_ref()
            .map_or_else(|| Self::type_to_string(declared_ret), Self::type_to_string);
        if !self.value_satisfies_declared(&expected_ret, &inferred_body_sem) {
            self.errors.push(CompilerError::TypeMismatch {
                expected: expected_ret,
                found: inferred_body_sem.display(),
                span,
            });
        }
    }

    /// Check a `let` value against the type its binding declares.
    ///
    /// Both `let` paths call this — the module-level one in
    /// [`Self::validate_let_statement`] and the block-level one in the
    /// statement walker. They used to differ: only the module-level
    /// path compared the two, so `let x: I32 = "text"` inside a
    /// function body compiled, and the lowered `IrBlockStatement::Let`
    /// claimed `ty: I32` over a `String` value. A backend that trusts
    /// that field would allocate an integer slot and store a string in
    /// it.
    ///
    /// Three shapes are compatible without being equal:
    ///
    /// - `nil` fits any optional type;
    /// - `T` fits `T?`, which is the implicit wrap;
    /// - a closure literal against a declared closure type, which the
    ///   bidirectional check right after this one covers in more
    ///   detail than a string comparison could;
    /// - a generic base whose arguments inference did not carry, for
    ///   which see [`Self::base_without_type_arguments`].
    pub(super) fn check_let_annotation(
        &mut self,
        type_ann: &Type,
        value: &Expr,
        span: crate::location::Span,
        file: &File,
    ) {
        let declared = Self::type_to_string(type_ann);
        let inferred_sem = self.infer_type_sem(value, file);
        let inferred = inferred_sem.display();

        if matches!(inferred_sem, SemType::Nil) && !declared.ends_with('?') {
            self.errors.push(CompilerError::NilAssignedToNonOptional {
                expected: declared,
                span,
            });
            return;
        }

        // A closure literal against a declared closure type is left to
        // the bidirectional check that follows; a string comparison
        // cannot say anything useful about it.
        let is_closure_pair =
            matches!(type_ann, Type::Closure { .. }) && matches!(value, Expr::ClosureExpr { .. });

        if !is_closure_pair && !self.value_satisfies_declared(&declared, &inferred_sem) {
            self.errors.push(CompilerError::TypeMismatch {
                expected: declared,
                found: inferred,
                span,
            });
        }
    }

    /// Decide what a `[T?]` annotation means for a literal that holds
    /// no `nil`.
    ///
    /// The element type says the array may hold nothing somewhere. For
    /// a `let mut` that is a statement about the future — a `nil` can
    /// be put there later — so the literal need not contain one.
    ///
    /// For a plain `let` there is no later. Every element is present
    /// and always will be, so the optional says more than the value
    /// means and `[T]` is what was wanted. Reporting it names the type
    /// to use rather than a bare mismatch.
    ///
    /// Only an array literal is judged: a value from anywhere else may
    /// hold a `nil` this file cannot see.
    pub(super) fn check_optional_elements_are_used(
        &mut self,
        type_ann: &Type,
        value: &Expr,
        mutable: bool,
        span: crate::location::Span,
    ) {
        if mutable {
            return;
        }
        let Type::Array(element) = type_ann else {
            return;
        };
        let Type::Optional(inner) = &**element else {
            return;
        };
        let Expr::Array { elements, .. } = value else {
            return;
        };
        // An empty literal says nothing either way.
        if elements.is_empty() {
            return;
        }
        if elements.iter().any(|e| {
            matches!(
                e,
                Expr::Literal {
                    value: crate::ast::Literal::Nil,
                    ..
                }
            )
        }) {
            return;
        }

        self.errors.push(CompilerError::PointlessOptionalElement {
            declared: Self::type_to_string(type_ann),
            suggested: format!("[{}]", Self::type_to_string(inner)),
            span,
        });
    }

    /// Report a `.variant` that has no enum to resolve against.
    ///
    /// A `let` with no annotation offers the value no context, so a
    /// bare `.red` there names nothing. Nothing checked it: the binding
    /// was accepted, the variant lowered to a `ResolvedType::Error`
    /// placeholder, and the first pass to meet that placeholder
    /// reported an internal error — so the user was told the compiler
    /// had broken, one pass away from the line they wrote.
    pub(super) fn check_inferred_enum_has_a_context(
        &mut self,
        annotation: Option<&Type>,
        value: &Expr,
        file: &File,
    ) {
        if annotation.is_some() {
            return;
        }
        if !matches!(self.infer_type_sem(value, file), SemType::InferredEnum) {
            return;
        }
        let variant = if let Expr::InferredEnumInstantiation { variant, .. } = value {
            variant.name.clone()
        } else {
            String::new()
        };
        self.errors.push(CompilerError::CannotInferEnumType {
            variant,
            span: value.span(),
        });
    }

    pub(super) fn validate_let_statement(
        &mut self,
        let_binding: &crate::ast::LetBinding,
        file: &File,
    ) {
        if let Some(type_ann) = &let_binding.type_annotation {
            // Tier-1 item E2: surface
            // `let x: SomeTrait = ...` as TraitUsedAsValueType (and
            // any other invalid type in the annotation) before the
            // value/declared compatibility check would mask it.
            self.validate_type(type_ann, let_binding.span);
        }
        self.validate_expr(&let_binding.value, file);
        // Reject nil-into-nonopt and any other mismatch between the
        // inferred value type and the declared annotation.
        if let Some(type_ann) = &let_binding.type_annotation {
            self.check_let_annotation(type_ann, &let_binding.value, let_binding.span, file);
            self.check_optional_elements_are_used(
                type_ann,
                &let_binding.value,
                let_binding.mutable,
                let_binding.span,
            );
        }
        self.check_inferred_enum_has_a_context(
            let_binding.type_annotation.as_ref(),
            &let_binding.value,
            file,
        );
        self.check_closure_literal_against_annotation(
            let_binding.type_annotation.as_ref(),
            &let_binding.value,
            let_binding.span,
            file,
        );
        // Register closure-typed module-level bindings for call-site enforcement.
        //
        // Conventions can come from either the explicit type annotation
        // (`let f: I32 -> I32 = ...`) or, when the let is unannotated, from
        // the closure literal's own parameter list (`let f = |n: I32| n + 1`).
        // Without the literal-based path, the call site `f(x)` resolves no
        // overload and emits `UndefinedReference`.
        let conventions_opt: Option<Vec<crate::ast::ParamConvention>> =
            match (&let_binding.type_annotation, &let_binding.value) {
                (Some(Type::Closure { params, .. }), _) => {
                    Some(params.iter().map(|(c, _)| *c).collect())
                }
                (None, Expr::ClosureExpr { params, .. }) => {
                    Some(params.iter().map(|p| p.convention).collect())
                }
                _ => None,
            };
        if let Some(conventions) = conventions_opt {
            let captures = if let Expr::ClosureExpr {
                params: cparams,
                body,
                ..
            } = &let_binding.value
            {
                let param_set: HashSet<String> =
                    cparams.iter().map(|p| p.name.name.clone()).collect();
                Some(Self::collect_free_variables(body, &param_set))
            } else {
                None
            };
            for binding in collect_bindings_from_pattern(&let_binding.pattern) {
                self.closure_binding_conventions
                    .insert(binding.name.clone(), conventions.clone());
                if let Some(caps) = &captures {
                    self.closure_binding_captures
                        .insert(binding.name.clone(), caps.clone());
                    self.fn_scope_closure_captures
                        .insert(binding.name, caps.clone());
                }
            }
        }
        self.validate_destructuring_pattern(
            &let_binding.pattern,
            &let_binding.value,
            let_binding.span,
            file,
        );
    }
}
