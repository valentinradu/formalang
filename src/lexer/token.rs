use logos::Logos;

/// Token types for FormaLang lexer
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
    #[token("model")]
    Model,
    #[token("view")]
    View,
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
    #[token("provides")]
    Provides,
    #[token("consumes")]
    Consumes,
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
    #[token("?")]
    Question,
    #[token("->")]
    Arrow,
    #[token("_")]
    Underscore,
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
    // Remove quotes and process escape sequences
    let content = &s[1..s.len() - 1];
    process_escapes(content)
}

fn parse_multiline_string(s: &str) -> String {
    // Remove triple quotes and process escape sequences
    let content = &s[3..s.len() - 3];
    process_escapes(content)
}

fn process_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
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
pub fn parse_regex(s: &str) -> Option<(String, String)> {
    if !s.starts_with("r/") {
        return None;
    }

    let content = &s[2..]; // Remove "r/"
    let last_slash = content.rfind('/')?;

    let pattern = content[..last_slash].to_string();
    let flags = content[last_slash + 1..].to_string();

    Some((pattern, flags))
}

impl Token {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::Trait
                | Token::Struct
                | Token::Impl
                | Token::Model
                | Token::View
                | Token::Enum
                | Token::Module
                | Token::Use
                | Token::Pub
                | Token::Let
                | Token::Mut
                | Token::Mount
                | Token::Match
                | Token::For
                | Token::In
                | Token::If
                | Token::Else
                | Token::True
                | Token::False
                | Token::Nil
                | Token::Provides
                | Token::Consumes
                | Token::As
                | Token::Fn
        )
    }

    pub fn is_type_keyword(&self) -> bool {
        matches!(
            self,
            Token::StringType
                | Token::NumberType
                | Token::BooleanType
                | Token::PathType
                | Token::RegexType
                | Token::NeverType
                | Token::F32Type
                | Token::I32Type
                | Token::U32Type
                | Token::BoolType
                | Token::Vec2Type
                | Token::Vec3Type
                | Token::Vec4Type
                | Token::IVec2Type
                | Token::IVec3Type
                | Token::IVec4Type
                | Token::UVec2Type
                | Token::UVec3Type
                | Token::UVec4Type
                | Token::Mat2Type
                | Token::Mat3Type
                | Token::Mat4Type
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Token::Trait => "trait",
            Token::Struct => "struct",
            Token::Impl => "impl",
            Token::Model => "model",
            Token::View => "view",
            Token::Enum => "enum",
            Token::Module => "mod",
            Token::Use => "use",
            Token::Pub => "pub",
            Token::Let => "let",
            Token::Mut => "mut",
            Token::Mount => "mount",
            Token::Match => "match",
            Token::For => "for",
            Token::In => "in",
            Token::If => "if",
            Token::Else => "else",
            Token::True => "true",
            Token::False => "false",
            Token::Nil => "nil",
            Token::Provides => "provides",
            Token::Consumes => "consumes",
            Token::As => "as",
            Token::SelfKeyword => "self",
            Token::Fn => "fn",
            Token::StringType => "String",
            Token::NumberType => "Number",
            Token::BooleanType => "Boolean",
            Token::PathType => "Path",
            Token::RegexType => "Regex",
            Token::NeverType => "Never",
            Token::F32Type => "f32",
            Token::I32Type => "i32",
            Token::U32Type => "u32",
            Token::BoolType => "bool",
            Token::Vec2Type => "vec2",
            Token::Vec3Type => "vec3",
            Token::Vec4Type => "vec4",
            Token::IVec2Type => "ivec2",
            Token::IVec3Type => "ivec3",
            Token::IVec4Type => "ivec4",
            Token::UVec2Type => "uvec2",
            Token::UVec3Type => "uvec3",
            Token::UVec4Type => "uvec4",
            Token::Mat2Type => "mat2",
            Token::Mat3Type => "mat3",
            Token::Mat4Type => "mat4",
            Token::Dot => ".",
            Token::Colon => ":",
            Token::DoubleColon => "::",
            Token::Comma => ",",
            Token::Equals => "=",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::EqEq => "==",
            Token::Ne => "!=",
            Token::Lt => "<",
            Token::Gt => ">",
            Token::Le => "<=",
            Token::Ge => ">=",
            Token::And => "&&",
            Token::Or => "||",
            Token::Question => "?",
            Token::Arrow => "->",
            Token::Underscore => "_",
            Token::DotDotDot => "...",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LBracket => "[",
            Token::RBracket => "]",
            Token::Eof => "<eof>",
            _ => "<complex token>",
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // For literal tokens, show descriptive names
            Token::String(_) => write!(f, "string"),
            Token::Number(_) => write!(f, "number"),
            Token::Regex(_) => write!(f, "regex"),
            Token::Path(_) => write!(f, "path"),
            Token::Ident(_) => write!(f, "identifier"),
            // For all other tokens, use the as_str() representation
            _ => write!(f, "'{}'", self.as_str()),
        }
    }
}
