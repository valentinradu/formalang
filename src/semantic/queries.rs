//! Semantic queries for LSP features
//!
//! This module provides high-level query APIs for IDE features like completion,
//! hover, go-to-definition, etc. These queries use the `SemanticAnalyzer`'s symbol
//! table and the AST node finder to provide context-aware information.

use super::symbol_table::{EnumInfo, LetInfo, StructInfo, SymbolKind, SymbolTable, TraitInfo};
use crate::ast::Visibility;
use crate::location::Span;
use std::path::PathBuf;

/// Kind of completion item. `View` / `ViewTrait` were vestiges of the
/// pre-"unified struct" design and were never produced by the completion
/// code — removed per audit finding #43.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompletionKind {
    Keyword,
    ModelTrait,
    Model,
    Enum,
    Field,
    EnumVariant,
    LetBinding,
    PrimitiveType,
}

/// A completion candidate
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompletionCandidate {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub documentation: Option<String>,
}

impl CompletionCandidate {
    pub fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            insert_text: None,
            documentation: None,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Information about a symbol for hover
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HoverInfo {
    pub symbol_name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub documentation: Option<String>,
    pub source_span: Span,
}

/// Information about a definition location
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DefinitionInfo {
    pub symbol_name: String,
    pub kind: SymbolKind,
    pub span: Span,
}

/// Query provider for semantic information
#[derive(Debug)]
#[non_exhaustive]
pub struct QueryProvider<'a> {
    pub symbols: &'a SymbolTable,
    /// Optional cache of imported modules, used to extend hover,
    /// go-to-definition, and completion across module boundaries.
    pub module_cache:
        Option<&'a std::collections::HashMap<PathBuf, (crate::ast::File, SymbolTable)>>,
}

