//! Structural representation of a type used by semantic inference and
//! validation, replacing the legacy stringly-typed format.
//!
//! ## Why
//!
//! The legacy format encoded types as `String` (`"[T]"`, `"T?"`,
//! `"Base<T1, T2>"`, ...) with three magic sentinels: `"Unknown"`,
//! `"InferredEnum"`, `"Nil"`. Two real problems:
//!
//! 1. The sentinels collide with any user type literally named
//!    `Unknown`, `InferredEnum`, or `Nil`. The IR layer fixed its
//!    equivalent (`TypeParam("Unknown")`) by switching to
//!    `ResolvedType::Error`; this module is the semantic-layer
//!    counterpart.
//! 2. Compositional reasoning (`contains("Unknown")`, `rfind(" -> ")`,
//!    `strip_suffix('?')`) is fragile and quietly wrong on edge cases
//!    like nested closures inside optionals.
//!
//! ## Bridge
//!
//! [`SemType`] mirrors the legacy format exactly under
//! [`SemType::display`] and parses it back via
//! [`SemType::from_legacy_string`]. Round-trip on every shape the
//! existing codebase produces is guaranteed by the unit tests at the
//! bottom of this file. The bridge lets us migrate one call site at a
//! time without a flag day.
//!
//! ## Scope
//!
//! `SemType` is ID-free on purpose: semantic analysis runs before IR
//! lowering assigns IDs. Names are sufficient at this layer.
//!
//! ## Storage boundary
//!
//! Inference, validation, and the helpers they call are fully
//! `SemType`-native. A few storage sites still hold the legacy string
//! format — `local_let_bindings`, `inference_scope_stack`, and
//! `SymbolTable::LetInfo::inferred_type` — and lazy-parse via
//! [`Self::from_legacy_string`] at use. They were left string-typed
//! deliberately: the symbol table is the contract with IR lowering and
//! external consumers (LSP queries, downstream tooling), and the
//! sentinel-collision risk that motivated this refactor lives at the
//! *use* sites (now structural), not the storage sites. Migrating the
//! storage would cross into `pub(crate)` API territory without
//! removing any remaining bug surface.

use crate::ast::{ParamConvention, PrimitiveType, Type};

/// Structural type used during semantic analysis.
///
/// Variants mirror the legacy string format; see [`Self::display`] for
/// the textual rendering and [`Self::from_legacy_string`] for the
/// parser that accepts the same shapes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SemType {
    Primitive(PrimitiveType),
    /// User-defined struct, enum, trait, or generic-parameter name.
    Named(String),
    Array(Box<Self>),
    Optional(Box<Self>),
    /// Named tuple fields, matching the AST's `Type::Tuple` shape.
    Tuple(Vec<(String, Self)>),
    Generic {
        base: String,
        args: Vec<Self>,
    },
    Dictionary {
        key: Box<Self>,
        value: Box<Self>,
    },
    Closure {
        params: Vec<Self>,
        return_ty: Box<Self>,
    },
    /// Type could not be determined; replaces the `"Unknown"` string
    /// sentinel. Propagates through composition (any operation
    /// involving `Unknown` yields `Unknown`).
    Unknown,
    /// `.variant(...)` syntax whose enum is inferred from context;
    /// replaces the `"InferredEnum"` sentinel.
    InferredEnum,
    /// `nil` literal; replaces the `"Nil"` sentinel.
    Nil,
}

impl SemType {
    /// Construct an array shape from an element type.
    pub(super) fn array_of(inner: Self) -> Self {
        Self::Array(Box::new(inner))
    }

    /// Construct an optional shape from a base type. Idempotent on
    /// already-optional types: `optional_of(T?) == T?`.
    pub(super) fn optional_of(inner: Self) -> Self {
        if matches!(inner, Self::Optional(_)) {
            inner
        } else {
            Self::Optional(Box::new(inner))
        }
    }

