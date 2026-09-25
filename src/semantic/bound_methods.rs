//! The methods that a trait bound gives a type parameter.
//!
//! In `fn unwrap<T: Container<I32>>(b: T) -> I32 { b.get() }`, the call
//! `b.get()` means the method `get` of `Container`, with the trait's own
//! type parameter read as `I32`. The trait may name its parameter `T`
//! too; that `T` is the trait's, not the function's.

use super::module_resolver::ModuleResolver;
use super::sem_type::SemType;
use super::SemanticAnalyzer;
use crate::ast::{FnSig, Type};
use std::collections::HashSet;

/// A method of a bound, and the types that the bound gives the trait's
/// own type parameters.
pub(super) struct BoundMethod {
    pub(super) sig: FnSig,
    substitution: Vec<(String, SemType)>,
}

impl BoundMethod {
    /// `ty` with each type parameter of the trait read as the bound
    /// gives it.
    pub(super) fn resolve(&self, ty: &Type) -> SemType {
        self.substitution
            .iter()
            .fold(SemType::from_ast(ty), |acc, (name, arg)| {
                acc.substitute_named(name, arg)
            })
    }
}

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// The method `method` that a bound of the type parameter `param`
    /// gives, when `param` is a type parameter in scope.
    pub(super) fn bound_method(&self, param: &str, method: &str) -> Option<BoundMethod> {
        let scope = self
            .generic_scopes
            .iter()
            .rev()
            .find(|scope| scope.params.contains_key(param))?;
        let bounds = scope.bound_args.get(param)?;
        for (trait_name, args) in bounds {
            let mut visited = HashSet::new();
            let Some((owner, sig)) = self.trait_chain_method(trait_name, method, &mut visited)
            else {
                continue;
            };
            // The arguments of the bound fill the parameters of the
            // trait that the bound names. A method of a parent trait
            // keeps its own names.
            let substitution = if owner == *trait_name {
                self.symbols
                    .get_trait(trait_name)
                    .map(|info| {
                        info.generics
                            .iter()
                            .zip(args)
                            .map(|(g, a)| (g.name.name.clone(), SemType::from_ast(a)))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            return Some(BoundMethod { sig, substitution });
        }
        None
    }

    /// The trait that declares `method`, `trait_name` or one of its
    /// parents, and the method's signature.
    fn trait_chain_method(
        &self,
        trait_name: &str,
        method: &str,
        visited: &mut HashSet<String>,
    ) -> Option<(String, FnSig)> {
        if !visited.insert(trait_name.to_string()) {
            return None;
        }
        let info = self.symbols.get_trait(trait_name)?;
        if let Some(sig) = info.methods.iter().find(|m| m.name.name == method) {
            return Some((trait_name.to_string(), sig.clone()));
        }
        info.composed_traits
            .iter()
            .find_map(|parent| self.trait_chain_method(parent, method, visited))
    }
}
