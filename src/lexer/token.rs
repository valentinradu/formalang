use logos::{Logos, Skip};

/// Token types for `FormaLang` lexer
#[expect(
    clippy::exhaustive_enums,
    reason = "token enum is matched exhaustively by the parser"
)]
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
// Skip whitespace
// Skip plain line comments. Requires the third character to NOT be `/`
// or `!` so `///` (item doc comment) and `//!` (module/parent doc
// comment) reach their dedicated variants below. The `|//[/!]?$`
// alternation handles the edge cases of a bare `//`, `///`, or `//!`
// at end of input — those carry no content and are skipped.
#[logos(skip r"//([^/!\n][^\n]*)?|//[/!][\n]")]
pub enum Token {
    /// Phantom variant: matches the opening `/*` of a (possibly nested)
    /// block comment. The `skip_block_comment` callback consumes the
    /// rest of the comment manually (tracking nest depth) and returns
    /// `Skip`, so this variant is never emitted into the token stream.
    /// It exists only because Logos requires the `#[token]` attribute
    /// to live on a variant. Closes audit finding #47.
    #[token("/*", skip_block_comment)]
    BlockComment,

    /// Item doc comment: `/// text` attaches to the following definition.
    /// Captured trimmed (leading `/// ` stripped, trailing whitespace
    /// removed). Multiple consecutive doc-comment lines are joined by
    /// the parser into a single attached docstring. Audit finding #51.
    #[regex(r"///[^\n]*", parse_doc_comment)]
    DocComment(String),

    /// Module/parent doc comment: `//! text` attaches to the enclosing
    /// definition or file. Captured the same way as `DocComment`.
    /// Audit finding #51.
    #[regex(r"//![^\n]*", parse_inner_doc_comment)]
    InnerDocComment(String),

    // Keywords
    #[token("trait")]
    Trait,
    #[token("struct")]
    Struct,
    #[token("impl")]
    Impl,
    #[token("enum")]
    Enum,
    #[token("mod")]
    Module,
    #[token("use")]
    Use,
    #[token("pub")]
    Pub,
    #[token("let")]
    Let,
    #[token("mut")]
    Mut,
    #[token("sink")]
    Sink,
    #[token("extern")]
    Extern,
    #[token("match")]
    Match,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("nil")]
    Nil,
    #[token("as")]
    As,
    #[token("self")]
    SelfKeyword,
    #[token("fn")]
    Fn,

    // Literals
    //
    // Single-line string: `"..."` with escape sequences from the spec:
    //   \"  \\  \n  \t  \r  \uXXXX
    // No raw newlines allowed.
    #[regex(r#""([^"\\\n]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""#, |lex| parse_string(lex.slice()))]
    // Multi-line string: `"""..."""` — raw newlines, tabs and carriage returns
    // are permitted. Logos' regex engine does not match `\n`/`\r`/`\t` inside
    // negated character classes by default, so they are enumerated explicitly.
    // The regex greedily matches to the final `"""` delimiter.
    #[regex(
        r#""""([^"\\\n\r\t]|\n|\r|\t|"[^"]|""[^"]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""""#,
        |lex| parse_multiline_string(lex.slice())
    )]
    String(String),

    // Number literal supporting underscores and scientific notation:
    //   1_000_000          integer with underscores
    //   1.5                simple fractional
    //   1_000.500_5        fractional with underscores
    //   1e5, 2E+10, 1.5e-3 scientific notation (with optional sign)
    // Underscores are stripped before parsing to f64.
    #[regex(
        r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?",
        |lex| parse_number(lex.slice())
    )]
    Number(f64),

    #[regex(r"r/([^/\\]|\\.)+/[gimsuvy]*", |lex| lex.slice().to_string())]
    Regex(String), // Full regex string, parse later

    // Path literals start with `/` and must be followed by a non-digit,
    // non-operator character. This disambiguates them from integer
    // division (`10/2` tokenises as Number, Slash, Number, not as
    // Number followed by Path("2")). See audit finding #20.
    #[regex(
        r"/[a-zA-Z._~][^/\s\\,(){}\[\]]*(/([^/\s\\,(){}\[\]]|\\.)+)*",
        |lex| lex.slice()[1..].to_string()
    )]
    Path(String),

    // Identifier: starts with letter/underscore, contains alphanumerics/underscores
    // BUT: standalone underscore "_" is excluded (handled by Underscore token)
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*|_[a-zA-Z0-9_]+", |lex| lex.slice().to_string())]
    Ident(String),

    // Operators and punctuation
    #[token(".")]
    Dot,
    #[token(":")]
    Colon,
    #[token("::")]
    DoubleColon,
    #[token(",")]
    Comma,
    #[token("=")]
    Equals,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("==")]
    EqEq,
    #[token("!=")]
    Ne,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token("&&")]
    And,
    #[token("||")]
    Or,
    #[token("|")]
    Pipe,
    #[token("!")]
    Bang,
    #[token("?")]
    Question,
    #[token("->")]
    Arrow,
    #[token("_")]
    Underscore,
    #[token("..")]
    DotDot,
    #[token("...")]
    DotDotDot,

    // Delimiters
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
}