impl<'a> QueryProvider<'a> {
    /// Create a new query provider bound to a single-file symbol table.
    /// For multi-file projects use [`QueryProvider::with_modules`].
    #[must_use]
    pub const fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            module_cache: None,
        }
    }

    /// Create a query provider that also searches an imported-module cache
    /// when resolving hover / go-to-definition / completion candidates.
    #[must_use]
    pub const fn with_modules(
        symbols: &'a SymbolTable,
        module_cache: &'a std::collections::HashMap<PathBuf, (crate::ast::File, SymbolTable)>,
    ) -> Self {
        Self {
            symbols,
            module_cache: Some(module_cache),
        }
    }

    /// Get all visible symbols as completions
    #[must_use]
    pub fn get_all_completions(&self) -> Vec<CompletionCandidate> {
        let mut completions = vec![
            CompletionCandidate::new("pub", CompletionKind::Keyword),
            CompletionCandidate::new("use", CompletionKind::Keyword),
            CompletionCandidate::new("let", CompletionKind::Keyword),
            CompletionCandidate::new("model", CompletionKind::Keyword),
            CompletionCandidate::new("view", CompletionKind::Keyword),
            CompletionCandidate::new("trait", CompletionKind::Keyword),
            CompletionCandidate::new("enum", CompletionKind::Keyword),
        ];

        // Add traits (unified)
        for name in self.symbols.traits.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::ModelTrait,
            ));
        }

        // Add structs (unified)
        for name in self.symbols.structs.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::Model,
            ));
        }

        // Add enums
        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

        // Add let bindings
        for name in self.symbols.lets.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::LetBinding,
            ));
        }

        completions
    }

    /// Get type completions (types that can be used in type positions)
    #[must_use]
    pub fn get_type_completions(&self) -> Vec<CompletionCandidate> {
        let mut completions = vec![
            CompletionCandidate::new("String", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Number", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Boolean", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Path", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Regex", CompletionKind::PrimitiveType),
        ];

        // Add structs (they can be used as types)
        for name in self.symbols.structs.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::Model,
            ));
        }

        // Add enums
        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

        // Add traits (can be used as type constraints)
        for name in self.symbols.traits.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::ModelTrait,
            ));
        }

        completions
    }

    /// Get hover info for a symbol by name
    #[must_use]
    pub fn get_hover_for_symbol(&self, name: &str) -> Option<HoverInfo> {
        // Check traits
        if let Some(info) = self.symbols.traits.get(name) {
            let kind = SymbolKind::Trait;
            return Some(Self::trait_info_to_hover(name, info, kind));
        }

        // Check structs
        if let Some(info) = self.symbols.structs.get(name) {
            return Some(Self::struct_info_to_hover(name, info));
        }

        // Check enums
        if let Some(info) = self.symbols.enums.get(name) {
            return Some(Self::enum_info_to_hover(name, info));
        }

        // Check let bindings
        if let Some(info) = self.symbols.lets.get(name) {
            return Some(Self::let_info_to_hover(name, info));
        }

        // Cross-module: search the imported-module cache when provided.
        if let Some(cache) = self.module_cache {
            for (_, symbols) in cache.values() {
                if let Some(info) = symbols.traits.get(name) {
                    return Some(Self::trait_info_to_hover(name, info, SymbolKind::Trait));
                }
                if let Some(info) = symbols.structs.get(name) {
                    return Some(Self::struct_info_to_hover(name, info));
                }
                if let Some(info) = symbols.enums.get(name) {
                    return Some(Self::enum_info_to_hover(name, info));
                }
                if let Some(info) = symbols.lets.get(name) {
                    return Some(Self::let_info_to_hover(name, info));
                }
            }
        }

        None
    }

    /// Find definition location for a symbol by name
    #[must_use]
    pub fn find_definition_by_name(&self, name: &str) -> Option<DefinitionInfo> {
        // Check traits
        if let Some(info) = self.symbols.traits.get(name) {
            return Some(DefinitionInfo {
                symbol_name: name.to_string(),
                kind: SymbolKind::Trait,
                span: info.span,
            });
        }

        // Check structs
        if let Some(info) = self.symbols.structs.get(name) {
            return Some(DefinitionInfo {
                symbol_name: name.to_string(),
                kind: SymbolKind::Struct,
                span: info.span,
            });
        }

        // Check enums
        if let Some(info) = self.symbols.enums.get(name) {
            return Some(DefinitionInfo {
                symbol_name: name.to_string(),
                kind: SymbolKind::Enum,
                span: info.span,
            });
        }

        // Check let bindings
        if let Some(info) = self.symbols.lets.get(name) {
            return Some(DefinitionInfo {
                symbol_name: name.to_string(),
                kind: SymbolKind::Let,
                span: info.span,
            });
        }

        // Cross-module: search the imported-module cache if provided.
        if let Some(cache) = self.module_cache {
            for (_, symbols) in cache.values() {
                if let Some(info) = symbols.traits.get(name) {
                    return Some(DefinitionInfo {
                        symbol_name: name.to_string(),
                        kind: SymbolKind::Trait,
                        span: info.span,
                    });
                }
                if let Some(info) = symbols.structs.get(name) {
                    return Some(DefinitionInfo {
                        symbol_name: name.to_string(),
                        kind: SymbolKind::Struct,
                        span: info.span,
                    });
                }
                if let Some(info) = symbols.enums.get(name) {
                    return Some(DefinitionInfo {
                        symbol_name: name.to_string(),
                        kind: SymbolKind::Enum,
                        span: info.span,
                    });
                }
                if let Some(info) = symbols.lets.get(name) {
                    return Some(DefinitionInfo {
                        symbol_name: name.to_string(),
                        kind: SymbolKind::Let,
                        span: info.span,
                    });
                }
            }
        }

        None
    }

    // ========== Helper Methods ==========

    fn trait_info_to_hover(name: &str, info: &TraitInfo, kind: SymbolKind) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) {
            "pub "
        } else {
            ""
        };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                info.generics
                    .iter()
                    .map(|g| g.name.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind,
            signature: format!("{vis}trait {name}{generics}"),
            documentation: None,
            source_span: info.span,
        }
    }

    fn struct_info_to_hover(name: &str, info: &StructInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) {
            "pub "
        } else {
            ""
        };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                info.generics
                    .iter()
                    .map(|g| g.name.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Struct,
            signature: format!("{vis}struct {name}{generics}"),
            documentation: None,
            source_span: info.span,
        }
    }

    fn enum_info_to_hover(name: &str, info: &EnumInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) {
            "pub "
        } else {
            ""
        };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                info.generics
                    .iter()
                    .map(|g| g.name.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Enum,
            signature: format!("{vis}enum {name}{generics}"),
            documentation: None,
            source_span: info.span,
        }
    }

    fn let_info_to_hover(name: &str, info: &LetInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) {
            "pub "
        } else {
            ""
        };

        let signature = info.inferred_type.as_ref().map_or_else(
            || format!("{vis}let {name}"),
            |ty| format!("{vis}let {name}: {ty}"),
        );

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Let,
            signature,
            documentation: None,
            source_span: info.span,
        }
    }
}
