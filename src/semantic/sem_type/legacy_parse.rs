//! Parser for the legacy stringly-typed type format. Symmetric with
//! [`SemType::display`] — every shape `display` emits round-trips back
//! through [`SemType::from_legacy_string`].

use super::{primitive_from_name, SemType};

impl SemType {
    /// Parse a legacy type-string into a structural [`SemType`].
    ///
    /// Accepts the format produced by `type_to_string` plus the three
    /// sentinels (`Unknown`, `InferredEnum`, `Nil`). Unrecognised
    /// shapes fall back to [`SemType::Named`] holding the trimmed
    /// input — this matches the legacy behaviour of treating any
    /// unknown bare identifier as a user-defined type.
    pub(in crate::semantic) fn from_legacy_string(s: &str) -> Self {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Self::Unknown;
        }
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
        // Tuple `(name: T, ...)` — needs a depth-0 colon to disambiguate
        // from a parenthesised single type.
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
            if depth_zero_colon_index(inner).is_some() {
                return Self::Tuple(parse_tuple_fields(inner));
            }
            return Self::from_legacy_string(inner);
        }
        // Generic `Base<T1, T2>` — `<` somewhere in the body, `>` at end.
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
        if let Some(p) = primitive_from_name(trimmed) {
            return Self::Primitive(p);
        }
        Self::Named(trimmed.to_string())
    }
}

/// True if all bracket pairs `( ) [ ] < >` in `s` are balanced.
pub(super) fn depth_zero_balanced(s: &str) -> bool {
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
pub(super) fn depth_zero_arrow(s: &str) -> Option<usize> {
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
pub(super) fn depth_zero_colon_index(s: &str) -> Option<usize> {
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
