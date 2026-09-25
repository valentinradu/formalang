//! Overload resolution: which of several functions of one name a call
//! means.
//!
//! An overload fits a call when the call's labels name its parameters
//! and the count lands between the required and the declared
//! parameters (`crate::ir::overload::call_fits`), and when each argument
//! has the type of its parameter. `format(value: 1)` means
//! `format(value: I32)`, and `format(value: "a")` means
//! `format(value: String)`.
//!
//! The types are compared exactly first, so `width(v: 1)` means
//! `width(v: I32)` and not `width(v: I64)`. When no overload fits
//! exactly, an unsuffixed numeric literal fits any numeric type of its
//! kind, the way it does in a position that declares one type. Among
//! the overloads that fit, the one that fires the fewest defaults wins.
//!
//! The analyzer records the overload that it chose for each call, and
//! IR lowering reads that record, so the two cannot choose differently.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::symbol_table::FunctionInfo;
use super::super::super::SemanticAnalyzer;
use super::overloads::ParamView;
use crate::ast::{Expr, File, Ident, Literal, NumberSourceKind, PrimitiveType, Type};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// The overloads that fit the call best. More than one result is an
    /// ambiguous call, and none is a call that fits no overload.
    pub(in crate::semantic::validation) fn most_specific_overloads<'o>(
        &self,
        overloads: &'o [FunctionInfo],
        args: &[(Option<Ident>, Expr)],
        file: &File,
    ) -> Vec<&'o FunctionInfo> {
        let labels: Vec<Option<String>> = args
            .iter()
            .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
            .collect();
        let shaped: Vec<&FunctionInfo> = overloads
            .iter()
            .filter(|o| {
                let views: Vec<_> = o.params.iter().map(ParamView::of_param_info).collect();
                crate::ir::overload::call_fits(&views, &labels, args.len())
            })
            .collect();
        let fitting = |lenient: bool| -> Vec<&FunctionInfo> {
            shaped
                .iter()
                .copied()
                .filter(|o| {
                    let views: Vec<_> = o.params.iter().map(ParamView::of_param_info).collect();
                    let names: Vec<String> =
                        o.generics.iter().map(|g| g.name.name.clone()).collect();
                    self.argument_types_fit(&views, &names, args, lenient, file)
                })
                .collect()
        };
        let mut pool = fitting(false);
        if pool.is_empty() {
            pool = fitting(true);
        }
        let fired = |o: &FunctionInfo| {
            o.params
                .iter()
                .filter(|p| p.name.name != "self")
                .count()
                .saturating_sub(args.len())
        };
        let fewest = pool.iter().map(|o| fired(o)).min();
        pool.into_iter()
            .filter(|o| Some(fired(o)) == fewest)
            .collect()
    }

    /// True when each argument has the type of the parameter it fills.
    /// A parameter typed by one of `generic_names`, and an argument
    /// whose type is not known here, fit anything.
    pub(in crate::semantic::validation) fn argument_types_fit(
        &self,
        params: &[ParamView<'_>],
        generic_names: &[String],
        args: &[(Option<Ident>, Expr)],
        lenient: bool,
        file: &File,
    ) -> bool {
        let non_self: Vec<_> = params.iter().filter(|p| p.name != "self").collect();
        args.iter().enumerate().all(|(position, (label, arg))| {
            let param = label.as_ref().map_or_else(
                || non_self.get(position).copied(),
                |label| {
                    non_self.iter().copied().find(|p| {
                        p.name == label.name || p.external_label == Some(label.name.as_str())
                    })
                },
            );
            let Some(declared) = param.and_then(|p| p.ty) else {
                return true;
            };
            if super::super::type_names::type_mentions_any(declared, generic_names) {
                return true;
            }
            if lenient && literal_fits(arg, declared) {
                return true;
            }
            let inferred = self.infer_type_sem(arg, file);
            inferred.is_indeterminate()
                || self.value_satisfies_declared(&Self::type_to_string(declared), &inferred)
        })
    }
}

/// True when `arg` is an unsuffixed numeric literal and `declared` is a
/// numeric type of the literal's kind.
const fn literal_fits(arg: &Expr, declared: &Type) -> bool {
    let Expr::Literal {
        value: Literal::Number(n),
        ..
    } = arg
    else {
        return false;
    };
    if n.suffix.is_some() {
        return false;
    }
    let Type::Primitive(p) = declared else {
        return false;
    };
    matches!(
        (n.kind, p),
        (
            NumberSourceKind::Integer,
            PrimitiveType::I32 | PrimitiveType::I64
        ) | (
            NumberSourceKind::Float,
            PrimitiveType::F32 | PrimitiveType::F64
        )
    )
}

impl crate::ir::overload::OverloadParam for ParamView<'_> {
    fn param_name(&self) -> &str {
        self.name
    }
    fn call_label(&self) -> Option<&str> {
        self.external_label
    }
    fn has_default(&self) -> bool {
        self.has_default
    }
}
