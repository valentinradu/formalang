//! Semantic queries for LSP features
//!
//! High-level query APIs for IDE features like completion, hover, and
//! go-to-definition. Backed by the `SemanticAnalyzer`'s symbol table.

mod hover_format;

use super::symbol_table::{SymbolKind, SymbolTable};
use crate::location::Span;
use std::path::PathBuf;

/// Kind of completion item.
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

        for name in self.symbols.traits.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::ModelTrait,
            ));
        }

        for name in self.symbols.structs.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::Model,
            ));
        }

        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

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
            CompletionCandidate::new("I32", CompletionKind::PrimitiveType),
            CompletionCandidate::new("I64", CompletionKind::PrimitiveType),
            CompletionCandidate::new("F32", CompletionKind::PrimitiveType),
            CompletionCandidate::new("F64", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Boolean", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Path", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Regex", CompletionKind::PrimitiveType),
        ];

        for name in self.symbols.structs.keys() {
            completions.push(CompletionCandidate::new(
                name.clone(),
                CompletionKind::Model,
            ));
        }

        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

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
        if let Some(info) = self.symbols.traits.get(name) {
            return Some(hover_format::trait_info_to_hover(
                name,
                info,
                SymbolKind::Trait,
            ));
        }
        if let Some(info) = self.symbols.structs.get(name) {
            return Some(hover_format::struct_info_to_hover(name, info));
        }
        if let Some(info) = self.symbols.enums.get(name) {
            return Some(hover_format::enum_info_to_hover(name, info));
        }
        if let Some(info) = self.symbols.lets.get(name) {
            return Some(hover_format::let_info_to_hover(name, info));
        }
        if let Some(info) = self.symbols.get_function(name) {
            return Some(hover_format::function_info_to_hover(name, info));
        }

        // Cross-module: search the imported-module cache when provided.
        if let Some(cache) = self.module_cache {
            for (_, symbols) in cache.values() {
                if let Some(info) = symbols.traits.get(name) {
                    return Some(hover_format::trait_info_to_hover(
                        name,
                        info,
                        SymbolKind::Trait,
                    ));
                }
                if let Some(info) = symbols.structs.get(name) {
                    return Some(hover_format::struct_info_to_hover(name, info));
                }
                if let Some(info) = symbols.enums.get(name) {
                    return Some(hover_format::enum_info_to_hover(name, info));
                }
                if let Some(info) = symbols.lets.get(name) {
                    return Some(hover_format::let_info_to_hover(name, info));
                }
                if let Some(info) = symbols.get_function(name) {
                    return Some(hover_format::function_info_to_hover(name, info));
                }
            }
        }

        None
    }

    /// Find definition location for a symbol by name
    #[must_use]
    pub fn find_definition_by_name(&self, name: &str) -> Option<DefinitionInfo> {
        let local = self
            .symbols
            .traits
            .get(name)
            .map(|i| (SymbolKind::Trait, i.span))
            .or_else(|| {
                self.symbols
                    .structs
                    .get(name)
                    .map(|i| (SymbolKind::Struct, i.span))
            })
            .or_else(|| {
                self.symbols
                    .enums
                    .get(name)
                    .map(|i| (SymbolKind::Enum, i.span))
            })
            .or_else(|| {
                self.symbols
                    .lets
                    .get(name)
                    .map(|i| (SymbolKind::Let, i.span))
            })
            .or_else(|| {
                self.symbols
                    .get_function(name)
                    .map(|i| (SymbolKind::Function, i.span))
            });
        if let Some((kind, span)) = local {
            return Some(DefinitionInfo {
                symbol_name: name.to_string(),
                kind,
                span,
            });
        }

        // Cross-module: search the imported-module cache if provided.
        if let Some(cache) = self.module_cache {
            for (_, symbols) in cache.values() {
                let hit = symbols
                    .traits
                    .get(name)
                    .map(|i| (SymbolKind::Trait, i.span))
                    .or_else(|| {
                        symbols
                            .structs
                            .get(name)
                            .map(|i| (SymbolKind::Struct, i.span))
                    })
                    .or_else(|| symbols.enums.get(name).map(|i| (SymbolKind::Enum, i.span)))
                    .or_else(|| symbols.lets.get(name).map(|i| (SymbolKind::Let, i.span)))
                    .or_else(|| {
                        symbols
                            .get_function(name)
                            .map(|i| (SymbolKind::Function, i.span))
                    });
                if let Some((kind, span)) = hit {
                    return Some(DefinitionInfo {
                        symbol_name: name.to_string(),
                        kind,
                        span,
                    });
                }
            }
        }

        None
    }
}
