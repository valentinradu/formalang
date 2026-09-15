//! Reject `Seq<T>` anywhere it cannot mean anything.
//!
//! A sequence is not a value. It holds no elements, it runs once, and
//! it exists only long enough for one consumer to drive it. So it has
//! no representation to store in a field, and no way to survive past
//! the expression that made it.
//!
//! Where a sequence may appear:
//!
//! | Position | Allowed |
//! | --- | --- |
//! | a local `let` | yes |
//! | a `for` source | yes |
//! | a function parameter | yes, `sink` only |
//! | an `extern fn` return type | yes — this is the host cursor |
//! | a struct field or enum variant field | no |
//! | a module-level `let` | no |
//! | a `fn` return type | no, in v1 |
//!
//! The `fn` return ban is a v1 restriction rather than a design limit.
//! A function returning a sequence is a pipeline fragment, and it
//! composes correctly if the backend always inlines it at the call
//! site. Inlining a linear pipeline is mechanical and the whole
//! program is available, so lift the restriction once mandatory
//! inlining exists and recursion is ruled out.

use super::super::module_resolver::ModuleResolver;
use super::super::SemanticAnalyzer;
use crate::ast::{
    Definition, EnumDef, File, FnParam, FunctionDef, ImplDef, ParamConvention, Statement,
    StructDef, Type,
};
use crate::error::CompilerError;
use crate::location::Span;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    pub(in crate::semantic) fn validate_sequence_placement(&mut self, file: &File) {
        for statement in &file.statements {
            match statement {
                Statement::Definition(def) => self.check_seq_definition(def),
                Statement::Let(let_binding) => {
                    if let Some(ty) = &let_binding.type_annotation {
                        self.reject_sequence(ty, "a module-level let", let_binding.span);
                    }
                }
                Statement::Use(_) => {}
            }
        }
    }

    fn check_seq_definition(&mut self, def: &Definition) {
        match def {
            Definition::Struct(s) => self.check_seq_struct(s),
            Definition::Enum(e) => self.check_seq_enum(e),
            Definition::Function(f) => self.check_seq_function(f),
            Definition::Impl(i) => self.check_seq_impl(i),
            Definition::Module(m) => {
                for nested in &m.definitions {
                    self.check_seq_definition(nested);
                }
            }
            Definition::Trait(_) => {}
        }
    }

    fn check_seq_struct(&mut self, struct_def: &StructDef) {
        for field in &struct_def.fields {
            self.reject_sequence(&field.ty, "a struct field", field.span);
        }
    }

    fn check_seq_enum(&mut self, enum_def: &EnumDef) {
        for variant in &enum_def.variants {
            for field in &variant.fields {
                self.reject_sequence(&field.ty, "an enum variant field", field.span);
            }
        }
    }

    fn check_seq_impl(&mut self, impl_def: &ImplDef) {
        // An `extern impl` declares the host's surface. The prelude's
        // own `Seq<T>` combinators live in one, and they legitimately
        // take and return sequences.
        if impl_def.is_extern {
            return;
        }
        for method in &impl_def.functions {
            self.check_seq_signature(
                &method.params,
                method.return_type.as_ref(),
                false,
                method.name.span,
            );
        }
    }

    fn check_seq_function(&mut self, func: &FunctionDef) {
        self.check_seq_signature(
            &func.params,
            func.return_type.as_ref(),
            func.extern_abi.is_some(),
            func.name.span,
        );
    }

    /// Shared by a top-level `fn` and an impl method, which the AST
    /// models as two different types with the same signature shape.
    fn check_seq_signature(
        &mut self,
        params: &[FnParam],
        return_type: Option<&Type>,
        is_extern: bool,
        name_span: Span,
    ) {
        for param in params {
            let Some(ty) = &param.ty else { continue };
            // A sequence parameter has to be consumed by the callee,
            // and `sink` is how the language says exactly that.
            if param.convention != ParamConvention::Sink {
                self.reject_sequence(
                    ty,
                    "a parameter that is not `sink`; a sequence must be consumed, so write `sink`",
                    param.span,
                );
            }
        }
        // An `extern fn` returning a sequence is the host cursor: the
        // host produces the elements and the generated code pulls
        // them. That is the one return position a sequence may hold.
        if !is_extern {
            if let Some(ty) = return_type {
                self.reject_sequence(ty, "a function return type", name_span);
            }
        }
    }

    /// Report `ty` when a sequence appears anywhere inside it.
    ///
    /// The search is structural, so `[Seq<I32>]` and `Seq<I32>?` are
    /// caught as well as a bare `Seq<I32>`. Nesting a sequence inside
    /// a container is the same mistake: the container would have to
    /// store something that cannot be stored.
    fn reject_sequence(&mut self, ty: &Type, position: &str, span: Span) {
        if !Self::mentions_sequence(ty) {
            return;
        }
        self.errors.push(CompilerError::SeqInvalidPosition {
            position: position.to_string(),
            span,
        });
    }

    fn mentions_sequence(ty: &Type) -> bool {
        match ty {
            Type::Generic { name, args, .. } => {
                name.name == "Seq" || args.iter().any(Self::mentions_sequence)
            }
            Type::Array(inner) | Type::Optional(inner) => Self::mentions_sequence(inner),
            Type::Tuple(fields) => fields.iter().any(|f| Self::mentions_sequence(&f.ty)),
            Type::Dictionary { key, value } => {
                Self::mentions_sequence(key) || Self::mentions_sequence(value)
            }
            Type::Closure { params, ret } => {
                params.iter().any(|(_, p)| Self::mentions_sequence(p))
                    || Self::mentions_sequence(ret)
            }
            Type::Primitive(_) | Type::Ident(_) => false,
        }
    }
}
