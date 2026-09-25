//! IR-level source span: AST byte range + file identity.
//!
//! The AST's [`crate::location::Span`] is single-file (byte / line /
//! column only). The IR can hold definitions and expressions that
//! originate from multiple source files (the linker copies each
//! imported module into the module that imports it), so it wraps the
//! AST span with a `FileId` indexing into
//! [`crate::ir::IrModule::file_table`].
//!
//! `FileId(0)` is reserved for synthetic / unknown nodes — closure-
//! converted lift wrappers, monomorphised specialisations, and any
//! IR constructed without a known source file (e.g. test fixtures
//! using `IrSpan::default()`). Real source files start at `FileId(1)`.

/// Index into [`crate::ir::IrModule::file_table`]. `FileId(0)` is
/// reserved for synthetic / unknown nodes.
#[expect(clippy::exhaustive_structs, reason = "public id wrapper")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FileId(pub u32);

impl FileId {
    /// The reserved id for synthetic / unknown source.
    pub const SYNTHETIC: Self = Self(0);

    /// True if this file id is the synthetic / unknown sentinel.
    #[must_use]
    pub const fn is_synthetic(&self) -> bool {
        self.0 == 0
    }
}

/// IR-level source span: a single-file AST span paired with the file identity.
///
/// Backends emit DWARF / source-map entries by reading the `span` (byte range
/// + line / column) and resolving `file` against `IrModule.file_table`.
#[expect(clippy::exhaustive_structs, reason = "public IR shape")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrSpan {
    /// Byte range + line / column within the source file.
    pub span: crate::location::Span,
    /// The originating source file (or [`FileId::SYNTHETIC`] when no
    /// real file applies).
    pub file: FileId,
}

impl IrSpan {
    /// Construct an [`IrSpan`] with the given AST span and file id.
    #[must_use]
    pub const fn new(span: crate::location::Span, file: FileId) -> Self {
        Self { span, file }
    }

    /// True when both the AST span and the file id are at their
    /// defaults — i.e. the IR node was constructed without a known
    /// source location. With the `serde` feature, serde leaves out a
    /// span that is at its default (see `no_span`).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.span == crate::location::Span::default() && self.file.is_synthetic()
    }
}

/// The `skip_serializing_if` predicate of each IR `span` field. Serde
/// leaves out a span that is at its default, so synthetic IR does not
/// make the JSON larger. The short name keeps each attribute on one
/// line.
#[cfg(feature = "serde")]
pub(crate) fn no_span(span: &IrSpan) -> bool {
    span.is_default()
}
