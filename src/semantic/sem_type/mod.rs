//! Structural representation of a type used by semantic inference,
//! validation, and the symbol table.
//!
//! The canonical "type couldn't be determined" form is
//! `SemType::Unknown`; there is no string-sentinel equivalent.
//!
//! `SemType` is ID-free on purpose: semantic analysis runs before IR
//! lowering assigns IDs. Names are sufficient at this layer.
//!
//! Storage: `SemanticAnalyzer::local_let_bindings`,
//! `SemanticAnalyzer::inference_scope_stack`, and
//! `SymbolTable::LetInfo::inferred_type` all hold `SemType` directly.
//! No round-trip through display / parse.

#[cfg(test)]
mod tests;

use crate::ast::{ParamConvention, PrimitiveType, Type};

/// Structural type used during semantic analysis.
///
/// Re-exported from `lib.rs` so downstream tooling (LSP queries, custom
/// analysis passes) can consume the analyzer's type information without
/// stringly-typed round-trips.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SemType {
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
    /// Type could not be determined. Propagates through composition
    /// (any operation involving `Unknown` yields `Unknown`); callers
    /// gate on [`Self::is_indeterminate`].
    Unknown,
    /// `.variant(...)` syntax whose enum is inferred from context.
    InferredEnum,
    /// `nil` literal.
    Nil,
}

impl SemType {
    /// Construct an array shape from an element type.
    pub(super) fn array_of(inner: Self) -> Self {
        Self::Array(Box::new(inner))
    }

    /// The type each step of `for x in self` binds: an array's
    /// element, a range's bound, or a sequence's element.
    ///
    /// `Unknown` for anything else. A non-iterable collection is
    /// reported by `validate_for_loop`, so this stays quiet rather
    /// than raising a second diagnostic.
    pub(super) fn iteration_element(&self) -> Self {
        match self {
            Self::Array(inner) => (**inner).clone(),
            Self::Generic { base, args } if base == "Range" || base == "Seq" => {
                args.first().cloned().unwrap_or(Self::Unknown)
            }
            Self::Primitive(_)
            | Self::Named(_)
            | Self::Optional(_)
            | Self::Tuple(_)
            | Self::Generic { .. }
            | Self::Dictionary { .. }
            | Self::Closure { .. }
            | Self::Unknown
            | Self::InferredEnum
            | Self::Nil => Self::Unknown,
        }
    }

