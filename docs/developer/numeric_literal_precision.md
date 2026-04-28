# Numeric-literal precision

**Status:** proposed
**Driver:** formawasm backend (round-trip integer literals into wasm
`i32.const` / `i64.const`)
**Scope:** lexer, AST, IR, every match site that destructures
`NumberLiteral`, fixtures.

## Problem

`ast::NumberLiteral` carries every numeric literal — integer or
float, any width — as a single `f64`:

```rust
// src/ast/mod.rs
pub struct NumberLiteral {
    pub value: f64,
    pub suffix: Option<NumericSuffix>,   // I32 | I64 | F32 | F64
    pub kind:   NumberSourceKind,        // Integer | Float
}
```

`f64`'s mantissa is 52 bits, so any `I64` literal whose magnitude
exceeds `2^53` (~`9_007_199_254_740_992`) silently rounds the moment
the lexer parses it. Examples:

- `9223372036854775807I64` (i64::MAX) becomes `9_223_372_036_854_775_808.0` when round-tripped — off by one.
- `123456789012345678I64` becomes `123_456_789_012_345_680.0` — last two digits lost.

Because the rounding happens at lex time, no downstream pass can
recover the original digits. **Every backend that emits integer
literals as native integers (wasm `i64.const`, JVM `ldc`, native
`mov $imm, %rax`) is silently corrupted.**

The float case is fine — `F32` / `F64` literals have always been
`f64`-shaped values, and the backend can cast `as f32` for `F32`.

## Goal

Preserve full precision for integer-syntax literals through the IR.
Float-syntax literals continue to use `f64`.

## Proposal

Replace `NumberLiteral.value: f64` with a discriminated union that
keeps the integer payload exact:

```rust
// src/ast/mod.rs
pub struct NumberLiteral {
    pub value:  NumberValue,
    pub suffix: Option<NumericSuffix>,
    pub kind:   NumberSourceKind,
}

pub enum NumberValue {
    /// Lexed from integer syntax. Preserves the exact digits.
    Integer(i128),
    /// Lexed from float syntax (digits include `.` or `e`).
    Float(f64),
}
```

`i128` covers `i64::MIN..=u64::MAX` plus a margin for unsigned types
that may land later, with no precision loss. The lexer chooses the
variant from the source-syntax kind, which it already tracks
(`NumberSourceKind`). The two enum arms make `NumberSourceKind`
redundant — you can drop the field, or keep it as a pure-source
metadata field if external tooling already depends on it.

### Range checks

Suffixes constrain the legal range:

| Suffix | Allowed range (integer syntax) |
| --- | --- |
| `I32` | `i32::MIN..=i32::MAX` |
| `I64` | `i64::MIN..=i64::MAX` |
| `F32` / `F64` | conversion via `as` from `i128` (some loss possible; existing behaviour) |
| no suffix, integer syntax | default `I32`; reject if outside `i32::MIN..=i32::MAX` |
| no suffix, float syntax | default `F64`; integer-only `i128` content is widened to `f64` losslessly when `<= 2^53` |

Reject out-of-range literals at semantic-analysis time with a typed
`CompilerError::NumericOverflow { written: String, target: PrimitiveType }`.

### Helpers

```rust
impl NumberValue {
    pub fn as_i32(&self) -> Option<i32>;   // None if out of range
    pub fn as_i64(&self) -> Option<i64>;
    pub fn as_f32(&self) -> f32;           // best-effort cast
    pub fn as_f64(&self) -> f64;           // best-effort cast
}
```

Backends call `as_i32` / `as_i64` after the semantic pass has already
range-checked, so failures are upstream-invariant violations rather
than user errors.

## Migration

Touch points (search-and-replace, then recompile):

1. `src/lexer/` — produce `NumberValue::Integer(i128)` from integer
   syntax, `NumberValue::Float(f64)` from float syntax.
2. `src/ast/mod.rs` — type definitions.
3. `src/parser/` — propagate.
4. `src/ir/lower/` — propagate.
5. `src/ir/fold.rs` — constant folding currently does `f64`
   arithmetic; split paths or convert to `f64` only at the leaf.
6. `src/ir/monomorphise.rs`, `src/ir/closure_conv.rs`,
   `src/ir/dce.rs` — only matters if they pattern-match on
   `NumberLiteral.value`; check.
7. `src/semantic/` — add range-check pass.
8. `tests/` and `tests/fixtures/` — any literal fixture above
   `2^53` exercises the precision win; add at least one
   `i64::MAX` round-trip test.

## Test plan

```rust
#[test]
fn i64_max_roundtrips_exactly() -> TestResult {
    let module = compile_to_ir("pub fn answer() -> I64 { 9223372036854775807I64 }")?;
    // Walk to the literal node, assert the i128 value matches.
    let lit = first_literal(&module)?;
    match lit.value {
        NumberValue::Integer(v) => {
            if v != 9_223_372_036_854_775_807_i128 {
                return Err(format!("lost precision: got {v}").into());
            }
        }
        NumberValue::Float(_) => return Err("expected Integer variant".into()),
    }
    Ok(())
}

#[test]
fn i32_overflow_rejected_at_semantic_analysis() -> TestResult {
    let result = compile_to_ir("pub fn x() -> I32 { 2147483648I32 }");
    match result {
        Err(errors) if errors.iter().any(|e| matches!(e, CompilerError::NumericOverflow { .. })) => Ok(()),
        other => Err(format!("expected NumericOverflow, got {other:?}").into()),
    }
}
```

## Out of scope

- `U32` / `U64` / `U128` / `BigInt` types — current language has
  signed-only primitives. If unsigned variants land later,
  `i128`'s range covers `u64` as well.
- Hex / octal / binary literal syntax — orthogonal lexer work.
- Decimal-floating literal precision (separate from integer).
