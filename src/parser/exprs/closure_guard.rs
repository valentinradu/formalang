//! A guard that tells whether a `(` starts a closure.
//!
//! A closure and a tuple both start with `(`, and both can start with
//! `(name: ...`. The closure parser comes first, so at each `(` it reads
//! the parameters and their types before it fails on a tuple. In a nest
//! of tuples, each level read the whole rest of the nest as types, and
//! the time grew with the square of the depth.
//!
//! A `(` starts a closure only when its matching `)` is followed by
//! `->`. The guard answers that question with a scan over the tokens.
//! One scan answers it for every `(` inside the group too, and a cache
//! keeps the answers, so the scans over a whole parse take linear time.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::lexer::Token;

/// The answers for one parse, keyed by the byte offset of each `(`.
///
/// A parser is built for each parse, so each parse has its own cache.
type Answers = Rc<RefCell<HashMap<usize, bool>>>;

/// Succeed, and consume nothing, when the next token is a `(` whose
/// matching `)` is followed by `->`. Fail in every other case.
pub(super) fn closure_ahead<'tokens, I>(
) -> impl Parser<'tokens, I, (), extra::Err<Rich<'tokens, Token>>> + Clone
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let answers: Answers = Rc::default();
    custom(move |inp| {
        let start = inp.save();
        let before = inp.cursor();
        let first = inp.next();
        let span: SimpleSpan = inp.span_since(&before);
        let is_closure = if first == Some(Token::LParen) {
            let known = answers.borrow().get(&span.start).copied();
            known.unwrap_or_else(|| scan_group(inp, span.start, &answers))
        } else {
            false
        };
        inp.rewind(start);
        if is_closure {
            Ok(())
        } else {
            Err(Rich::custom(span, "not a closure"))
        }
    })
}

/// Read the tokens after the `(` at `open` up to its matching `)`, and
/// record the answer for that `(` and for each `(` inside it.
///
/// A `(` that has no `)` before the end of the input is not a closure.
fn scan_group<'tokens, I>(
    inp: &mut chumsky::input::InputRef<'tokens, '_, I, extra::Err<Rich<'tokens, Token>>>,
    open: usize,
    answers: &Answers,
) -> bool
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    let mut open_parens = vec![open];
    let mut found = HashMap::new();
    while let Some(outer) = open_parens.last().copied() {
        let before = inp.cursor();
        let Some(token) = inp.next() else {
            break;
        };
        if token == Token::LParen {
            let span: SimpleSpan = inp.span_since(&before);
            open_parens.push(span.start);
        } else if token == Token::RParen {
            open_parens.pop();
            found.insert(outer, inp.peek() == Some(Token::Arrow));
        }
    }
    for unclosed in open_parens {
        found.insert(unclosed, false);
    }
    let answer = found.get(&open).copied().unwrap_or(false);
    answers.borrow_mut().extend(found);
    answer
}
