//! Doc comments: `///` lines before an item and `//!` lines at the
//! start of a file or a `mod` body.

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::{Definition, Statement};
use crate::lexer::Token;

/// Consume zero or more leading `///` doc-comment lines and join them
/// with newlines. Returns `None` when no doc comments precede the next
/// item. An inner `//!` comment is not an item doc: see
/// [`inner_doc_comments_parser`].
pub(super) fn doc_comments_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Option<String>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! { Token::DocComment(s) => s }
        .repeated()
        .collect::<Vec<_>>()
        .map(|lines| join_doc_lines(&lines))
}

/// Consume zero or more `//!` doc-comment lines and join them with
/// newlines. They document the enclosing file or `mod`, so they are
/// valid only at the start of the file or of the `mod` body. Anywhere
/// else, an inner doc comment is a parse error.
pub(super) fn inner_doc_comments_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Option<String>, extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    select! { Token::InnerDocComment(s) => s }
        .repeated()
        .collect::<Vec<_>>()
        .map(|lines| join_doc_lines(&lines))
}

/// Join doc-comment lines with newlines, or give `None` for no lines.
fn join_doc_lines(lines: &[String]) -> Option<String> {
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Attach a captured doc-comment string to whichever AST node the
/// statement carries. `Use` statements don't currently support docs and
/// silently drop the captured text.
pub(super) fn attach_doc_to_statement(doc: Option<String>, stmt: Statement) -> Statement {
    let Some(doc) = doc else {
        return stmt;
    };
    match stmt {
        Statement::Let(mut lb) => {
            lb.doc = Some(doc);
            Statement::Let(lb)
        }
        Statement::Definition(def) => {
            Statement::Definition(Box::new(attach_doc_to_definition(doc, *def)))
        }
        Statement::Use(_) => stmt,
    }
}

fn attach_doc_to_definition(doc: String, def: Definition) -> Definition {
    match def {
        Definition::Function(mut f) => {
            f.doc = Some(doc);
            Definition::Function(f)
        }
        Definition::Struct(mut s) => {
            s.doc = Some(doc);
            Definition::Struct(s)
        }
        Definition::Trait(mut t) => {
            t.doc = Some(doc);
            Definition::Trait(t)
        }
        Definition::Enum(mut e) => {
            e.doc = Some(doc);
            Definition::Enum(e)
        }
        Definition::Impl(mut i) => {
            i.doc = Some(doc);
            Definition::Impl(i)
        }
        Definition::Module(mut m) => {
            // The outer `///` lines come first, then the inner `//!`
            // lines of the body.
            m.doc = Some(match m.doc.take() {
                Some(inner) => format!("{doc}\n{inner}"),
                None => doc,
            });
            Definition::Module(m)
        }
    }
}