/// Strip the `///` prefix and a single leading space from a doc-comment
/// slice. Returns the remaining text (trimmed of trailing whitespace).
fn parse_doc_comment(lex: &logos::Lexer<'_, Token>) -> String {
    let raw = lex.slice();
    let body = raw.strip_prefix("///").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
}

/// Strip the `//!` prefix and a single leading space from an inner
/// doc-comment slice. Returns the remaining text (trimmed of trailing
/// whitespace).
fn parse_inner_doc_comment(lex: &logos::Lexer<'_, Token>) -> String {
    let raw = lex.slice();
    let body = raw.strip_prefix("//!").unwrap_or(raw);
    let body = body.strip_prefix(' ').unwrap_or(body);
    body.trim_end().to_string()
}

/// Skip a nested block comment.
///
/// Called after Logos has matched the opening `/*`. Scans the remainder
/// while tracking nesting depth: every `/*` increments the counter and
/// every `*/` decrements it. Bumps the lexer cursor past the matching
/// closing `*/` (or to end-of-input on an unterminated comment, which
/// then surfaces as a parse error downstream). Audit finding #47.
fn skip_block_comment(lex: &mut logos::Lexer<'_, Token>) -> Skip {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut depth: usize = 1;
    let mut i: usize = 0;
    let len = bytes.len();
    while i < len {
        let next_idx = i.saturating_add(1);
        let byte = bytes.get(i).copied().unwrap_or(0);
        let next = bytes.get(next_idx).copied().unwrap_or(0);
        if next_idx < len && byte == b'/' && next == b'*' {
            depth = depth.saturating_add(1);
            i = i.saturating_add(2);
        } else if next_idx < len && byte == b'*' && next == b'/' {
            depth = depth.saturating_sub(1);
            i = i.saturating_add(2);
            if depth == 0 {
                lex.bump(i);
                return Skip;
            }
        } else {
            i = i.saturating_add(1);
        }
    }
    // Unterminated block comment: consume the rest of the input so the
    // lexer doesn't loop. The parser will surface "unexpected end of
    // input" as a normal parse error.
    lex.bump(len);
    Skip
}