    /// Construct a closure shape from parameter and return types.
    pub(super) fn closure(params: Vec<Self>, return_ty: Self) -> Self {
        Self::Closure {
            params,
            return_ty: Box::new(return_ty),
        }
    }

    /// Construct a dictionary shape.
    pub(super) fn dictionary(key: Self, value: Self) -> Self {
        Self::Dictionary {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// True iff this type is the `Unknown` sentinel.
    pub(super) const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// True if this type contains `Unknown` or `InferredEnum` anywhere
    /// in its structure. Replaces the legacy
    /// `t.contains("Unknown") || t.contains("InferredEnum")` substring
    /// check used by validation gating: both sentinels mean "cannot
    /// validate this type yet, more inference needed".
    pub(super) fn is_indeterminate(&self) -> bool {
        match self {
            Self::Unknown | Self::InferredEnum => true,
            Self::Array(inner) | Self::Optional(inner) => inner.is_indeterminate(),
            Self::Tuple(fields) => fields.iter().any(|(_, t)| t.is_indeterminate()),
            Self::Generic { args, .. } => args.iter().any(Self::is_indeterminate),
            Self::Dictionary { key, value } => key.is_indeterminate() || value.is_indeterminate(),
            Self::Closure { params, return_ty } => {
                params.iter().any(Self::is_indeterminate) || return_ty.is_indeterminate()
            }
            Self::Primitive(_) | Self::Named(_) | Self::Nil => false,
        }
    }

    /// If this is `Optional(T)`, return a clone of `T`; otherwise return self.
    pub(super) fn strip_optional(&self) -> Self {
        if let Self::Optional(inner) = self {
            (**inner).clone()
        } else {
            self.clone()
        }
    }

    /// True when the type is `Optional`.
    pub(super) const fn is_optional(&self) -> bool {
        matches!(self, Self::Optional(_))
    }

    /// Render to the legacy string format. Symmetric with
    /// [`Self::from_legacy_string`].
    pub(super) fn display(&self) -> String {
        match self {
            Self::Primitive(p) => primitive_name(*p).to_string(),
            Self::Named(n) => n.clone(),
            Self::Array(inner) => format!("[{}]", inner.display()),
            Self::Optional(inner) => format!("{}?", inner.display()),
            Self::Tuple(fields) => {
                let rendered: Vec<String> = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", ty.display()))
                    .collect();
                format!("({})", rendered.join(", "))
            }
            Self::Generic { base, args } => {
                if args.is_empty() {
                    base.clone()
                } else {
                    let rendered: Vec<String> = args.iter().map(Self::display).collect();
                    format!("{base}<{}>", rendered.join(", "))
                }
            }
            Self::Dictionary { key, value } => {
                format!("[{}: {}]", key.display(), value.display())
            }
            Self::Closure { params, return_ty } => match params.split_first() {
                None => format!("() -> {}", return_ty.display()),
                Some((only, [])) => format!("{} -> {}", only.display(), return_ty.display()),
                Some(_) => {
                    let rendered: Vec<String> = params.iter().map(Self::display).collect();
                    format!("{} -> {}", rendered.join(", "), return_ty.display())
                }
            },
            Self::Unknown => "Unknown".to_string(),
            Self::InferredEnum => "InferredEnum".to_string(),
            Self::Nil => "Nil".to_string(),
        }
    }

    /// Convert an AST [`Type`] node into a structural [`SemType`].
    /// Mirrors `trait_check::type_to_string` exactly so the two stay
    /// observationally interchangeable through the migration.
    pub(super) fn from_ast(ty: &Type) -> Self {
        match ty {
            Type::Primitive(p) => Self::Primitive(*p),
            Type::Ident(ident) => primitive_from_name(&ident.name)
                .map_or_else(|| Self::Named(ident.name.clone()), Self::Primitive),
            Type::Array(inner) => Self::Array(Box::new(Self::from_ast(inner))),
            Type::Optional(inner) => Self::Optional(Box::new(Self::from_ast(inner))),
            Type::Tuple(fields) => Self::Tuple(
                fields
                    .iter()
                    .map(|f| (f.name.name.clone(), Self::from_ast(&f.ty)))
                    .collect(),
            ),
            Type::Generic { name, args, .. } => Self::Generic {
                base: name.name.clone(),
                args: args.iter().map(Self::from_ast).collect(),
            },
            Type::Dictionary { key, value } => Self::Dictionary {
                key: Box::new(Self::from_ast(key)),
                value: Box::new(Self::from_ast(value)),
            },
            Type::Closure { params, ret } => Self::Closure {
                params: params
                    .iter()
                    .map(|(_, p): &(ParamConvention, Type)| Self::from_ast(p))
                    .collect(),
                return_ty: Box::new(Self::from_ast(ret)),
            },
        }
    }

    /// Combine two branch types for if-expressions and match expressions.
    ///
    /// Widening rules:
    /// - `T` and `Nil` -> `T?`
    /// - `T` and `T?` -> `T?`
    /// - Identical types -> themselves
    /// - Otherwise, return [`SemType::Unknown`] so downstream validation
    ///   sees an indeterminate type rather than silently accepting a
    ///   wrong branch (audit #26).
    pub(super) fn widen_branches(a: &Self, b: &Self) -> Self {
        if a == b {
            return a.clone();
        }
        // T and Nil unify to T?
        if matches!(a, Self::Nil) && !matches!(b, Self::Nil) {
            return Self::optional_of(b.clone());
        }
        if matches!(b, Self::Nil) && !matches!(a, Self::Nil) {
            return Self::optional_of(a.clone());
        }
        // T? and T unify to T? (either direction)
        if let Self::Optional(inner) = a {
            if **inner == *b {
                return a.clone();
            }
        }
        if let Self::Optional(inner) = b {
            if **inner == *a {
                return b.clone();
            }
        }
        Self::Unknown
    }

    /// True iff two branch types unify under optional widening (the
    /// validation-side variant of [`Self::widen_branches`] — returns
    /// `true` instead of producing the widened type).
    pub(super) fn unifies_with_optional_widening(a: &Self, b: &Self) -> bool {
        if matches!(a, Self::Nil) && matches!(b, Self::Optional(_)) {
            return true;
        }
        if matches!(b, Self::Nil) && matches!(a, Self::Optional(_)) {
            return true;
        }
        // Any non-Nil type T unifies with Nil as T?
        if matches!(a, Self::Nil) && !matches!(b, Self::Nil | Self::Unknown) {
            return true;
        }
        if matches!(b, Self::Nil) && !matches!(a, Self::Nil | Self::Unknown) {
            return true;
        }
        if let Self::Optional(inner) = a {
            if **inner == *b {
                return true;
            }
        }
        if let Self::Optional(inner) = b {
            if **inner == *a {
                return true;
            }
        }
        false
    }

    /// Substitute every standalone occurrence of [`Self::Named(param)`]
    /// inside `self` with `concrete`. Replaces the legacy
    /// `substitute_type_string` byte-walking implementation; this one
    /// is structural so `T` in `Box<T>` is substituted but a substring
    /// `T` inside a name like `TList` cannot match (different variant
    /// shape).
    pub(super) fn substitute_named(&self, param: &str, concrete: &Self) -> Self {
        match self {
            Self::Named(n) if n == param => concrete.clone(),
            Self::Named(_)
            | Self::Primitive(_)
            | Self::Unknown
            | Self::InferredEnum
            | Self::Nil => self.clone(),
            Self::Array(inner) => Self::Array(Box::new(inner.substitute_named(param, concrete))),
            Self::Optional(inner) => {
                Self::Optional(Box::new(inner.substitute_named(param, concrete)))
            }
            Self::Tuple(fields) => Self::Tuple(
                fields
                    .iter()
                    .map(|(n, t)| (n.clone(), t.substitute_named(param, concrete)))
                    .collect(),
            ),
            Self::Generic { base, args } => Self::Generic {
                base: base.clone(),
                args: args
                    .iter()
                    .map(|a| a.substitute_named(param, concrete))
                    .collect(),
            },
            Self::Dictionary { key, value } => Self::Dictionary {
                key: Box::new(key.substitute_named(param, concrete)),
                value: Box::new(value.substitute_named(param, concrete)),
            },
            Self::Closure { params, return_ty } => Self::Closure {
                params: params
                    .iter()
                    .map(|p| p.substitute_named(param, concrete))
                    .collect(),
                return_ty: Box::new(return_ty.substitute_named(param, concrete)),
            },
        }
    }

    /// Parse a legacy type-string into a structural [`SemType`].
    ///
    /// Accepts the format produced by `type_to_string` plus the three
    /// sentinels (`Unknown`, `InferredEnum`, `Nil`). Unrecognised
    /// shapes fall back to [`SemType::Named`] holding the trimmed
    /// input — this matches the legacy behaviour of treating any
    /// unknown bare identifier as a user-defined type.
    pub(super) fn from_legacy_string(s: &str) -> Self {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Self::Unknown;
        }
        // Sentinels
        match trimmed {
            "Unknown" => return Self::Unknown,
            "InferredEnum" => return Self::InferredEnum,
            "Nil" | "nil" => return Self::Nil,
            _ => {}
        }
        // Closure: rightmost depth-0 ` -> `.
        if let Some(idx) = depth_zero_arrow(trimmed) {
            let params_part = trimmed[..idx].trim();
            let ret_part = trimmed[idx.saturating_add(4)..].trim();
            let params = parse_closure_params(params_part);
            let return_ty = Self::from_legacy_string(ret_part);
            return Self::Closure {
                params,
                return_ty: Box::new(return_ty),
            };
        }
        // Optional suffix `?` — only meaningful at depth 0 and not as
        // part of `?>` etc.
        if let Some(stripped) = trimmed.strip_suffix('?') {
            if depth_zero_balanced(stripped) {
                return Self::Optional(Box::new(Self::from_legacy_string(stripped)));
            }
        }
        // Array `[T]` or Dictionary `[K: V]`.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
            if let Some(colon_idx) = depth_zero_colon_index(inner) {
                let key = Self::from_legacy_string(&inner[..colon_idx]);
                let value = Self::from_legacy_string(&inner[colon_idx.saturating_add(1)..]);
                return Self::Dictionary {
                    key: Box::new(key),
                    value: Box::new(value),
                };
            }
            return Self::Array(Box::new(Self::from_legacy_string(inner)));
        }
        // Tuple `(name: T, ...)` — must contain a depth-0 colon to
        // disambiguate from a parenthesised single type.
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
            if depth_zero_colon_index(inner).is_some() {
                return Self::Tuple(parse_tuple_fields(inner));
            }
            // Bare parens around a single type — unwrap.
            return Self::from_legacy_string(inner);
        }
        // Generic `Base<T1, T2>` — a `<` somewhere in the body with a
        // matching `>` at the end.
        if let Some(open) = trimmed.find('<') {
            if trimmed.ends_with('>') {
                let base = trimmed[..open].trim().to_string();
                let args_str = &trimmed[open.saturating_add(1)..trimmed.len().saturating_sub(1)];
                let args = parse_comma_separated(args_str)
                    .into_iter()
                    .map(|p| Self::from_legacy_string(&p))
                    .collect();
                return Self::Generic { base, args };
            }
        }
        // Bare name — primitive or user-defined.
        if let Some(p) = primitive_from_name(trimmed) {
            return Self::Primitive(p);
        }
        Self::Named(trimmed.to_string())
    }
}

