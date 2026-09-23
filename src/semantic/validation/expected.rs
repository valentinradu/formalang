//! The expected type of an expression, and the rule for a closure
//! parameter with no type.
//!
//! A position gives an expression an expected type only when a declared
//! type gives one: a `let` annotation, the declared type of a parameter
//! or a field, or a declared return type. The type goes down through a
//! group, the branches of `if` and `match`, the result of a block, and
//! the elements of an array, a tuple and a dictionary. IR lowering
//! follows the same positions.
//!
//! A closure parameter with no type takes its type from the expected
//! closure type. When the position gives no closure type with the same
//! number of parameters, the parameter needs a type:
//! [`CompilerError::ClosureParameterNeedsType`].
//!
//! `Some(SemType::Unknown)` means that a declared type exists but this
//! pass cannot compute it, for example the parameter of a method on a
//! receiver whose type is not known. The rule then reports nothing:
//! another check reports the real mistake.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use super::invocation::overloads::ParamView;
use crate::ast::{ClosureParam, Expr, File, Ident};
use crate::error::CompilerError;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// Validate `expr` in a position whose expected type is `expected`.
    ///
    /// [`Self::validate_expr`] takes the expected type as it starts, so
    /// the type reaches only `expr` and never a later expression.
    pub(in crate::semantic) fn validate_expr_expecting(
        &mut self,
        expr: &Expr,
        expected: Option<SemType>,
        file: &File,
    ) {
        self.expected_type = expected;
        self.validate_expr(expr, file);
    }

    /// Report each parameter of a closure literal that has no type and
    /// gets none from `expected`.
    pub(in crate::semantic::validation) fn check_closure_parameter_types(
        &mut self,
        params: &[ClosureParam],
        expected: Option<&SemType>,
    ) {
        if params.iter().all(|p| p.ty.is_some()) {
            return;
        }
        let expected = expected.map(Self::closure_slot);
        if expected.is_some_and(|t| matches!(t, SemType::Unknown)) {
            return;
        }
        if let Some(SemType::Closure { params: slots, .. }) = expected {
            if slots.len() == params.len() {
                // A slot that holds a type parameter of the callee that
                // no argument binds gives the parameter no type: in
                // `apply<U>(f: (U) -> I32)`, nothing says what `U` is.
                for (param, (_, slot)) in params.iter().zip(slots) {
                    if param.ty.is_none()
                        && crate::semantic::inference::holds_an_unbound_type_param(slot)
                    {
                        self.errors.push(CompilerError::ClosureParameterNeedsType {
                            param: param.name.name.clone(),
                            span: param.name.span,
                        });
                    }
                }
                return;
            }
            // A closure with the wrong number of parameters. Its shape
            // is wrong, whatever the parameter types are, so this is a
            // type mismatch and not a missing type.
            let blanks = vec!["_"; params.len()].join(", ");
            self.errors.push(CompilerError::TypeMismatch {
                expected: expected.map_or_else(String::new, SemType::display),
                found: format!("({blanks}) -> _"),
                span: params
                    .first()
                    .map_or_else(crate::location::Span::default, |p| p.span),
            });
            return;
        }
        for param in params.iter().filter(|p| p.ty.is_none()) {
            self.errors.push(CompilerError::ClosureParameterNeedsType {
                param: param.name.name.clone(),
                span: param.name.span,
            });
        }
    }

    /// The expected type of the body of a closure literal: its declared
    /// return type, or else the return type of the expected closure.
    pub(in crate::semantic::validation) fn closure_body_expected(
        return_type: Option<&crate::ast::Type>,
        expected: Option<&SemType>,
    ) -> Option<SemType> {
        if let Some(declared) = return_type {
            return Some(SemType::from_ast(declared));
        }
        match expected.map(Self::closure_slot) {
            Some(SemType::Closure { return_ty, .. }) => Some((**return_ty).clone()),
            Some(SemType::Unknown) => Some(SemType::Unknown),
            _ => None,
        }
    }

    /// The closure type in an expected type. An optional closure slot
    /// holds a closure too: `on_press: (() -> Event)?`.
    pub(in crate::semantic::validation) fn closure_slot(expected: &SemType) -> &SemType {
        if let SemType::Optional(inner) = expected {
            if matches!(**inner, SemType::Closure { .. }) {
                return inner;
            }
        }
        expected
    }

    /// `Some(SemType::Unknown)` when `expected` is a declared type that
    /// this pass cannot compute, and `None` otherwise.
    fn unknown_if_unknown(expected: &SemType) -> Option<SemType> {
        matches!(expected, SemType::Unknown).then_some(SemType::Unknown)
    }

    /// The expected type of the element of an array.
    pub(in crate::semantic::validation) fn expected_element(
        expected: Option<&SemType>,
    ) -> Option<SemType> {
        let expected = expected?;
        if let SemType::Array(element) = expected {
            return Some((**element).clone());
        }
        Self::unknown_if_unknown(expected)
    }

    /// The expected types of the key and the value of a dictionary.
    pub(in crate::semantic::validation) fn expected_entry(
        expected: Option<&SemType>,
    ) -> (Option<SemType>, Option<SemType>) {
        let Some(expected) = expected else {
            return (None, None);
        };
        if let SemType::Dictionary { key, value } = expected {
            return (Some((**key).clone()), Some((**value).clone()));
        }
        (
            Self::unknown_if_unknown(expected),
            Self::unknown_if_unknown(expected),
        )
    }

    /// The expected type of the tuple field `name`.
    pub(in crate::semantic::validation) fn expected_tuple_field(
        expected: Option<&SemType>,
        name: &str,
    ) -> Option<SemType> {
        let expected = expected?;
        if let SemType::Tuple(fields) = expected {
            return fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, ty)| ty.clone());
        }
        Self::unknown_if_unknown(expected)
    }

    /// The declared type of the parameter that the argument at `index`
    /// with `label` fills.
    ///
    /// `SemType::Unknown` when no parameter fits the argument, or when
    /// the parameter has no type: the call check reports that.
    pub(in crate::semantic) fn expected_argument(
        params: &[ParamView<'_>],
        index: usize,
        label: Option<&Ident>,
    ) -> SemType {
        let non_self: Vec<_> = params.iter().filter(|p| p.name != "self").collect();
        let param = label.map_or_else(
            || non_self.get(index).copied(),
            |label| {
                non_self
                    .iter()
                    .find(|p| p.external_label == Some(label.name.as_str()) || p.name == label.name)
                    .copied()
            },
        );
        param
            .and_then(|p| p.ty)
            .map_or(SemType::Unknown, SemType::from_ast)
    }

    /// The expected type of each argument of the call `name(args)`.
    ///
    /// A struct gives its field types, and a function its parameter
    /// types. With several overloads, the overload that the call fits
    /// gives them, as the call check chooses it. A binding that holds a
    /// closure gives its parameter types. Each argument is
    /// `Some(SemType::Unknown)` when the callee is not known here, or
    /// when no single overload fits: the call check reports that.
    pub(in crate::semantic::validation) fn invocation_argument_types(
        &self,
        name: &str,
        type_args: &[crate::ast::Type],
        args: &[(Option<Ident>, Expr)],
        file: &File,
    ) -> Vec<Option<SemType>> {
        let unknown = || vec![Some(SemType::Unknown); args.len()];
        let for_views = |views: &[ParamView<'_>]| -> Vec<Option<SemType>> {
            args.iter()
                .enumerate()
                .map(|(i, (label, _))| Some(Self::expected_argument(views, i, label.as_ref())))
                .collect()
        };

        if let Some(info) = self.symbols.get_struct_qualified(name) {
            let views: Vec<_> = info
                .fields
                .iter()
                .map(|f| ParamView {
                    name: f.name.as_str(),
                    external_label: None,
                    ty: Some(&f.ty),
                })
                .collect();
            return for_views(&views);
        }

        let simple_name = name.rsplit("::").next().unwrap_or(name);
        let overloads = self.function_overloads(name);
        match &*overloads {
            [] => match self.lookup_closure_type(simple_name) {
                Some(SemType::Closure { params, .. }) => (0..args.len())
                    .map(|i| Some(params.get(i).map_or(SemType::Unknown, |(_, ty)| ty.clone())))
                    .collect(),
                _ => unknown(),
            },
            [only] => self.function_argument_types(only, type_args, args, file),
            several => match self.most_specific_overloads(several, args, file).as_slice() {
                [fits] => self.function_argument_types(fits, type_args, args, file),
                _ => unknown(),
            },
        }
    }

    /// The expected type of each argument of a call to `function`.
    ///
    /// Its type parameters read as the call gives them: a written type
    /// argument, or the type that the other arguments bind. A name that
    /// nothing binds stays, and the closure check reports an untyped
    /// parameter that it types.
    fn function_argument_types(
        &self,
        function: &crate::semantic::symbol_table::FunctionInfo,
        type_args: &[crate::ast::Type],
        args: &[(Option<Ident>, Expr)],
        file: &File,
    ) -> Vec<Option<SemType>> {
        let views: Vec<_> = function
            .params
            .iter()
            .map(ParamView::of_param_info)
            .collect();
        let (view, fresh) =
            crate::semantic::inference::function_view(&function.generics, type_args);
        let outer = |ty| crate::semantic::inference::substitute_all(&ty, &view);
        let bindings = self.bind_call_generics(&fresh, &views, &outer, args, file);
        args.iter()
            .enumerate()
            .map(|(i, (label, _))| {
                let declared = outer(Self::expected_argument(&views, i, label.as_ref()));
                Some(crate::semantic::inference::substitute_all(
                    &declared, &bindings,
                ))
            })
            .collect()
    }

    /// Whether `name` names a type in scope: a struct, an enum, a
    /// trait, or a generic parameter of the enclosing definition.
    pub(in crate::semantic::validation) fn names_a_type(&self, name: &str) -> bool {
        self.symbols.is_type(name)
            || self.symbols.is_trait(name)
            || self.symbols.get_struct_qualified(name).is_some()
            || self.symbols.get_enum_qualified(name).is_some()
            || self.is_type_parameter(name)
    }

    /// The name of the enum in an expected type, for an inferred
    /// `.variant`: `Status`, `Box<T>` or `Status?`.
    pub(in crate::semantic::validation) fn expected_enum_name(
        expected: &SemType,
    ) -> Option<String> {
        let named = if let SemType::Optional(inner) = expected {
            inner
        } else {
            expected
        };
        if let SemType::Named(name) | SemType::Generic { base: name, .. } = named {
            return Some(name.clone());
        }
        None
    }

    /// The expected type of each payload field of an enum variant.
    ///
    /// `SemType::Unknown` when the variant is not known: the enum check
    /// reports that.
    pub(in crate::semantic::validation) fn expected_payload(
        &self,
        enum_name: Option<&str>,
        variant: &str,
        field: &str,
    ) -> SemType {
        let declared = enum_name
            .and_then(|name| self.symbols.get_enum_qualified(name))
            .and_then(|info| info.variant_fields.get(variant))
            .and_then(|fields| fields.iter().find(|f| f.name == field))
            .map(|f| SemType::from_ast(&f.ty));
        declared.unwrap_or(SemType::Unknown)
    }
}