/// Parse a numeric literal slice, stripping underscores before calling `f64::parse`.
///
/// Returns `None` on parse failure so logos emits an error that the lexer converts
/// into [`crate::error::CompilerError::InvalidNumber`].
fn parse_number(s: &str) -> Option<f64> {
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

fn parse_string(s: &str) -> String {
    // Remove surrounding double-quotes and process escape sequences.
    // The lexer regex guarantees s starts and ends with `"`.
    let content = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_default();
    process_escapes(content)
}

fn parse_multiline_string(s: &str) -> String {
    // Remove surrounding triple-quotes and process escape sequences.
    // The lexer regex guarantees s starts and ends with `"""`.
    let content = s
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
        .unwrap_or_default();
    process_escapes(content)
}

fn process_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(c @ ('"' | '\\')) => result.push(c),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('u') => {
                    // Parse \uXXXX
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(unicode_char) = char::from_u32(code) {
                            result.push(unicode_char);
                        }
                    }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Helper to parse regex string into pattern and flags
#[must_use]
pub fn parse_regex(s: &str) -> Option<(String, String)> {
    let content = s.strip_prefix("r/")?;
    let last_slash = content.rfind('/')?;
    let (pattern, rest) = content.split_at(last_slash);
    let flags = rest.strip_prefix('/').unwrap_or_default();

    Some((pattern.to_string(), flags.to_string()))
}

impl Token {
    #[must_use]
    pub const fn is_keyword(&self) -> bool {
        matches!(
            self,
            Self::Trait
                | Self::Struct
                | Self::Impl
                | Self::Enum
                | Self::Module
                | Self::Use
                | Self::Pub
                | Self::Let
                | Self::Mut
                | Self::Sink
                | Self::Extern
                | Self::Match
                | Self::For
                | Self::In
                | Self::If
                | Self::Else
                | Self::True
                | Self::False
                | Self::Nil
                | Self::As
                | Self::SelfKeyword
                | Self::Fn
        )
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Trait => "trait",
            Self::Struct => "struct",
            Self::Impl => "impl",
            Self::Enum => "enum",
            Self::Module => "mod",
            Self::Use => "use",
            Self::Pub => "pub",
            Self::Let => "let",
            Self::Mut => "mut",
            Self::Sink => "sink",
            Self::Extern => "extern",
            Self::Match => "match",
            Self::For => "for",
            Self::In => "in",
            Self::If => "if",
            Self::Else => "else",
            Self::True => "true",
            Self::False => "false",
            Self::Nil => "nil",
            Self::As => "as",
            Self::SelfKeyword => "self",
            Self::Fn => "fn",
            Self::Dot => ".",
            Self::Colon => ":",
            Self::DoubleColon => "::",
            Self::Comma => ",",
            Self::Equals => "=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::EqEq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Le => "<=",
            Self::Ge => ">=",
            Self::And => "&&",
            Self::Or => "||",
            Self::Pipe => "|",
            Self::Bang => "!",
            Self::Question => "?",
            Self::Arrow => "->",
            Self::Underscore => "_",
            Self::DotDot => "..",
            Self::DotDotDot => "...",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBrace => "{",
            Self::RBrace => "}",
            Self::LBracket => "[",
            Self::RBracket => "]",
            Self::String(_)
            | Self::Number(_)
            | Self::Regex(_)
            | Self::Path(_)
            | Self::Ident(_)
            | Self::DocComment(_)
            | Self::InnerDocComment(_) => "<complex token>",
            // Phantom variant — `skip_block_comment` returns Skip so the
            // lexer never emits it. See `BlockComment` doc comment.
            Self::BlockComment => "<block comment>",
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // For literal tokens, show descriptive names
            Self::String(_) => write!(f, "string"),
            Self::Number(_) => write!(f, "number"),
            Self::Regex(_) => write!(f, "regex"),
            Self::Path(_) => write!(f, "path"),
            Self::Ident(_) => write!(f, "identifier"),
            Self::DocComment(_) => write!(f, "doc comment"),
            Self::InnerDocComment(_) => write!(f, "inner doc comment"),
            // For all other tokens, use the as_str() representation
            Self::Trait
            | Self::Struct
            | Self::Impl
            | Self::Enum
            | Self::Module
            | Self::Use
            | Self::Pub
            | Self::Let
            | Self::Mut
            | Self::Sink
            | Self::Extern
            | Self::Match
            | Self::For
            | Self::In
            | Self::If
            | Self::Else
            | Self::True
            | Self::False
            | Self::Nil
            | Self::As
            | Self::SelfKeyword
            | Self::Fn
            | Self::Dot
            | Self::Colon
            | Self::DoubleColon
            | Self::Comma
            | Self::Equals
            | Self::Plus
            | Self::Minus
            | Self::Star
            | Self::Slash
            | Self::Percent
            | Self::EqEq
            | Self::Ne
            | Self::Lt
            | Self::Gt
            | Self::Le
            | Self::Ge
            | Self::And
            | Self::Or
            | Self::Pipe
            | Self::Bang
            | Self::Question
            | Self::Arrow
            | Self::Underscore
            | Self::DotDot
            | Self::DotDotDot
            | Self::LParen
            | Self::RParen
            | Self::LBrace
            | Self::RBrace
            | Self::LBracket
            | Self::RBracket
            | Self::BlockComment => write!(f, "'{}'", self.as_str()),
        }
    }
}
