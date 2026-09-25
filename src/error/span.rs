//! Where each error points in the source.
//!
//! Split out of `mod.rs` to keep each file under the line ceiling that
//! `scripts/check_file_sizes.sh` enforces.

use super::CompilerError;
use crate::location::Span;

impl CompilerError {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::InvalidCharacter { span, .. }
            | Self::UnterminatedString { span }
            | Self::UnterminatedBlockComment { span }
            | Self::InvalidUnicodeEscape { span, .. }
            | Self::InvalidEscape { span, .. }
            | Self::BidirectionalControl { span, .. }
            | Self::InvalidNumber { span, .. }
            | Self::UnexpectedToken { span, .. }
            | Self::UnexpectedEof { span }
            | Self::UndefinedReference { span, .. }
            | Self::TypeMismatch { span, .. }
            | Self::DuplicateDefinition { span, .. }
            | Self::ModuleNotFound { span, .. }
            | Self::AmbiguousModulePath { span, .. }
            | Self::ModuleReadError { span, .. }
            | Self::CircularImport { span, .. }
            | Self::PrivateImport { span, .. }
            | Self::ImportItemNotFound { span, .. }
            | Self::ParseError { span, .. }
            | Self::UndefinedType { span, .. }
            | Self::PrimitiveRedefinition { span, .. }
            | Self::ImplOnPrimitive { span, .. }
            | Self::TraitUsedAsValueType { span, .. }
            | Self::UndefinedTrait { span, .. }
            | Self::NotATrait { span, .. }
            | Self::MissingTraitField { span, .. }
            | Self::TraitFieldTypeMismatch { span, .. }
            | Self::CircularDependency { span, .. }
            | Self::InvalidBinaryOp { span, .. }
            | Self::ForLoopNotArray { span, .. }
            | Self::ArrayDestructuringNotArray { span, .. }
            | Self::StructDestructuringNotStruct { span, .. }
            | Self::InvalidIfCondition { span, .. }
            | Self::MatchNotEnum { span, .. }
            | Self::NonExhaustiveMatch { span, .. }
            | Self::DuplicateMatchArm { span, .. }
            | Self::UnreachableMatchArm { span, .. }
            | Self::UnknownEnumVariant { span, .. }
            | Self::PointlessOptionalElement { span, .. }
            | Self::PrivateTypeInPublic { span, .. }
            | Self::NotIndexable { span, .. }
            | Self::ArgumentCountMismatch { span, .. }
            | Self::VariantArityMismatch { span, .. }
            | Self::MissingField { span, .. }
            | Self::UnknownField { span, .. }
            | Self::PositionalArgInStruct { span, .. }
            | Self::EnumVariantWithoutData { span, .. }
            | Self::EnumVariantRequiresData { span, .. }
            | Self::MutabilityMismatch { span, .. }
            | Self::GenericArityMismatch { span, .. }
            | Self::GenericConstraintViolation { span, .. }
            | Self::OutOfScopeTypeParameter { span, .. }
            | Self::MissingGenericArguments { span, .. }
            | Self::DuplicateGenericParam { span, .. }
            | Self::GenericTraitMethod { span, .. }
            | Self::UninferableMethodTypeParameter { span, .. }
            | Self::ExternFnWithBody { span, .. }
            | Self::RegularFnWithoutBody { span, .. }
            | Self::ExternImplWithBody { span, .. }
            | Self::RequiredParamAfterDefault { span, .. }
            | Self::NilAssignedToNonOptional { span, .. }
            | Self::OptionalUsedAsNonOptional { span, .. }
            | Self::MissingTraitMethod { span, .. }
            | Self::TraitMethodSignatureMismatch { span, .. }
            | Self::AmbiguousCall { span, .. }
            | Self::NoMatchingOverload { span, .. }
            | Self::CannotInferEnumType { span, .. }
            | Self::ClosureParameterNeedsType { span, .. }
            | Self::InvalidDictionaryKey { span, .. }
            | Self::SeqNotConsumed { span }
            | Self::SeqUsedTwice { span, .. }
            | Self::SeqInvalidPosition { span, .. }
            | Self::FunctionReturnTypeMismatch { span, .. }
            | Self::AssignmentToImmutable { span, .. }
            | Self::AssignmentToElement { span, .. }
            | Self::OverlappingArguments { span, .. }
            | Self::UseAfterSink { span, .. }
            | Self::ExpressionDepthExceeded { span }
            | Self::InstantiationDepthExceeded { span, .. }
            | Self::TooManyDefinitions { span, .. }
            | Self::VisibilityViolation { span, .. }
            | Self::InternalError { span, .. }
            | Self::NumericOverflow { span, .. }
            | Self::PublicClosureField { span, .. }
            | Self::LabelledClosureArgument { span, .. }
            | Self::NotAStaticMethod { span, .. } => *span,
        }
    }
}
