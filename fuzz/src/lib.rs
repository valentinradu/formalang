//! Shared helpers for the `FormaLang` fuzz targets.
//!
//! The byte-level targets (`lex`, `parse`, `compile`) feed raw input into
//! the compiler. They find crashes in the lexer and the parser, but they
//! almost never reach the semantic analyser or the IR lowerer, because
//! random bytes are not a parseable program.
//!
//! [`Program`] closes that gap. It is an `Arbitrary` description of a
//! `FormaLang` program that renders to source text. Every name comes from
//! a small fixed pool, so references resolve often and the generated
//! program reaches the deep phases of the compiler.

use std::collections::HashMap;
use std::path::PathBuf;

use arbitrary::Arbitrary;
use formalang::semantic::module_resolver::{ModuleError, ModuleResolver};

/// A resolver that serves modules from memory.
///
/// The fuzz targets must not touch the filesystem: the corpus is shared
/// between runs, and a target that reads the working directory is not
/// reproducible.
#[derive(Debug, Default)]
pub struct MemResolver {
    modules: HashMap<Vec<String>, (String, PathBuf)>,
}

impl MemResolver {
    /// An empty resolver. Every `use` fails with `NotFound`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one module under `path`.
    pub fn add(&mut self, path: Vec<String>, source: &str) {
        let file = PathBuf::from(format!("{}.fv", path.join("/")));
        self.modules.insert(path, (source.to_string(), file));
    }
}

impl ModuleResolver for MemResolver {
    fn resolve(
        &self,
        path: &[String],
        _current_file: Option<&PathBuf>,
    ) -> Result<(String, PathBuf), ModuleError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| ModuleError::NotFound {
                path: path.to_vec(),
                searched_paths: Vec::new(),
            })
    }
}

// ---------------------------------------------------------------------------
// Name pools
//
// A small pool makes collisions and cross-references common, which is
// what drives the generator into the resolver, the overload picker and
// the monomorphiser.
// ---------------------------------------------------------------------------

const TYPE_NAMES: [&str; 4] = ["Alpha", "Beta", "Gamma", "Delta"];
const FN_NAMES: [&str; 5] = ["f", "g", "h", "k", "run_checks"];
const FIELD_NAMES: [&str; 4] = ["a", "b", "c", "d"];
const VAR_NAMES: [&str; 4] = ["x", "y", "z", "w"];
const VARIANT_NAMES: [&str; 4] = ["one", "two", "three", "four"];
const TRAIT_NAMES: [&str; 2] = ["Named", "Sized2"];
const MOD_NAMES: [&str; 2] = ["inner", "outer"];
const TYPE_PARAMS: [&str; 2] = ["T", "U"];
const STRINGS: [&str; 4] = ["", "hello", "a b c", "\\n"];

/// Pick one entry of `pool` by index. The index wraps, so every `u8`
/// value is valid and the generator never rejects an input.
fn pick<'a>(pool: &[&'a str], index: u8) -> &'a str {
    let len = pool.len();
    pool[(index as usize) % len]
}

/// Depth budget for rendering. Beyond it, a node renders as a leaf.
///
/// Without a cap the generator writes programs nested thousands of
/// levels deep, and every target then reports the same recursion
/// crash. Deep nesting is worth testing, but as a dedicated test with
/// a known depth — not as the fuzzer's every finding.
const MAX_DEPTH: u32 = 6;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A generated type expression.
#[derive(Arbitrary, Debug, Clone)]
pub enum GType {
    /// `I32`
    I32,
    /// `I64`
    I64,
    /// `F64`
    F64,
    /// `Boolean`
    Boolean,
    /// `String`
    Str,
    /// One of the generated struct or enum names.
    Named(u8),
    /// A type parameter name (`T` / `U`).
    Param(u8),
    /// `[T]`
    Array(Box<GType>),
    /// `[K: V]`
    Dict(Box<GType>, Box<GType>),
    /// `T?`
    Optional(Box<GType>),
    /// `(a: T, b: U)`
    Tuple(Box<GType>, Box<GType>),
    /// `(T) -> U`
    Function(Box<GType>, Box<GType>),
    /// `module::Type`
    Qualified(u8, u8),
}