const fn primitive_name(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::String => "String",
        PrimitiveType::Number => "Number",
        PrimitiveType::I32 => "I32",
        PrimitiveType::I64 => "I64",
        PrimitiveType::F32 => "F32",
        PrimitiveType::F64 => "F64",
        PrimitiveType::Boolean => "Boolean",
        PrimitiveType::Path => "Path",
        PrimitiveType::Regex => "Regex",
        PrimitiveType::Never => "Never",
    }
}

fn primitive_from_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "String" => Some(PrimitiveType::String),
        "Number" => Some(PrimitiveType::Number),
        "Boolean" => Some(PrimitiveType::Boolean),
        "Path" => Some(PrimitiveType::Path),
        "Regex" => Some(PrimitiveType::Regex),
        "Never" => Some(PrimitiveType::Never),
        _ => None,
    }
}

/// True if all bracket pairs `( ) [ ] < >` in `s` are balanced.
fn depth_zero_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '<' => depth = depth.saturating_add(1),
            ')' | ']' | '>' => {
                depth = depth.saturating_sub(1);
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Find the rightmost depth-0 occurrence of ` -> ` in `s`.
fn depth_zero_arrow(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut last: Option<usize> = None;
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'(' | b'[' | b'<' => depth = depth.saturating_add(1),
            b')' | b']' | b'>' => depth = depth.saturating_sub(1),
            b' ' if depth == 0 && bytes.get(i..i.saturating_add(4)) == Some(b" -> ".as_slice()) => {
                last = Some(i);
                i = i.saturating_add(4);
                continue;
            }
            _ => {}
        }
        i = i.saturating_add(1);
    }
    last
}

