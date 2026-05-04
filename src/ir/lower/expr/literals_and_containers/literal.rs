//! Mapping from AST `Literal` kinds onto their IR primitive types.

use crate::ast::{Literal, PrimitiveType};
use crate::ir::lower::IrLowerer;
use crate::ir::ResolvedType;

impl IrLowerer<'_> {
    pub(in crate::ir::lower::expr) fn literal_type(&self, lit: &Literal) -> ResolvedType {
        match lit {
            Literal::String(_) => ResolvedType::Primitive(PrimitiveType::String),
            Literal::Number(n) => ResolvedType::Primitive(n.primitive_type()),
            Literal::Boolean(_) => ResolvedType::Primitive(PrimitiveType::Boolean),
            Literal::Path(_) => ResolvedType::Primitive(PrimitiveType::Path),
            Literal::Regex { .. } => ResolvedType::Primitive(PrimitiveType::Regex),
            // `nil` is the zero value of every optional type. Modelled as
            // `Optional<Never>` against the prelude enum — backends
            // destructure this as "missing value, no payload" and
            // assignments to `T?` widen through the normal generic
            // matching path.
            Literal::Nil => self
                .optional_of(ResolvedType::Primitive(PrimitiveType::Never))
                .unwrap_or(ResolvedType::Error),
        }
    }
}