impl GType {
    fn render(&self, out: &mut String, depth: u32) {
        if depth >= MAX_DEPTH {
            out.push_str("I32");
            return;
        }
        match self {
            Self::I32 => out.push_str("I32"),
            Self::I64 => out.push_str("I64"),
            Self::F64 => out.push_str("F64"),
            Self::Boolean => out.push_str("Boolean"),
            Self::Str => out.push_str("String"),
            Self::Named(n) => out.push_str(pick(&TYPE_NAMES, *n)),
            Self::Param(n) => out.push_str(pick(&TYPE_PARAMS, *n)),
            Self::Array(inner) => {
                out.push('[');
                inner.render(out, depth + 1);
                out.push(']');
            }
            Self::Dict(k, v) => {
                out.push('[');
                k.render(out, depth + 1);
                out.push_str(": ");
                v.render(out, depth + 1);
                out.push(']');
            }
            Self::Optional(inner) => {
                inner.render(out, depth + 1);
                out.push('?');
            }
            Self::Tuple(a, b) => {
                out.push_str("(a: ");
                a.render(out, depth + 1);
                out.push_str(", b: ");
                b.render(out, depth + 1);
                out.push(')');
            }
            Self::Function(a, r) => {
                out.push('(');
                a.render(out, depth + 1);
                out.push_str(") -> ");
                r.render(out, depth + 1);
            }
            Self::Qualified(m, t) => {
                out.push_str(pick(&MOD_NAMES, *m));
                out.push_str("::");
                out.push_str(pick(&TYPE_NAMES, *t));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

const BIN_OPS: [&str; 14] = [
    "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||", "??",
];

/// A generated expression.
#[derive(Arbitrary, Debug, Clone)]
pub enum GExpr {
    /// An integer literal.
    Int(i64),
    /// A float literal, rendered from an `i32` so the text is always valid.
    Float(i32, u16),
    /// `true` / `false`
    Bool(bool),
    /// A string literal from the pool.
    Str(u8),
    /// `none`
    None,
    /// A variable reference.
    Var(u8),
    /// A binary operation.
    Binary(u8, Box<GExpr>, Box<GExpr>),
    /// `-e` or `!e`
    Unary(bool, Box<GExpr>),
    /// `if c { t } else { e }`
    If(Box<GExpr>, Box<GExpr>, Box<GExpr>),
    /// `name(field: arg, ...)` — a free function call with named arguments.
    Call(u8, Vec<GExpr>),
    /// `recv.name(field: arg, ...)`
    MethodCall(Box<GExpr>, u8, Vec<GExpr>),
    /// `recv.field`
    Field(Box<GExpr>, u8),
    /// `Type(field: arg, ...)`
    StructInit(u8, Vec<GExpr>),
    /// `Type.variant(field: arg, ...)`
    EnumInit(u8, u8, Vec<GExpr>),
    /// `.variant` — an inferred-context enum literal.
    InferredEnum(u8),
    /// `[a, b, c]`
    ArrayLit(Vec<GExpr>),
    /// `[k: v, ...]`
    DictLit(Vec<(GExpr, GExpr)>),
    /// `(a: x, b: y)`
    TupleLit(Box<GExpr>, Box<GExpr>),
    /// `e[i]`
    Index(Box<GExpr>, Box<GExpr>),
    /// `(p) -> body`
    Closure(u8, Option<GType>, Box<GExpr>),
    /// `match scrutinee { ... }`
    Match(Box<GExpr>, Vec<GArm>),
    /// `for v in iter { body }.<terminal>`
    For(u8, Box<GExpr>, Box<GExpr>, u8),
    /// `a..b`
    Range(Box<GExpr>, Box<GExpr>),
    /// A block with let bindings and a tail expression.
    Block(Vec<GLet>, Box<GExpr>),
    /// `module::name(...)`
    Qualified(u8, u8, Vec<GExpr>),
}

/// One arm of a generated `match`.
#[derive(Arbitrary, Debug, Clone)]
pub struct GArm {
    /// `None` renders the wildcard arm `_`.
    pub variant: Option<u8>,
    /// Payload bindings for the variant pattern.
    pub bindings: Vec<u8>,
    /// The arm's body.
    pub body: GExpr,
}

/// One `let` binding inside a generated block.
#[derive(Arbitrary, Debug, Clone)]
pub struct GLet {
    /// The bound name.
    pub name: u8,
    /// `let mut` when true.
    pub mutable: bool,
    /// An optional type annotation.
    pub ty: Option<GType>,
    /// The bound value.
    pub value: GExpr,
}

const TERMINALS: [&str; 5] = [
    ".collect()",
    ".count()",
    ".run()",
    ".fold(initial: 0, f: (a, b) -> a + b)",
    ".filter(f: (v) -> true).collect()",
];

impl GExpr {
    fn render(&self, out: &mut String, depth: u32) {
        if depth >= MAX_DEPTH {
            out.push('0');
            return;
        }
        let d = depth + 1;
        match self {
            Self::Int(v) => {
                // `i64::MIN` has no positive literal form; the parser sees
                // `-` applied to the magnitude, so print through `i128`.
                out.push_str(&i128::from(*v).to_string());
            }
            Self::Float(whole, frac) => {
                out.push_str(&format!("{whole}.{frac}"));
            }
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Str(n) => {
                out.push('"');
                out.push_str(pick(&STRINGS, *n));
                out.push('"');
            }
            Self::None => out.push_str("none"),
            Self::Var(n) => out.push_str(pick(&VAR_NAMES, *n)),
            Self::Binary(op, l, r) => {
                out.push('(');
                l.render(out, d);
                out.push(' ');
                out.push_str(pick(&BIN_OPS, *op));
                out.push(' ');
                r.render(out, d);
                out.push(')');
            }
            Self::Unary(neg, e) => {
                out.push(if *neg { '-' } else { '!' });
                out.push('(');
                e.render(out, d);
                out.push(')');
            }
            Self::If(c, t, e) => {
                out.push_str("if ");
                c.render(out, d);
                out.push_str(" { ");
                t.render(out, d);
                out.push_str(" } else { ");
                e.render(out, d);
                out.push_str(" }");
            }
            Self::Call(name, args) => {
                out.push_str(pick(&FN_NAMES, *name));
                render_named_args(out, args, d);
            }
            Self::MethodCall(recv, name, args) => {
                recv.render(out, d);
                out.push('.');
                out.push_str(pick(&FN_NAMES, *name));
                render_named_args(out, args, d);
            }
            Self::Field(recv, name) => {
                recv.render(out, d);
                out.push('.');
                out.push_str(pick(&FIELD_NAMES, *name));
            }
            Self::StructInit(name, args) => {
                out.push_str(pick(&TYPE_NAMES, *name));
                render_named_args(out, args, d);
            }
            Self::EnumInit(ty, variant, args) => {
                out.push_str(pick(&TYPE_NAMES, *ty));
                out.push('.');
                out.push_str(pick(&VARIANT_NAMES, *variant));
                if !args.is_empty() {
                    render_named_args(out, args, d);
                }
            }
            Self::InferredEnum(variant) => {
                out.push('.');
                out.push_str(pick(&VARIANT_NAMES, *variant));
            }
            Self::ArrayLit(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.render(out, d);
                }
                out.push(']');
            }
            Self::DictLit(entries) => {
                out.push('[');
                if entries.is_empty() {
                    out.push(':');
                }
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    k.render(out, d);
                    out.push_str(": ");
                    v.render(out, d);
                }
                out.push(']');
            }
            Self::TupleLit(a, b) => {
                out.push_str("(a: ");
                a.render(out, d);
                out.push_str(", b: ");
                b.render(out, d);
                out.push(')');
            }
            Self::Index(recv, idx) => {
                recv.render(out, d);
                out.push('[');
                idx.render(out, d);
                out.push(']');
            }
            Self::Closure(param, ty, body) => {
                out.push('(');
                out.push_str(pick(&VAR_NAMES, *param));
                if let Some(ty) = ty {
                    out.push_str(": ");
                    ty.render(out, d);
                }
                out.push_str(") -> ");
                body.render(out, d);
            }
            Self::Match(scrutinee, arms) => {
                out.push_str("match ");
                scrutinee.render(out, d);
                out.push_str(" {\n");
                if arms.is_empty() {
                    out.push_str("    _: 0\n");
                }
                for (i, arm) in arms.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\n");
                    }
                    out.push_str("    ");
                    match arm.variant {
                        None => out.push('_'),
                        Some(v) => {
                            out.push('.');
                            out.push_str(pick(&VARIANT_NAMES, v));
                            if !arm.bindings.is_empty() {
                                out.push('(');
                                for (j, b) in arm.bindings.iter().enumerate() {
                                    if j > 0 {
                                        out.push_str(", ");
                                    }
                                    out.push_str(pick(&VAR_NAMES, *b));
                                }
                                out.push(')');
                            }
                        }
                    }
                    out.push_str(": ");
                    arm.body.render(out, d);
                }
                out.push_str("\n}");
            }
            Self::For(var, iter, body, terminal) => {
                out.push_str("for ");
                out.push_str(pick(&VAR_NAMES, *var));
                out.push_str(" in ");
                iter.render(out, d);
                out.push_str(" { ");
                body.render(out, d);
                out.push_str(" }");
                out.push_str(pick(&TERMINALS, *terminal));
            }
            Self::Range(a, b) => {
                a.render(out, d);
                out.push_str("..");
                b.render(out, d);
            }
            Self::Block(lets, tail) => {
                out.push_str("{\n");
                for l in lets {
                    out.push_str("    let ");
                    if l.mutable {
                        out.push_str("mut ");
                    }
                    out.push_str(pick(&VAR_NAMES, l.name));
                    if let Some(ty) = &l.ty {
                        out.push_str(": ");
                        ty.render(out, d);
                    }
                    out.push_str(" = ");
                    l.value.render(out, d);
                    out.push('\n');
                }
                out.push_str("    ");
                tail.render(out, d);
                out.push_str("\n}");
            }
            Self::Qualified(m, name, args) => {
                out.push_str(pick(&MOD_NAMES, *m));
                out.push_str("::");
                out.push_str(pick(&TYPE_NAMES, *name));
                render_named_args(out, args, d);
            }
        }
    }
}

