//! Numeric-literal AST types: [`NumberLiteral`], [`NumberValue`],
//! [`NumberSourceKind`], [`NumericSuffix`].
//!
//! Split out of [`super::expressions`] to keep that file under the 500-LOC
//! ceiling. Re-exported from [`crate::ast`].

use crate::ast::PrimitiveType;
use serde::{Deserialize, Serialize};

/// Whether a numeric literal was written with integer or float syntax.
///
/// The lexer sets this based on the presence of `.` or `e` in the digit
/// slice — `42` is integer, `42.0` and `1e5` are float. Used to pick the
/// inference default for unsuffixed literals (`I32` for integer, `F64`
/// for float).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumberSourceKind {
    Integer,
    Float,
}

impl NumberSourceKind {
    /// Default [`PrimitiveType`] for an unsuffixed literal of this kind.
    #[must_use]
    pub const fn default_primitive(self) -> PrimitiveType {
        match self {
            Self::Integer => PrimitiveType::I32,
            Self::Float => PrimitiveType::F64,
        }
    }
}

/// Width-tagged suffix attached to a numeric literal at parse time.
///
/// Source spelling is uppercase and adjacent to the digits (e.g. `42I32`,
/// `3.14F64`). The suffix is preserved through the AST so later passes can
/// type the literal without re-running inference defaults.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumericSuffix {
    I32,
    I64,
    F32,
    F64,
}

impl NumericSuffix {
    /// The [`PrimitiveType`] this suffix designates.
    #[must_use]
    pub const fn primitive(self) -> PrimitiveType {
        match self {
            Self::I32 => PrimitiveType::I32,
            Self::I64 => PrimitiveType::I64,
            Self::F32 => PrimitiveType::F32,
            Self::F64 => PrimitiveType::F64,
        }
    }
}

/// Discriminated payload for a numeric literal.
///
/// Integer-syntax literals (`42`, `9_223_372_036_854_775_807I64`) preserve the
/// exact digits as `i128` so backends emitting native integer instructions
/// (wasm `i64.const`, JVM `ldc`, native `mov $imm, %rax`) round-trip without
/// loss. Float-syntax literals (`3.14`, `1e5`) use `f64`, matching their IEEE
/// 754 representation in source.
///
/// The `i128` arm comfortably covers `i64::MIN..=u64::MAX`; suffix range
/// checks happen at semantic-analysis time, so by the time codegen runs the
/// payload fits the target primitive.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NumberValue {
    /// Lexed from integer syntax. Preserves the exact digits.
    Integer(i128),
    /// Lexed from float syntax (digits include `.` or `e`).
    Float(f64),
}

impl NumberValue {
    /// Convert to `i32` if the integer value fits in range, else `None`.
    /// Float-syntax payloads return `None` (semantic analysis rejects
    /// float-typed literals where an integer was expected).
    #[must_use]
    pub fn as_i32(&self) -> Option<i32> {
        match *self {
            Self::Integer(v) => i32::try_from(v).ok(),
            Self::Float(_) => None,
        }
    }

    /// Convert to `i64` if the integer value fits in range, else `None`.
    /// Float-syntax payloads return `None`.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Self::Integer(v) => i64::try_from(v).ok(),
            Self::Float(_) => None,
        }
    }

    /// Best-effort cast to `f32`. Integer payloads cast via `as`; large
    /// magnitudes lose precision (existing behaviour). Float payloads are
    /// truncated from `f64`.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "best-effort backend cast; range-check happened at semantic time"
    )]
    pub const fn as_f32(&self) -> f32 {
        match *self {
            Self::Integer(v) => v as f32,
            Self::Float(f) => f as f32,
        }
    }

    /// Best-effort cast to `f64`. Integer payloads above `2^53` lose
    /// precision in the cast. Float payloads pass through.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "i128 → f64 may lose precision above 2^53; semantic gates this when it matters"
    )]
    pub const fn as_f64(&self) -> f64 {
        match *self {
            Self::Integer(v) => v as f64,
            Self::Float(f) => f,
        }
    }
}

