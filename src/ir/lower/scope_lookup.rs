//! Finding a function or a type by the name that the source writes,
//! with the rules of inline modules and of overloads. Split out of
//! `mod.rs` to keep each file under the line ceiling that
//! `scripts/check_file_sizes.sh` enforces.

use super::IrLowerer;

impl IrLowerer<'_> {
    /// Look up a function by its source-level (single-segment) name
    /// using module-aware resolution: when called from inside
    /// `mod foo { … }`, prefer `"foo::name"` so intra-module calls
    /// resolve to the local definition; fall back to the bare name
    /// for top-level functions.
    pub(super) fn find_function_in_scope(&self, name: &str) -> Option<crate::ir::FunctionId> {
        if !self.current_module_prefix.is_empty() {
            let qualified = format!("{}::{}", self.current_module_prefix, name);
            if let Some(id) = self.module.function_id(&qualified) {
                return Some(id);
            }
        }
        self.module.function_id(name).or_else(|| {
            // `use m::f` names the function `m::f` of an inline module.
            self.symbols
                .local_alias(name)
                .and_then(|qualified| self.module.function_id(qualified))
        })
    }

    /// The id of the overload of `name` that the call's argument
    /// labels select.
    ///
    /// `IrModule.function_names` maps a name to one id, so a later
    /// overload overwrites an earlier one and every call to an
    /// overloaded name lowered to whichever was registered last. The
    /// semantic analyser resolved the overloads correctly, so the
    /// program compiled — and then a backend emitted a call to the
    /// wrong function. `format(value: x)` and
    /// `format(value: x, precision: p)` both became a call to the
    /// one-argument `format`.
    ///
    /// Selection mirrors the analyser: an overload is a candidate when
    /// it can take every label the call supplies and the call supplies
    /// every parameter it has no default for. Among candidates the one
    /// firing the fewest defaults wins; a tie keeps the first, which is
    /// the ambiguous case the analyser has already reported.
    pub(super) fn find_overload_in_scope(
        &self,
        name: &str,
        arg_labels: &[Option<String>],
        arg_count: usize,
    ) -> Option<crate::ir::FunctionId> {
        let qualified = if self.current_module_prefix.is_empty() {
            None
        } else {
            Some(format!("{}::{}", self.current_module_prefix, name))
        };

        let mut candidates: Vec<(crate::ir::FunctionId, &crate::ir::IrFunction)> = self
            .module
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| qualified.as_deref() == Some(f.name.as_str()) || f.name == name)
            .filter_map(|(i, f)| u32::try_from(i).ok().map(|i| (crate::ir::FunctionId(i), f)))
            .collect();

        // A call inside `mod math` means `math::add` when that exists,
        // whatever a top-level `add` says. Narrowing here keeps that
        // lexical rule ahead of the label matching below.
        if let Some(prefixed) = qualified.as_deref() {
            if candidates.iter().any(|(_, f)| f.name == prefixed) {
                candidates.retain(|(_, f)| f.name == prefixed);
            }
        }

        if candidates.len() <= 1 {
            return candidates
                .first()
                .map(|(id, _)| *id)
                .or_else(|| self.find_function_in_scope(name));
        }

        // The analyzer chose the overload with the types of the
        // arguments, which lowering does not have yet here.
        if let Some(index) = self.symbols.overload_choice(self.current_span, name) {
            if let Some((id, _)) = candidates.get(index) {
                return Some(*id);
            }
        }

        // The same rule a method call uses — labels fit, count between
        // required and declared, fewest defaults fired. One copy, so
        // the two cannot drift and so one test covers both.
        let ordered = candidates;
        crate::ir::overload::choose(
            ordered.iter().map(|(_, f)| *f).enumerate(),
            |f| f.params.as_slice(),
            arg_labels,
            arg_count,
        )
        .and_then(|index| ordered.get(index).map(|(id, _)| *id))
        .or_else(|| self.find_function_in_scope(name))
    }

    /// The name under which the type `name`, written in the current
    /// module, is registered.
    ///
    /// A type in an inline `mod` is registered by its qualified name,
    /// `m::P`, and the code in `m` names it `P`. The current module
    /// comes first, then each enclosing module, then the top level.
    pub(super) fn scoped_type_name(&self, name: &str) -> String {
        let mut prefix = self.current_module_prefix.as_str();
        while !prefix.is_empty() {
            let qualified = format!("{prefix}::{name}");
            if self.module.struct_id(&qualified).is_some()
                || self.module.enum_id(&qualified).is_some()
                || self.module.trait_id(&qualified).is_some()
            {
                return qualified;
            }
            prefix = prefix.rsplit_once("::").map_or("", |(outer, _)| outer);
        }
        // `use m::T` names the type `m::T` of an inline module.
        self.symbols
            .local_alias(name)
            .map_or_else(|| name.to_string(), str::to_string)
    }
}