fn render_named_args(out: &mut String, args: &[GExpr], depth: u32) {
    out.push('(');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        // Field names repeat once `args.len() > FIELD_NAMES.len()`. That
        // is deliberate: a duplicate argument label must produce an
        // error, not a panic.
        out.push_str(pick(&FIELD_NAMES, u8::try_from(i).unwrap_or(0)));
        out.push_str(": ");
        arg.render(out, depth);
    }
    out.push(')');
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

/// A parameter-passing convention.
#[derive(Arbitrary, Debug, Clone, Copy)]
pub enum GConvention {
    /// The default, by value.
    Value,
    /// `mut` — in-place mutation.
    Mut,
    /// `sink` — ownership transfer.
    Sink,
}

/// One generated function parameter.
#[derive(Arbitrary, Debug, Clone)]
pub struct GParam {
    /// The parameter name.
    pub name: u8,
    /// The parameter type.
    pub ty: GType,
    /// The passing convention.
    pub convention: GConvention,
    /// An optional default value.
    pub default: Option<GExpr>,
}

/// One generated enum variant: a name index plus its payload fields.
pub type GVariant = (u8, Vec<(u8, GType)>);

/// A generated top-level definition.
#[derive(Arbitrary, Debug, Clone)]
pub enum GItem {
    /// A struct definition.
    Struct {
        /// The struct name.
        name: u8,
        /// Declare one type parameter when true.
        generic: bool,
        /// The fields.
        fields: Vec<(u8, GType)>,
    },
    /// An enum definition.
    Enum {
        /// The enum name.
        name: u8,
        /// The variants, each with its payload fields.
        variants: Vec<GVariant>,
    },
    /// A free function.
    Function {
        /// The function name.
        name: u8,
        /// `pub` when true.
        public: bool,
        /// Declare one type parameter when true.
        generic: bool,
        /// The parameters.
        params: Vec<GParam>,
        /// The declared return type.
        ret: Option<GType>,
        /// The body.
        body: GExpr,
    },
    /// A trait definition.
    Trait {
        /// The trait name.
        name: u8,
        /// Required fields.
        fields: Vec<(u8, GType)>,
        /// Required methods, by name and return type.
        methods: Vec<(u8, GType)>,
    },
    /// An inherent or trait `impl` block.
    Impl {
        /// The implementing type.
        ty: u8,
        /// `Some` renders `impl Trait for Type`.
        trait_name: Option<u8>,
        /// The methods.
        methods: Vec<(u8, GType, GExpr)>,
    },
    /// A top-level `let`.
    Let {
        /// The bound name.
        name: u8,
        /// An optional type annotation.
        ty: Option<GType>,
        /// The bound value.
        value: GExpr,
    },
    /// An inline module.
    Module {
        /// The module name.
        name: u8,
        /// The contained definitions.
        items: Vec<GItem>,
    },
    /// An `extern fn` declaration.
    Extern {
        /// The declared name.
        name: u8,
        /// The parameters.
        params: Vec<(u8, GType)>,
        /// The return type.
        ret: Option<GType>,
    },
    /// A `use` statement. It always fails to resolve under
    /// [`MemResolver::new`], which exercises the import error paths.
    Use {
        /// The module name.
        module: u8,
        /// The imported item names.
        items: Vec<u8>,
    },
}

