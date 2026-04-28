mod callbacks;

pub use callbacks::parse_regex;
use callbacks::{
    parse_doc_comment, parse_inner_doc_comment, parse_multiline_string, parse_number,
    parse_string, skip_block_comment,
};

use logos::Logos;

/// Per-lexer state passed to Logos callbacks via `extras`.
///
/// Used by Logos callbacks to record diagnostics that can't be expressed
/// directly through the `Result<Token, ()>` return convention:
///
/// - `unterminated_block_comments` — byte ranges of `/* … */` comments
///   that run to end-of-input.
/// - `invalid_unicode_escapes` — `(literal_start, literal_end, bad_hex)`
///   for each `\uXXXX` escape inside a string literal whose hex digits do
///   not denote a valid Unicode scalar value. The literal
///   span is used as the diagnostic span; the bad hex is reported as the
///   error's `value`.
///
/// The wrapping [`Lexer`](super::Lexer) drains both vectors into real
/// [`CompilerError`](crate::CompilerError) values after tokenisation.
#[derive(Default, Debug)]
pub struct LexerExtras {
    pub unterminated_block_comments: Vec<(usize, usize)>,
    pub invalid_unicode_escapes: Vec<(usize, usize, String)>,
}

/// Token types for `FormaLang` lexer
#[expect(
    clippy::exhaustive_enums,
    reason = "token enum is matched exhaustively by the parser"
)]
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(extras = LexerExtras)]
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
    /// to live on a variant.
    #[token("/*", skip_block_comment)]
    BlockComment,

    /// Item doc comment: `/// text` attaches to the following definition.
    /// Captured trimmed (leading `/// ` stripped, trailing whitespace
    /// removed). Multiple consecutive doc-comment lines are joined by
    /// the parser into a single attached docstring.
    #[regex(r"///[^\n]*", parse_doc_comment)]
    DocComment(String),

    /// Module/parent doc comment: `//! text` attaches to the enclosing
    /// definition or file. Captured the same way as `DocComment`.
    ///
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
    /// Codegen hint: inline this function at every call site when
    /// possible. Parsed as a prefix keyword before `fn`. Frontend
    /// passes the attribute through to the IR; backends honour it.
    #[token("inline")]
    Inline,
    /// Codegen hint: do not inline this function. Mirrors `inline` but
    /// in the opposite direction.
    #[token("no_inline")]
    NoInline,
    /// Codegen hint: this function is rarely called (cold path).
    /// Backends may place it in a separate section and bias surrounding
    /// branches.
    #[token("cold")]
    Cold,

    // Literals
    //
    // Single-line string: `"..."` with escape sequences from the spec:
    //   \"  \\  \n  \t  \r  \uXXXX
    // No raw newlines allowed.
    #[regex(r#""([^"\\\n]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""#, parse_string)]
    // Multi-line string: `"""..."""` — raw newlines, tabs and carriage returns
    // are permitted. Logos' regex engine does not match `\n`/`\r`/`\t` inside
    // negated character classes by default, so they are enumerated explicitly.
    // The regex greedily matches to the final `"""` delimiter.
    #[regex(
        r#""""([^"\\\n\r\t]|\n|\r|\t|"[^"]|""[^"]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""""#,
        parse_multiline_string
    )]
    String(String),

    // Number literal supporting underscores, scientific notation, and an
    // optional uppercase width-tag suffix:
    //   1_000_000          integer with underscores
    //   1.5                simple fractional
    //   1_000.500_5        fractional with underscores
    //   1e5, 2E+10, 1.5e-3 scientific notation (with optional sign)
    //   42I32, 3.14F64     numeric literal with type suffix
    // Underscores are stripped before parsing the digits to f64.
    #[regex(
        r"[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9]+)?(I32|I64|F32|F64)?",
        |lex| parse_number(lex.slice())
    )]
    Number(crate::ast::NumberLiteral),

    #[regex(r"r/([^/\\]|\\.)+/[gimsuvy]*", |lex| lex.slice().to_string())]
    Regex(String), // Full regex string, parse later

    // Path literals start with `/` and must be followed by a non-digit,
    // non-operator character. This disambiguates them from integer
    // division (`10/2` tokenises as Number, Slash, Number, not as
    // Number followed by Path("2")).
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
                | Self::Inline
                | Self::NoInline
                | Self::Cold
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
            Self::Inline => "inline",
            Self::NoInline => "no_inline",
            Self::Cold => "cold",
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
            | Self::Inline
            | Self::NoInline
            | Self::Cold
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
