//! Post-process the AST to fill line/column info for spans the parser only
//! recorded as byte offsets.

mod defs;
mod exprs;

use defs::fill_definition_span;
use exprs::{fill_binding_pattern_span, fill_expr_span};

use crate::ast::{File, Statement, UseItems};
use crate::location::Span as CustomSpan;

/// Fill in line/column information for all spans in the AST using source text
pub(super) fn fill_file_spans(file: &mut File, source: &str) {
    for stmt in &mut file.statements {
        fill_statement_span(stmt, source);
    }
}

/// Lift `(start.offset, end.offset)` into a fully-resolved `Span` only when
/// the line/column fields are still zero (i.e. parser never set them).
pub(super) fn fill_span(span: &mut CustomSpan, source: &str) {
    if span.start.line == 0 && span.end.line == 0 {
        *span = CustomSpan::from_range_with_source(span.start.offset, span.end.offset, source);
    }
}

fn fill_statement_span(stmt: &mut Statement, source: &str) {
    match stmt {
        Statement::Use(use_stmt) => {
            for ident in &mut use_stmt.path {
                fill_span(&mut ident.span, source);
            }
            match &mut use_stmt.items {
                UseItems::Single(ident) => fill_span(&mut ident.span, source),
                UseItems::Multiple(idents) => {
                    for ident in idents {
                        fill_span(&mut ident.span, source);
                    }
                }
                UseItems::Glob => {}
            }
            fill_span(&mut use_stmt.span, source);
        }
        Statement::Let(let_stmt) => {
            fill_binding_pattern_span(&mut let_stmt.pattern, source);
            fill_expr_span(&mut let_stmt.value, source);
            fill_span(&mut let_stmt.span, source);
        }
        Statement::Definition(def) => fill_definition_span(def.as_mut(), source),
    }
}