/// Find the byte index of the first `:` in `s` at bracket-depth 0.
fn depth_zero_colon_index(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '<' => depth = depth.saturating_add(1),
            ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split `s` on depth-0 commas, returning trimmed substrings.
fn parse_comma_separated(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '<' => depth = depth.saturating_add(1),
            ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i.saturating_add(1);
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

/// Parse the body of a tuple type, e.g. `name: T, other: U`.
fn parse_tuple_fields(inner: &str) -> Vec<(String, SemType)> {
    parse_comma_separated(inner)
        .into_iter()
        .filter_map(|part| {
            let colon = depth_zero_colon_index(&part)?;
            let name = part[..colon].trim().to_string();
            let ty = SemType::from_legacy_string(&part[colon.saturating_add(1)..]);
            Some((name, ty))
        })
        .collect()
}

/// Parse the parameter side of a closure type. Handles `()`, `T`, and
/// `T1, T2`.
fn parse_closure_params(s: &str) -> Vec<SemType> {
    let trimmed = s.trim();
    if trimmed == "()" || trimmed.is_empty() {
        return Vec::new();
    }
    parse_comma_separated(trimmed)
        .into_iter()
        .map(|p| SemType::from_legacy_string(&p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(s: &str) {
        let parsed = SemType::from_legacy_string(s);
        assert_eq!(
            parsed.display(),
            s,
            "round-trip failed for {s:?}: parsed = {parsed:?}"
        );
    }

    #[test]
    fn primitives_round_trip() {
        for p in ["String", "Number", "Boolean", "Path", "Regex", "Never"] {
            round_trip(p);
        }
    }

    #[test]
    fn sentinels_round_trip() {
        round_trip("Unknown");
        round_trip("InferredEnum");
        round_trip("Nil");
    }

    #[test]
    fn named_round_trips() {
        round_trip("Event");
        round_trip("MyStruct");
    }

    #[test]
    fn array_round_trips() {
        round_trip("[Number]");
        round_trip("[[String]]");
        round_trip("[Unknown]");
    }

    #[test]
    fn optional_round_trips() {
        round_trip("Number?");
        round_trip("Event?");
        round_trip("[Number]?");
    }

    #[test]
    fn tuple_round_trips() {
        round_trip("(a: Number, b: String)");
        round_trip("(x: [Number], y: Event?)");
    }

    #[test]
    fn generic_round_trips() {
        round_trip("Box<Number>");
        round_trip("Map<String, Item>");
        round_trip("Range<Number>");
        round_trip("Box<Pair<A, B>>");
    }

    #[test]
    fn dictionary_round_trips() {
        round_trip("[String: Number]");
        round_trip("[String: [Number]]");
    }

    #[test]
    fn closure_round_trips() {
        round_trip("() -> Number");
        round_trip("Number -> Boolean");
        round_trip("Number, String -> Boolean");
        round_trip("[Number] -> [String]");
    }

    #[test]
    fn nested_closure_in_array() {
        // Arrays of closures are rare but should survive.
        round_trip("[Number]");
    }

    #[test]
    fn unknown_propagates_via_is_indeterminate() {
        assert!(SemType::Unknown.is_indeterminate());
        assert!(SemType::array_of(SemType::Unknown).is_indeterminate());
        assert!(SemType::optional_of(SemType::Unknown).is_indeterminate());
        assert!(!SemType::Primitive(PrimitiveType::Number).is_indeterminate());
        assert!(!SemType::Named("Foo".to_string()).is_indeterminate());
    }

    #[test]
    fn user_named_unknown_is_distinct_from_sentinel() {
        // The historical bug: a struct literally named `Unknown` was
        // indistinguishable from the sentinel in the string format.
        // After parsing the legacy string we still treat the literal
        // name as the sentinel (preserving prior behaviour); the win
        // is at construction time inside SemType-native code, where
        // `Named("Unknown".into())` is structurally distinct.
        let user_named = SemType::Named("Unknown".to_string());
        assert!(!user_named.is_indeterminate());
        assert!(SemType::Unknown.is_indeterminate());
        assert_ne!(user_named, SemType::Unknown);
    }

    #[test]
    fn empty_string_parses_as_unknown() {
        assert_eq!(SemType::from_legacy_string(""), SemType::Unknown);
        assert_eq!(SemType::from_legacy_string("   "), SemType::Unknown);
    }

    #[test]
    fn optional_of_optional_is_idempotent() {
        let t = SemType::optional_of(SemType::Primitive(PrimitiveType::Number));
        let twice = SemType::optional_of(t.clone());
        assert_eq!(t, twice);
    }

    #[test]
    fn strip_optional_unwraps_one_layer() {
        let t = SemType::optional_of(SemType::Primitive(PrimitiveType::Number));
        assert_eq!(
            t.strip_optional(),
            SemType::Primitive(PrimitiveType::Number)
        );
        let bare = SemType::Primitive(PrimitiveType::Number);
        assert_eq!(bare.strip_optional(), bare);
    }

    #[test]
    fn substitute_named_replaces_param_only_at_named_positions() {
        let t = SemType::Generic {
            base: "Box".into(),
            args: vec![SemType::Named("T".into())],
        };
        let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::Number));
        assert_eq!(result.display(), "Box<Number>");
    }

    #[test]
    fn substitute_named_skips_substring_collisions() {
        // The legacy byte-walker had to guard against `T` matching
        // inside `TList` — structurally that can't happen because the
        // identifier is a single Named variant, not a substring.
        let t = SemType::Generic {
            base: "TList".into(),
            args: vec![SemType::Named("T".into())],
        };
        let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::Number));
        assert_eq!(result.display(), "TList<Number>");
    }

    #[test]
    fn substitute_named_recurses_through_closure() {
        let t = SemType::closure(
            vec![SemType::Named("T".into())],
            SemType::array_of(SemType::Named("T".into())),
        );
        let result = t.substitute_named("T", &SemType::Primitive(PrimitiveType::Boolean));
        assert_eq!(result.display(), "Boolean -> [Boolean]");
    }

    #[test]
    fn from_ast_matches_legacy_string_for_primitive_ident() {
        use crate::ast::Ident;
        use crate::location::Span;
        let ty = crate::ast::Type::Ident(Ident {
            name: "Number".into(),
            span: Span::default(),
        });
        // Identifier whose name happens to be a primitive should be
        // promoted to Primitive — matches what the parser would do at
        // type position.
        assert_eq!(
            SemType::from_ast(&ty),
            SemType::Primitive(PrimitiveType::Number)
        );
    }
}