impl GItem {
    fn render(&self, out: &mut String, depth: u32) {
        if depth >= MAX_DEPTH {
            return;
        }
        match self {
            Self::Struct {
                name,
                generic,
                fields,
            } => {
                out.push_str("pub struct ");
                out.push_str(pick(&TYPE_NAMES, *name));
                if *generic {
                    out.push_str("<T>");
                }
                out.push_str(" {\n");
                for (i, (fname, fty)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\n");
                    }
                    out.push_str("    ");
                    out.push_str(pick(&FIELD_NAMES, *fname));
                    out.push_str(": ");
                    fty.render(out, depth + 1);
                }
                out.push_str("\n}\n\n");
            }
            Self::Enum { name, variants } => {
                out.push_str("pub enum ");
                out.push_str(pick(&TYPE_NAMES, *name));
                out.push_str(" {\n");
                if variants.is_empty() {
                    out.push_str("    one\n");
                }
                for (i, (vname, payload)) in variants.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\n");
                    }
                    out.push_str("    ");
                    out.push_str(pick(&VARIANT_NAMES, *vname));
                    if !payload.is_empty() {
                        out.push('(');
                        for (j, (pname, pty)) in payload.iter().enumerate() {
                            if j > 0 {
                                out.push_str(", ");
                            }
                            out.push_str(pick(&FIELD_NAMES, *pname));
                            out.push_str(": ");
                            pty.render(out, depth + 1);
                        }
                        out.push(')');
                    }
                }
                out.push_str("\n}\n\n");
            }
            Self::Function {
                name,
                public,
                generic,
                params,
                ret,
                body,
            } => {
                if *public {
                    out.push_str("pub ");
                }
                out.push_str("fn ");
                out.push_str(pick(&FN_NAMES, *name));
                if *generic {
                    out.push_str("<T>");
                }
                render_params(out, params, depth + 1);
                if let Some(ret) = ret {
                    out.push_str(" -> ");
                    ret.render(out, depth + 1);
                }
                out.push_str(" {\n    ");
                body.render(out, depth + 1);
                out.push_str("\n}\n\n");
            }
            Self::Trait {
                name,
                fields,
                methods,
            } => {
                out.push_str("pub trait ");
                out.push_str(pick(&TRAIT_NAMES, *name));
                out.push_str(" {\n");
                for (fname, fty) in fields {
                    out.push_str("    ");
                    out.push_str(pick(&FIELD_NAMES, *fname));
                    out.push_str(": ");
                    fty.render(out, depth + 1);
                    out.push('\n');
                }
                for (mname, mty) in methods {
                    out.push_str("    fn ");
                    out.push_str(pick(&FN_NAMES, *mname));
                    out.push_str("(self) -> ");
                    mty.render(out, depth + 1);
                    out.push('\n');
                }
                out.push_str("}\n\n");
            }
            Self::Impl {
                ty,
                trait_name,
                methods,
            } => {
                out.push_str("impl ");
                if let Some(t) = trait_name {
                    out.push_str(pick(&TRAIT_NAMES, *t));
                    out.push_str(" for ");
                }
                out.push_str(pick(&TYPE_NAMES, *ty));
                out.push_str(" {\n");
                for (mname, mty, mbody) in methods {
                    out.push_str("    fn ");
                    out.push_str(pick(&FN_NAMES, *mname));
                    out.push_str("(self) -> ");
                    mty.render(out, depth + 1);
                    out.push_str(" {\n        ");
                    mbody.render(out, depth + 1);
                    out.push_str("\n    }\n");
                }
                out.push_str("}\n\n");
            }
            Self::Let { name, ty, value } => {
                out.push_str("let ");
                out.push_str(pick(&VAR_NAMES, *name));
                if let Some(ty) = ty {
                    out.push_str(": ");
                    ty.render(out, depth + 1);
                }
                out.push_str(" = ");
                value.render(out, depth + 1);
                out.push_str("\n\n");
            }
            Self::Module { name, items } => {
                out.push_str("pub mod ");
                out.push_str(pick(&MOD_NAMES, *name));
                out.push_str(" {\n");
                for item in items {
                    item.render(out, depth + 1);
                }
                out.push_str("}\n\n");
            }
            Self::Extern { name, params, ret } => {
                out.push_str("extern fn ");
                out.push_str(pick(&FN_NAMES, *name));
                out.push('(');
                for (i, (pname, pty)) in params.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(pick(&VAR_NAMES, *pname));
                    out.push_str(": ");
                    pty.render(out, depth + 1);
                }
                out.push(')');
                if let Some(ret) = ret {
                    out.push_str(" -> ");
                    ret.render(out, depth + 1);
                }
                out.push('\n');
            }
            Self::Use { module, items } => {
                out.push_str("use ");
                out.push_str(pick(&MOD_NAMES, *module));
                if !items.is_empty() {
                    out.push_str("::{");
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(pick(&TYPE_NAMES, *item));
                    }
                    out.push('}');
                }
                out.push('\n');
            }
        }
    }
}