    /// Construct a sequence shape from an element type.
    ///
    /// A sequence has no sugar syntax — it is always written
    /// `Seq<T>` — so it takes the same `Generic` shape an annotation
    /// produces. A dedicated variant would not compare equal to one.
    pub(super) fn seq_of(inner: Self) -> Self {
        Self::Generic {
            base: "Seq".to_string(),
            args: vec![inner],
        }
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

    /// The type that covers both `self` and `other`.
    ///
    /// An array or dictionary literal takes its type from its
    /// elements, and the elements need not all name the same one:
    /// `[1, nil]` is an array of `I32?`, and neither element says so on
    /// its own. Taking the first element's type instead — which is what
    /// inference used to do — typed that literal `[I32]` and then
    /// rejected it against a declared `[I32?]`.
    ///
    /// Four cases join; anything else is a genuine mismatch and comes
    /// back [`Self::Unknown`], which leaves the element check in
    /// `validation::expr::literals` to report it.
    pub(super) fn join(self, other: Self) -> Self {
        if self == other {
            return self;
        }

        match (self, other) {
            // Nothing is known about one side: take the other.
            (Self::Unknown, known) | (known, Self::Unknown) => known,

            // `nil` widens whatever it meets to an optional.
            (Self::Nil, known) | (known, Self::Nil) => Self::optional_of(known),

            // `T` and `T?` meet at `T?`.
            (Self::Optional(inner), plain) | (plain, Self::Optional(inner)) if *inner == plain => {
                Self::Optional(inner)
            }

            // Two arrays join element-wise, so `[[1], [nil]]` works the
            // same way one level down.
            (Self::Array(a), Self::Array(b)) => Self::array_of(a.join(*b)),

            _ => Self::Unknown,
        }
    }

    /// Whether this type contains a closure, at the top level or
    /// inside a container.
    ///
    /// Equality is structural in this language, but a closure has no
    /// structure to compare: after closure conversion it is a code
    /// pointer plus a captured environment. Two closures written the
    /// same way are still two different values, so `==` on them has no
    /// answer a backend can give. [`Self::Named`] is not resolved here
    /// — the caller walks a struct's fields through the symbol table,
    /// which this type does not have.
    pub(super) fn holds_a_closure(&self) -> bool {
        match self {
            Self::Closure { .. } => true,
            Self::Array(inner) | Self::Optional(inner) => inner.holds_a_closure(),
            Self::Dictionary { key, value } => key.holds_a_closure() || value.holds_a_closure(),
            Self::Tuple(fields) => fields.iter().any(|(_, ty)| ty.holds_a_closure()),
            Self::Generic { args, .. } => args.iter().any(Self::holds_a_closure),
            Self::Primitive(_)
            | Self::Named(_)
            | Self::Unknown
            | Self::InferredEnum
            | Self::Nil => false,
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
    /// in its structure. Validation paths use this to skip when
    /// inference hasn't settled yet ("cannot validate this type yet,
    /// more inference needed").
    ///
    /// IR lowering reads it too, before stringifying a type through
    /// [`Self::display`]: the marker words `Unknown` and
    /// `InferredEnum` are not type names, so a round trip through the
    /// string form would report them as undefined types.
    pub(crate) fn is_indeterminate(&self) -> bool {
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

    /// Render to a canonical string form (e.g. `[I32]`, `Box<T>`,
    /// `(K) -> V`). Used for diagnostics, hover output, and IR-side
    /// type-string parsing during lowering.
    pub fn display(&self) -> String {
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
            // Closures always render with a parenthesised parameter list so
            // every `->` in rendered output is preceded by `)` — matching
            // the surface syntax.
            Self::Closure { params, return_ty } => {
                let rendered: Vec<String> = params.iter().map(Self::display).collect();
                format!("({}) -> {}", rendered.join(", "), return_ty.display())
            }
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
            // The built-in carriers have two spellings each:
            // `Dictionary<K, V>` and `[K: V]`, `Array<T>` and `[T]`,
            // `Optional<T>` and `T?`. They name one type, so they
            // become one `SemType` here. Keeping them apart made the
            // two halves of the language disagree: `[F64: I32]` was
            // rejected as a float key while `Dictionary<F64, I32>` was
            // not, `d["k"]` on the generic spelling was "not
            // indexable", and `let d: Dictionary<String, I32> = [:]`
            // was a type mismatch against its own value.
            Type::Generic { name, args, .. } => {
                let lowered: Vec<Self> = args.iter().map(Self::from_ast).collect();
                match (name.name.as_str(), lowered.as_slice()) {
                    ("Array", [element]) => Self::array_of(element.clone()),
                    ("Optional", [inner]) => Self::optional_of(inner.clone()),
                    ("Dictionary", [key, value]) => Self::dictionary(key.clone(), value.clone()),
                    _ => Self::Generic {
                        base: name.name.clone(),
                        args: lowered,
                    },
                }
            }
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
    ///   wrong branch.
    pub(super) fn widen_branches(a: &Self, b: &Self) -> Self {
        if a == b {
            return a.clone();
        }
        if matches!(a, Self::Nil) && !matches!(b, Self::Nil) {
            return Self::optional_of(b.clone());
        }
        if matches!(b, Self::Nil) && !matches!(a, Self::Nil) {
            return Self::optional_of(a.clone());
        }
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
    /// inside `self` with `concrete`. Structural by design so `T` in
    /// `Box<T>` is substituted but a substring `T` inside a name like
    /// `TList` cannot match (different variant shape).
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
}

const fn primitive_name(p: PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::String => "String",
        PrimitiveType::I32 => "I32",
        PrimitiveType::I64 => "I64",
        PrimitiveType::F32 => "F32",
        PrimitiveType::F64 => "F64",
        PrimitiveType::Boolean => "Boolean",
        PrimitiveType::Never => "Never",
    }
}

pub(super) fn primitive_from_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "String" => Some(PrimitiveType::String),
        "I32" => Some(PrimitiveType::I32),
        "I64" => Some(PrimitiveType::I64),
        "F32" => Some(PrimitiveType::F32),
        "F64" => Some(PrimitiveType::F64),
        "Boolean" => Some(PrimitiveType::Boolean),
        "Never" => Some(PrimitiveType::Never),
        _ => None,
    }
}
