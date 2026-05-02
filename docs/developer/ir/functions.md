# Functions

`IrFunction` is the body-bearing function shape used both for standalone
functions on `IrModule.functions` and for impl methods on `IrImpl.functions`.
`IrFunctionSig` is the body-less shape used for trait-required methods on
`IrTrait.methods`.

## IrFunction

```rust
pub struct IrFunction {
    /// Function name
    pub name: String,

    /// Generic type parameters declared on the function itself
    /// (e.g. `fn identity<T>(x: T) -> T`). Empty for impl methods —
    /// method-level generics aren't yet supported; enclosing-type
    /// generics live on the containing `IrImpl` / `IrStruct`.
    pub generic_params: Vec<IrGenericParam>,

    /// Parameters (first is `self` for methods; no `self` for standalone functions)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Function body expression (None for extern functions)
    pub body: Option<IrExpr>,

    /// Calling convention when the function is `extern`. `None` for
    /// regular functions; `Some(ExternAbi::C)` for `extern fn` /
    /// `extern "C" fn`; `Some(ExternAbi::System)` for
    /// `extern "system" fn`.
    pub extern_abi: Option<ExternAbi>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`)
    /// declared as keyword prefixes before `fn`.
    pub attributes: Vec<FunctionAttribute>,

    /// Joined `///` doc comments preceding this function.
    pub doc: Option<String>,
}

impl IrFunction {
    /// Whether this function is declared `extern`. Convenience
    /// wrapper over `extern_abi.is_some()`.
    pub const fn is_extern(&self) -> bool;
}
```

## IrFunctionSig

A signature-only function declaration (no body). Used for required
methods declared in traits.

```rust
pub struct IrFunctionSig {
    /// Function name
    pub name: String,

    /// Parameters (first is typically `self`)
    pub params: Vec<IrFunctionParam>,

    /// Return type (None = unit/void)
    pub return_type: Option<ResolvedType>,

    /// Codegen-hint attributes (`inline`, `no_inline`, `cold`).
    pub attributes: Vec<FunctionAttribute>,
}
```

## IrFunctionParam

```rust
pub struct IrFunctionParam {
    /// Parameter name
    pub name: String,

    /// Parameter type (None for bare `self`)
    pub ty: Option<ResolvedType>,

    /// Default value expression, if any
    pub default: Option<IrExpr>,

    /// Parameter passing convention
    pub convention: ParamConvention,
}
```

### ParamConvention in the IR

`ParamConvention` is re-exported from `formalang::ast`. Backends should
interpret it as follows:

| Variant  | Meaning for the backend                                                                       |
|----------|-----------------------------------------------------------------------------------------------|
| `Let`    | Immutable read access. The backend may pass by reference or copy.                             |
| `Mut`    | Exclusive mutable access. The backend must ensure no aliasing.                                |
| `Sink`   | Ownership transfer. The value is logically moved; the caller cannot use it after this call.   |

All three conventions use identical call syntax in FormaLang source;
the distinction is purely semantic. Backends that target languages with
explicit ownership (Rust, C++ move semantics, Swift inout) should map
directly. Backends targeting garbage-collected languages (TypeScript,
Python) may treat all three as pass-by-value and ignore the distinction.

```rust
use formalang::ast::ParamConvention;

fn emit_param(param: &IrFunctionParam) {
    match param.convention {
        ParamConvention::Let  => { /* pass by value / reference */ }
        ParamConvention::Mut  => { /* pass as mutable / inout */ }
        ParamConvention::Sink => { /* consume / move */ }
    }
}
```

## ExternAbi

The FFI calling convention of an `extern` function.

```rust
pub enum ExternAbi {
    C,       // `extern fn` (default) or `extern "C" fn`
    System,  // `extern "system" fn`  (stdcall on Win32 x86, C elsewhere)
}
```

Unknown ABI strings (`extern "rustcall" fn ...`) are rejected at
parse time. A backend-side mapping by function name still owns
symbol-name overrides and type marshalling rules across the FFI
boundary.

## FunctionAttribute

```rust
pub enum FunctionAttribute {
    Inline,    // `inline fn`
    NoInline,  // `no_inline fn`
    Cold,      // `cold fn`
}
```

Source syntax stacks freely with `pub` and `extern`:
`pub cold extern fn abort() -> Never`. The frontend passes
attributes through unchanged; backends decide whether to honour them.
