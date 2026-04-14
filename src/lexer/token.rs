use logos::Logos;

/// Token types for `FormaLang` lexer
#[expect(clippy::exhaustive_enums, reason = "token enum is matched exhaustively by the parser")]
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")] // Skip whitespace
#[logos(skip r"//[^\n]*")] // Skip line comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")] // Skip block comments
pub enum Token {
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
    #[token("mount")]
    Mount,
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

    // Primitive type keywords
    #[token("String")]
    StringType,
    #[token("Number")]
    NumberType,
    #[token("Boolean")]
    BooleanType,
    #[token("Path")]
    PathType,
    #[token("Regex")]
    RegexType,
    #[token("Never")]
    NeverType,

    // GPU primitive types
    #[token("f32")]
    F32Type,
    #[token("i32")]
    I32Type,
    #[token("u32")]
    U32Type,
    #[token("bool")]
    BoolType,

    // Vector types
    #[token("vec2")]
    Vec2Type,
    #[token("vec3")]
    Vec3Type,
    #[token("vec4")]
    Vec4Type,
    #[token("ivec2")]
    IVec2Type,
    #[token("ivec3")]
    IVec3Type,
    #[token("ivec4")]
    IVec4Type,
    #[token("uvec2")]
    UVec2Type,
    #[token("uvec3")]
    UVec3Type,
    #[token("uvec4")]
    UVec4Type,

    // Matrix types
    #[token("mat2")]
    Mat2Type,
    #[token("mat3")]
    Mat3Type,
    #[token("mat4")]
    Mat4Type,

    // Literals
    #[regex(r#""([^"\\]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""#, |lex| parse_string(lex.slice()))]
    #[regex(r#""""([^\\]|\\["\\ntr]|\\u[0-9a-fA-F]{4})*""""#, |lex| parse_multiline_string(lex.slice()))]
    String(String),

    // Unsigned integer literal with 'u' suffix: 1u, 42u
    #[regex(r"[0-9]+u", |lex| {
        let s = lex.slice();
        s.strip_suffix('u').and_then(|n| n.parse::<u32>().ok())
    })]
    UnsignedInt(u32),

    // Signed integer literal with 'i' suffix: 1i, -42i
    #[regex(r"-?[0-9]+i", |lex| {
        let s = lex.slice();
        s.strip_suffix('i').and_then(|n| n.parse::<i32>().ok())
    })]
    SignedInt(i32),

    #[regex(r"-?[0-9]+(\.[0-9]+)?", |lex| lex.slice().parse::<f64>().ok())]
    Number(f64),

    #[regex(r"r/([^/\\]|\\.)+/[gimsuvy]*", |lex| lex.slice().to_string())]
    Regex(String), // Full regex string, parse later

    #[regex(r"/([^/\s\\,(){}\[\]]|\\.)+(/([^/\s\\,(){}\[\]]|\\.)+)*", |lex| lex.slice()[1..].to_string())]
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

    // Special
    Eof,
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
                | Self::Mount
                | Self::Match
                | Self::For
                | Self::In
                | Self::If
                | Self::Else
                | Self::True
                | Self::False
                | Self::Nil
                | Self::As
                | Self::Fn
        )
    }

    #[must_use] 
    pub const fn is_type_keyword(&self) -> bool {
        matches!(
            self,
            Self::StringType
                | Self::NumberType
                | Self::BooleanType
                | Self::PathType
                | Self::RegexType
                | Self::NeverType
                | Self::F32Type
                | Self::I32Type
                | Self::U32Type
                | Self::BoolType
                | Self::Vec2Type
                | Self::Vec3Type
                | Self::Vec4Type
                | Self::IVec2Type
                | Self::IVec3Type
                | Self::IVec4Type
                | Self::UVec2Type
                | Self::UVec3Type
                | Self::UVec4Type
                | Self::Mat2Type
                | Self::Mat3Type
                | Self::Mat4Type
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
            Self::Mount => "mount",
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
            Self::StringType => "String",
            Self::NumberType => "Number",
            Self::BooleanType => "Boolean",
            Self::PathType => "Path",
            Self::RegexType => "Regex",
            Self::NeverType => "Never",
            Self::F32Type => "f32",
            Self::I32Type => "i32",
            Self::U32Type => "u32",
            Self::BoolType => "bool",
            Self::Vec2Type => "vec2",
            Self::Vec3Type => "vec3",
            Self::Vec4Type => "vec4",
            Self::IVec2Type => "ivec2",
            Self::IVec3Type => "ivec3",
            Self::IVec4Type => "ivec4",
            Self::UVec2Type => "uvec2",
            Self::UVec3Type => "uvec3",
            Self::UVec4Type => "uvec4",
            Self::Mat2Type => "mat2",
            Self::Mat3Type => "mat3",
            Self::Mat4Type => "mat4",
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
            Self::Eof => "<eof>",
            Self::String(_)
            | Self::UnsignedInt(_)
            | Self::SignedInt(_)
            | Self::Number(_)
            | Self::Regex(_)
            | Self::Path(_)
            | Self::Ident(_) => "<complex token>",
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // For literal tokens, show descriptive names
            Self::String(_) => write!(f, "string"),
            Self::Number(_) => write!(f, "number"),
            Self::UnsignedInt(_) => write!(f, "unsigned int"),
            Self::SignedInt(_) => write!(f, "signed int"),
            Self::Regex(_) => write!(f, "regex"),
            Self::Path(_) => write!(f, "path"),
            Self::Ident(_) => write!(f, "identifier"),
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
            | Self::Mount
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
            | Self::StringType
            | Self::NumberType
            | Self::BooleanType
            | Self::PathType
            | Self::RegexType
            | Self::NeverType
            | Self::F32Type
            | Self::I32Type
            | Self::U32Type
            | Self::BoolType
            | Self::Vec2Type
            | Self::Vec3Type
            | Self::Vec4Type
            | Self::IVec2Type
            | Self::IVec3Type
            | Self::IVec4Type
            | Self::UVec2Type
            | Self::UVec3Type
            | Self::UVec4Type
            | Self::Mat2Type
            | Self::Mat3Type
            | Self::Mat4Type
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
            | Self::Eof => write!(f, "'{}'", self.as_str()),
        }
    }
}