fn render_params(out: &mut String, params: &[GParam], depth: u32) {
    out.push('(');
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match p.convention {
            GConvention::Value => {}
            GConvention::Mut => out.push_str("mut "),
            GConvention::Sink => out.push_str("sink "),
        }
        out.push_str(pick(&VAR_NAMES, p.name));
        out.push_str(": ");
        p.ty.render(out, depth);
        if let Some(default) = &p.default {
            out.push_str(" = ");
            default.render(out, depth);
        }
    }
    out.push(')');
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

/// A whole generated program.
#[derive(Arbitrary, Debug, Clone)]
pub struct Program {
    /// The top-level definitions.
    pub items: Vec<GItem>,
}

impl Program {
    /// Render the program to `FormaLang` source text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for item in &self.items {
            item.render(&mut out, 0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbitrary::Unstructured;

    /// The generator must terminate and must produce text for any input,
    /// including an empty one.
    #[test]
    fn renders_for_any_input() {
        for seed in 0_u32..256 {
            let bytes: Vec<u8> = (0..512_u32)
                .map(|i| {
                    u8::try_from((seed.wrapping_mul(2_654_435_761).wrapping_add(i)) % 256)
                        .unwrap_or(0)
                })
                .collect();
            let mut u = Unstructured::new(&bytes);
            let program = Program::arbitrary(&mut u).expect("generator must not fail");
            let _ = program.render();
        }
    }
}
