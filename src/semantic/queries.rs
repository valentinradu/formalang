//! Semantic queries for LSP features
//!
//! This module provides high-level query APIs for IDE features like completion,
//! hover, go-to-definition, etc. These queries use the SemanticAnalyzer's symbol
//! table and the AST node finder to provide context-aware information.

use crate::ast::*;
use crate::location::Span;
use super::symbol_table::{SymbolTable, SymbolKind, TraitInfo, StructInfo, EnumInfo, LetInfo};

/// Kind of completion item
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    ModelTrait,
    ViewTrait,
    Model,
    View,
    Enum,
    Field,
    EnumVariant,
    LetBinding,
    PrimitiveType,
}

/// A completion candidate
#[derive(Debug, Clone)]
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

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Information about a symbol for hover
#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub symbol_name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub documentation: Option<String>,
    pub source_span: Span,
}

/// Information about a definition location
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    pub symbol_name: String,
    pub kind: SymbolKind,
    pub span: Span,
}

/// Query provider for semantic information
pub struct QueryProvider<'a> {
    pub symbols: &'a SymbolTable,
}

impl<'a> QueryProvider<'a> {
    /// Create a new query provider
    pub fn new(symbols: &'a SymbolTable) -> Self {
        Self { symbols }
    }

    /// Get all visible symbols as completions
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
        for (name, info) in &self.symbols.traits {
            let kind = if info.mount_fields.is_empty() {
                CompletionKind::ModelTrait
            } else {
                CompletionKind::ViewTrait
            };
            completions.push(CompletionCandidate::new(name.clone(), kind));
        }

        // Add structs (unified)
        for (name, info) in &self.symbols.structs {
            let kind = if info.mount_fields.is_empty() {
                CompletionKind::Model
            } else {
                CompletionKind::View
            };
            completions.push(CompletionCandidate::new(name.clone(), kind));
        }

        // Add enums
        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

        // Add let bindings
        for name in self.symbols.lets.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::LetBinding));
        }

        completions
    }

    /// Get type completions (types that can be used in type positions)
    pub fn get_type_completions(&self) -> Vec<CompletionCandidate> {
        let mut completions = vec![
            CompletionCandidate::new("String", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Number", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Boolean", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Path", CompletionKind::PrimitiveType),
            CompletionCandidate::new("Regex", CompletionKind::PrimitiveType),
        ];

        // Add structs (they can be used as types)
        for (name, info) in &self.symbols.structs {
            let kind = if info.mount_fields.is_empty() {
                CompletionKind::Model
            } else {
                CompletionKind::View
            };
            completions.push(CompletionCandidate::new(name.clone(), kind));
        }

        // Add enums
        for name in self.symbols.enums.keys() {
            completions.push(CompletionCandidate::new(name.clone(), CompletionKind::Enum));
        }

        // Add traits (can be used as type constraints)
        for (name, info) in &self.symbols.traits {
            let kind = if info.mount_fields.is_empty() {
                CompletionKind::ModelTrait
            } else {
                CompletionKind::ViewTrait
            };
            completions.push(CompletionCandidate::new(name.clone(), kind));
        }

        completions
    }

    /// Get hover info for a symbol by name
    pub fn get_hover_for_symbol(&self, name: &str) -> Option<HoverInfo> {
        // Check traits
        if let Some(info) = self.symbols.traits.get(name) {
            let kind = SymbolKind::Trait;
            return Some(self.trait_info_to_hover(name, info, kind));
        }

        // Check structs
        if let Some(info) = self.symbols.structs.get(name) {
            return Some(self.struct_info_to_hover(name, info));
        }

        // Check enums
        if let Some(info) = self.symbols.enums.get(name) {
            return Some(self.enum_info_to_hover(name, info));
        }

        // Check let bindings
        if let Some(info) = self.symbols.lets.get(name) {
            return Some(self.let_info_to_hover(name, info));
        }

        None
    }

    /// Find definition location for a symbol by name
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

        None
    }

    // ========== Helper Methods ==========

    fn trait_info_to_hover(&self, name: &str, info: &TraitInfo, kind: SymbolKind) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) { "pub " } else { "" };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", info.generics.iter()
                .map(|g| g.name.name.clone())
                .collect::<Vec<_>>()
                .join(", "))
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind,
            signature: format!("{}trait {}{}", vis, name, generics),
            documentation: None,
            source_span: info.span,
        }
    }

    fn struct_info_to_hover(&self, name: &str, info: &StructInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) { "pub " } else { "" };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", info.generics.iter()
                .map(|g| g.name.name.clone())
                .collect::<Vec<_>>()
                .join(", "))
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Struct,
            signature: format!("{}struct {}{}", vis, name, generics),
            documentation: None,
            source_span: info.span,
        }
    }

    fn enum_info_to_hover(&self, name: &str, info: &EnumInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) { "pub " } else { "" };

        let generics = if info.generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", info.generics.iter()
                .map(|g| g.name.name.clone())
                .collect::<Vec<_>>()
                .join(", "))
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Enum,
            signature: format!("{}enum {}{}", vis, name, generics),
            documentation: None,
            source_span: info.span,
        }
    }

    fn let_info_to_hover(&self, name: &str, info: &LetInfo) -> HoverInfo {
        let vis = if matches!(info.visibility, Visibility::Public) { "pub " } else { "" };

        let signature = if let Some(ty) = &info.inferred_type {
            format!("{}let {}: {}", vis, name, ty)
        } else {
            format!("{}let {}", vis, name)
        };

        HoverInfo {
            symbol_name: name.to_string(),
            kind: SymbolKind::Let,
            signature,
            documentation: None,
            source_span: info.span,
        }
    }
}

