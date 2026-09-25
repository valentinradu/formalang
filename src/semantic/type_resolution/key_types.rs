//! The types that can be a dictionary key.
//!
//! A key needs a total equality that a backend can hash. `String`,
//! `I32`, `I64` and `Boolean` have one. A struct or an enum has one
//! when each of its fields and payloads has one. A float does not
//! (`NaN != NaN`), a closure has no structure to compare, and an
//! array, a dictionary, a tuple and an optional are not key types in
//! `docs/user/types.md`.

use super::super::module_resolver::ModuleResolver;
use super::super::sem_type::SemType;
use super::super::SemanticAnalyzer;
use crate::ast::PrimitiveType;

impl<R: ModuleResolver> SemanticAnalyzer<R> {
    /// True when `key` can be a dictionary key.
    ///
    /// A name that is not a struct or an enum here, for example a type
    /// parameter or an import, is accepted: its own check applies where
    /// it is known.
    pub(in crate::semantic) fn is_key_type(&self, key: &SemType) -> bool {
        let mut seen = Vec::new();
        self.is_key_type_inner(key, &mut seen)
    }

    fn is_key_type_inner(&self, key: &SemType, seen: &mut Vec<String>) -> bool {
        let name = match key {
            SemType::Primitive(p) => {
                return matches!(
                    p,
                    PrimitiveType::String
                        | PrimitiveType::I32
                        | PrimitiveType::I64
                        | PrimitiveType::Boolean
                );
            }
            SemType::Unknown | SemType::InferredEnum => return true,
            SemType::Array(_)
            | SemType::Optional(_)
            | SemType::Tuple(_)
            | SemType::Dictionary { .. }
            | SemType::Closure { .. }
            | SemType::Nil => return false,
            SemType::Named(name) | SemType::Generic { base: name, .. } => name,
        };
        // A type that holds itself is checked once.
        if seen.contains(name) {
            return true;
        }
        seen.push(name.clone());
        let fields: Vec<SemType> = if let Some(info) = self.symbols.get_struct_qualified(name) {
            let generics: Vec<String> = info.generics.iter().map(|g| g.name.name.clone()).collect();
            info.fields
                .iter()
                .filter(|f| {
                    !super::super::validation::type_names::type_mentions_any(&f.ty, &generics)
                })
                .map(|f| SemType::from_ast(&f.ty))
                .collect()
        } else if let Some(info) = self.symbols.get_enum_qualified(name) {
            let generics: Vec<String> = info.generics.iter().map(|g| g.name.name.clone()).collect();
            info.variant_fields
                .values()
                .flatten()
                .filter(|f| {
                    !super::super::validation::type_names::type_mentions_any(&f.ty, &generics)
                })
                .map(|f| SemType::from_ast(&f.ty))
                .collect()
        } else {
            return true;
        };
        fields.iter().all(|f| self.is_key_type_inner(f, seen))
    }
}