impl From<f64> for NumberValue {
    /// `Integer(v as i128)` for finite whole-number values; `Float(v)`
    /// otherwise. Preserves the existing `From<f64> for NumberLiteral`
    /// heuristic.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "finite whole-number f64 fits i128 modulo magnitudes beyond 2^127, which the From<f64> heuristic does not promise to preserve exactly"
    )]
    fn from(value: f64) -> Self {
        if value.is_finite() && value.fract() == 0.0 {
            Self::Integer(value as i128)
        } else {
            Self::Float(value)
        }
    }
}

impl From<i128> for NumberValue {
    fn from(value: i128) -> Self {
        Self::Integer(value)
    }
}

/// Parsed payload of a numeric literal.
///
/// Carries a discriminated [`NumberValue`] (preserving exact integer digits
/// or `f64` float bits), an optional source-level type suffix, and the
/// integer-vs-float source-syntax kind. A single field type that both
/// `lexer::Token::Number` and `Literal::Number` wrap — single-field because
/// logos (used by the lexer) only supports single-field token variants.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NumberLiteral {
    pub value: NumberValue,
    pub suffix: Option<NumericSuffix>,
    pub kind: NumberSourceKind,
}

impl NumberLiteral {
    /// Construct an integer-syntax literal with no suffix.
    #[must_use]
    pub const fn unsuffixed(value: i128) -> Self {
        Self {
            value: NumberValue::Integer(value),
            suffix: None,
            kind: NumberSourceKind::Integer,
        }
    }

    /// Construct a float-syntax literal with no suffix.
    #[must_use]
    pub const fn unsuffixed_float(value: f64) -> Self {
        Self {
            value: NumberValue::Float(value),
            suffix: None,
            kind: NumberSourceKind::Float,
        }
    }

    /// Construct with an explicit suffix. The source kind is inferred from
    /// the suffix's family (`I32`/`I64` → Integer, `F32`/`F64` → Float).
    #[must_use]
    pub const fn suffixed(value: NumberValue, suffix: NumericSuffix) -> Self {
        let kind = match suffix {
            NumericSuffix::I32 | NumericSuffix::I64 => NumberSourceKind::Integer,
            NumericSuffix::F32 | NumericSuffix::F64 => NumberSourceKind::Float,
        };
        Self {
            value,
            suffix: Some(suffix),
            kind,
        }
    }

    /// Construct with the suffix already wrapped in an `Option` and an
    /// explicit source kind — convenience for the lexer, which determines
    /// both fields independently from the literal slice.
    #[must_use]
    pub const fn from_lex(
        value: NumberValue,
        suffix: Option<NumericSuffix>,
        kind: NumberSourceKind,
    ) -> Self {
        Self {
            value,
            suffix,
            kind,
        }
    }

    /// The [`PrimitiveType`] this literal carries.
    ///
    /// When a suffix is present, that wins. Otherwise the source kind picks
    /// the default: `Integer` → [`PrimitiveType::I32`],
    /// `Float` → [`PrimitiveType::F64`].
    #[must_use]
    pub fn primitive_type(&self) -> PrimitiveType {
        self.suffix
            .map_or_else(|| self.kind.default_primitive(), NumericSuffix::primitive)
    }
}

impl From<f64> for NumberLiteral {
    /// Default conversion infers the source kind from whether `value` has a
    /// fractional part — finite whole-number values get `Integer`, anything
    /// else (including `NaN` / infinities) gets `Float`. Convenient for tests
    /// and IR-internal construction; suffix is always `None`.
    fn from(value: f64) -> Self {
        let kind = if value.is_finite() && value.fract() == 0.0 {
            NumberSourceKind::Integer
        } else {
            NumberSourceKind::Float
        };
        Self {
            value: NumberValue::from(value),
            suffix: None,
            kind,
        }
    }
}

impl From<i128> for NumberLiteral {
    /// Construct an integer-syntax literal with no suffix from an `i128`.
    fn from(value: i128) -> Self {
        Self {
            value: NumberValue::Integer(value),
            suffix: None,
            kind: NumberSourceKind::Integer,
        }
    }
}
