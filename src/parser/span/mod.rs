//! Post-process the AST to fill line/column info for spans the parser only
//! recorded as byte offsets.

mod defs;
mod exprs;

use defs::{fill_definition_span, fill_type_span};
use exprs::{fill_binding_pattern_span, fill_expr_span};

use crate::ast::{File, Statement, UseItems};
use crate::location::{LineIndex, Span as CustomSpan};

/// Fill in line/column information for all spans in the AST using source text
///
/// Takes a [`LineIndex`] rather than the source text. Resolving each
/// span from a `&str` builds a throwaway index, and there is one span
/// per AST node, so that made parsing cost
/// `O(nodes * source length)` — the same defect the lexer had. A file
/// of 1024 structs took 296 ms to parse, almost all of it here.
pub(super) fn fill_file_spans(file: &mut File, index: &LineIndex<'_>) {
    for stmt in &mut file.statements {
        fill_statement_span(stmt, index);
    }
}

/// Lift `(start.offset, end.offset)` into a fully-resolved `Span` only when
/// the line/column fields are still zero (i.e. parser never set them).
pub(super) fn fill_span(span: &mut CustomSpan, index: &LineIndex<'_>) {
    if span.start.line == 0 && span.end.line == 0 {
        *span = index.fill_span(*span);
    }
}

fn fill_statement_span(stmt: &mut Statement, index: &LineIndex<'_>) {
    match stmt {
        Statement::Use(use_stmt) => {
            for ident in &mut use_stmt.path {
                fill_span(&mut ident.span, index);
            }
            match &mut use_stmt.items {
                UseItems::Single(ident) => fill_span(&mut ident.span, index),
                UseItems::Multiple(idents) => {
                    for ident in idents {
                        fill_span(&mut ident.span, index);
                    }
                }
                UseItems::Glob => {}
            }
            fill_span(&mut use_stmt.span, index);
        }
        Statement::Let(let_stmt) => {
            fill_binding_pattern_span(&mut let_stmt.pattern, index);
            if let Some(type_ann) = &mut let_stmt.type_annotation {
                fill_type_span(type_ann, index);
            }
            fill_expr_span(&mut let_stmt.value, index);
            fill_span(&mut let_stmt.span, index);
        }
        Statement::Definition(def) => fill_definition_span(def.as_mut(), index),
    }
}
