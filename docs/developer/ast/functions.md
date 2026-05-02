# Functions & Parameters

`FnDef` is the body-bearing function shape used inside impl blocks;
`FnSig` is the body-less shape used to declare trait-required methods;
`FunctionDef` is the standalone (top-level) form. They share the same
parameter and attribute machinery.

## FnDef

Function definition inside an impl block.

```rust
pub struct FnDef {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Option<Expr>,           // None for extern fn / extern impl methods
    pub attributes: Vec<AttributeAnnotation>,  // inline / no_inline / cold prefixes
    pub span: Span,
}
```

`attributes` carries codegen-hint keyword prefixes parsed before
`fn` — `inline fn foo() { ... }`, `cold fn rare() { ... }`. The
frontend passes them through unchanged; backends decide whether to
honour them.

## FnSig

A signature-only function declaration (no body). Used in trait method
declarations.

```rust
pub struct FnSig {
    pub name: Ident,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub attributes: Vec<AttributeAnnotation>,  // inline / no_inline / cold
    pub span: Span,
}
```

## ParamConvention

Controls how a parameter receives its argument (Mutable Value Semantics).

```rust
#[non_exhaustive]
#[derive(Default)]
pub enum ParamConvention {
    #[default]
    Let,   // Immutable reference — the callee cannot mutate the value
    Mut,   // Exclusive mutable access — callee may mutate the value
    Sink,  // Ownership transfer — the binding is consumed at the call site
}
```

Syntax summary (`Let` is the Rust enum variant name — there is no `let`
keyword in FormaLang parameter position):

| Variant | FormaLang parameter syntax | Meaning                              |
|---------|----------------------------|--------------------------------------|
| `Let`   | `fn f(x: T)` (no keyword)  | Default; callee reads the value      |
| `Mut`   | `fn f(mut x: T)`           | Callee may mutate; arg must be `mut` |
| `Sink`  | `fn f(sink x: T)`          | Callee owns the value; arg is moved  |

All three use the same call syntax: `f(x)`. There is no annotation at the
call site.

Semantic rules enforced during validation:

- A `Mut` parameter requires that the argument binding is declared
  `let mut` (or is another `mut` / `sink` parameter). Passing an
  immutable binding produces `MutabilityMismatch`.
- A `Sink` parameter consumes the argument binding. Any subsequent use
  of that binding produces `UseAfterSink`.

`self` parameters follow the same conventions: `fn f(self)`,
`fn f(mut self)`, `fn f(sink self)`.

## FnParam

```rust
pub struct FnParam {
    pub convention: ParamConvention, // Let (default), Mut, or Sink
    pub external_label: Option<Ident>, // External call-site label (e.g., `to` in `fn send(to name: String)`)
    pub name: Ident,
    pub ty: Option<Type>,            // None for bare `self` parameter
    pub default: Option<Expr>,       // Default value expression
    pub span: Span,
}
```

`external_label` mirrors Swift's argument-label convention:
`fn send(to recipient: String)` creates a parameter whose external label
is `to` and whose internal name is `recipient`. Callers write
`send(to: "Alice")`. When `external_label` is `None`, the internal `name`
is used at the call site.

## FunctionDef (standalone)

```rust
pub struct FunctionDef {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Option<Expr>,           // None for `extern fn` declarations
    pub extern_abi: Option<ExternAbi>, // Some(_) for `extern fn`; None otherwise
    pub attributes: Vec<AttributeAnnotation>,  // inline / no_inline / cold
    pub span: Span,
}
```

`extern_abi` carries the FFI calling convention. Source forms:

| Source                      | `extern_abi`               |
|-----------------------------|----------------------------|
| `fn foo() { ... }`          | `None`                     |
| `extern fn foo()`           | `Some(ExternAbi::C)`       |
| `extern "C" fn foo()`       | `Some(ExternAbi::C)`       |
| `extern "system" fn foo()`  | `Some(ExternAbi::System)`  |

Unknown ABI strings are rejected at parse time. The convenience method
`is_extern()` returns `extern_abi.is_some()` for the common boolean check.

## FunctionAttribute

Codegen-hint keyword prefixes parsed before `fn`. The frontend passes
them through unchanged; backends with inlining heuristics or
section-placement controls consume them as hints.

```rust
pub enum FunctionAttribute {
    Inline,    // `inline fn`
    NoInline,  // `no_inline fn`
    Cold,      // `cold fn`
}
```

Multiple prefixes can stack: `pub cold no_inline fn rare_path() { ... }`.

## AttributeAnnotation

The AST stores attributes as `AttributeAnnotation`, a thin wrapper that
pairs a `FunctionAttribute` with the source span of the keyword that
introduced it. Diagnostics can cite the exact `inline` / `cold` keyword
token (e.g. for duplicate-annotation errors). IR lowering drops the span
and stores plain `FunctionAttribute`s, so `IrModule` JSON is unchanged.

```rust
pub struct AttributeAnnotation {
    pub kind: FunctionAttribute,
    pub span: Span,
}
```
