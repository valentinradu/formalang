//! Which of several methods of one name a call means. Split out of
//! `mod.rs` to keep each file under the line ceiling that
//! `scripts/check_file_sizes.sh` enforces.

use super::super::super::module_resolver::ModuleResolver;
use super::super::super::SemanticAnalyzer;
use crate::ast::{Expr, File};

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// The overload of a method that a call means.
    ///
    /// The rule is `crate::ir::overload::choose`, the one IR lowering
    /// uses: of the overloads whose parameters take the call's labels
    /// and count, the one that leaves the fewest parameters to their
    /// defaults. When none fits, the first that takes the count, then
    /// the first, so a diagnostic names the likeliest one.
    pub(in crate::semantic) fn choose_method_overload<'f>(
        &self,
        overloads: &[(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])],
        args: &[(Option<crate::ast::Ident>, Expr)],
        file: &File,
    ) -> Option<(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])> {
        let labels: Vec<Option<String>> = args
            .iter()
            .map(|(label, _)| label.as_ref().map(|l| l.name.clone()))
            .collect();
        // The argument types decide between methods that differ only in
        // their parameter types: exact types first, then an unsuffixed
        // literal of either width. IR lowering and
        // `ResolveReferencesPass` apply the same rule to the lowered
        // argument types.
        let fitting =
            |lenient: bool| -> Vec<(&'f crate::ast::FnDef, &'f [crate::ast::GenericParam])> {
                overloads
                .iter()
                .copied()
                .filter(|(fn_def, impl_generics)| {
                    let views: Vec<_> = fn_def
                        .params
                        .iter()
                        .map(crate::semantic::validation::invocation::overloads::ParamView::of_fn_param)
                        .collect();
                    let names: Vec<String> = impl_generics
                        .iter()
                        .chain(&fn_def.generics)
                        .map(|g| g.name.name.clone())
                        .collect();
                    self.argument_types_fit(&views, &names, args, lenient, file)
                })
                .collect()
            };
        let mut pool = fitting(false);
        if pool.is_empty() {
            pool = fitting(true);
        }
        if pool.is_empty() {
            pool = overloads.to_vec();
        }
        crate::ir::overload::choose(
            pool.iter().enumerate(),
            |(fn_def, _)| fn_def.params.as_slice(),
            &labels,
            args.len(),
        )
        .and_then(|index| pool.get(index).copied())
        .or_else(|| {
            overloads
                .iter()
                .find(|(fn_def, _)| Self::method_arity_mismatch(&fn_def.params, args).is_none())
                .copied()
        })
        .or_else(|| overloads.first().copied())
    }

    /// Whether another method of `overloads` has the same labels and
    /// parameter types as `chosen`.
    ///
    /// One impl cannot hold two such methods: that is a
    /// `DuplicateDefinition`. Two impls can: one type can implement two
    /// instances of one generic trait, as `Container<I32>` and
    /// `Container<String>`, and each gives `fn get(self)`. A call on the
    /// type then fits both, and only a bound can say which it means.
    pub(in crate::semantic) fn has_twin_method(
        overloads: &[(&crate::ast::FnDef, &[crate::ast::GenericParam])],
        chosen: &crate::ast::FnDef,
    ) -> bool {
        let shape = |f: &crate::ast::FnDef| -> Vec<(String, Option<String>)> {
            f.params
                .iter()
                .filter(|p| p.name.name != "self")
                .map(|p| {
                    let label = p
                        .external_label
                        .as_ref()
                        .map_or_else(|| p.name.name.clone(), |l| l.name.clone());
                    (
                        label,
                        p.ty.as_ref().map(crate::semantic::symbol_table::ty_shape),
                    )
                })
                .collect()
        };
        let wanted = shape(chosen);
        overloads
            .iter()
            .any(|(other, _)| !std::ptr::eq(*other, chosen) && shape(other) == wanted)
    }

    /// Check a call on a type, `Counter.zero()`: it reaches only a
    /// method with no `self`. Return whether the call is right, so the
    /// caller can go on to check the arguments.
    pub(in crate::semantic) fn check_static_call(
        &mut self,
        chosen: Option<&crate::ast::FnDef>,
        method: &str,
        type_name: &str,
        span: crate::location::Span,
    ) -> bool {
        match chosen {
            None => {
                self.errors
                    .push(crate::error::CompilerError::UndefinedReference {
                        name: format!("method '{method}' on type '{type_name}'"),
                        span,
                    });
                false
            }
            Some(fn_def) if fn_def.params.iter().any(|p| p.name.name == "self") => {
                self.errors
                    .push(crate::error::CompilerError::NotAStaticMethod {
                        method: method.to_string(),
                        type_name: type_name.to_string(),
                        span,
                    });
                false
            }
            Some(_) => true,
        }
    }

    /// The type that a receiver names, when the receiver is the name of
    /// a struct: `Counter` in `Counter.zero()`. A struct name wins over
    /// a local binding of the same name, as an enum name does.
    pub(in crate::semantic) fn type_receiver(
        &self,
        receiver: &Expr,
    ) -> Option<crate::semantic::sem_type::SemType> {
        let Expr::Reference { path, .. } = receiver else {
            return None;
        };
        let [name] = path.as_slice() else {
            return None;
        };
        (self.symbols.is_struct(&name.name) && self.symbols.get_enum_variants(&name.name).is_none())
            .then(|| crate::semantic::sem_type::SemType::Named(name.name.clone()))
    }
}
