# Feature Checklist

## Implemented Features

**Core Language**:

- Comments (single-line `//`, multi-line `/* */`, doc `///` and `//!`)
- One statement per line in a body; no terminator
- Refusal of bidirectional control characters in comments and strings
- A nesting depth limit
- Visibility modifiers (`pub`)
- Use statements (Rust-style imports with `::` and `{}`)

**Type System**:

- Primitive types (`String`, `I32`, `I64`, `F32`, `F64`, `Boolean`, `Never`)
- `for` yields a lazy `Seq<T>`, consumed exactly once by a terminal combinator
- Array types (`[Type]`)
- Dictionary types (`[KeyType: ValueType]`)
- Optional types (`Type?`)
- Tuple types (named-only)
- Generic types (`Type<T>`, `Type<T: Constraint>`)
- Closure types (`(T) -> U`, `(T, U) -> V`, `() -> T`)
- Type inference

**Definitions**:

- Struct definitions (with field defaults)
- Inherent impl blocks (methods, and static methods without `self`,
  called as `Type.method(...)`)
- Trait definitions (field requirements and method signatures)
- `impl Trait for Type` conformance blocks
- Enum definitions (with associated data, generics)
- `extern fn` declarations (with `"C"` / `"system"` ABI selection)
- `extern impl` blocks (including `extern impl String`,
  `extern impl I32`, etc. on primitive receivers)
- Function definitions with optional overloading
- Parameter conventions (`mut`, `sink`) on regular and closure params
- Default parameter values (`fn f(x: I32 = 0)`); arity checks treat
  defaulted params as optional
- Codegen attribute prefixes (`inline`, `no_inline`, `cold`)
- Let bindings (file-level, with `pub`, `mut`)
- Generic parameters on structs, traits, enums, functions and methods

**Expressions**:

- All literals (string, multi-line string, number with suffix, boolean, nil, array, dictionary)
- Literal types from the position (`let x: I64 = 5`), with a range check
- Unary operators (`-`, `!`) and binary operators (arithmetic,
  comparison, equality, logical, concatenation)
- Field access (including nested)
- Destructuring (arrays, tuples, structs); an enum value is read with
  `match` or `if let`
- Struct and enum instantiation
- Closure expressions
- Range operator (`..`)
- Correct operator precedence

**Control Flow**:

- For expressions over arrays, ranges and sequences, which yield a
  lazy `Seq<T>`
- If expressions (with boolean and optional unwrapping)
- Match expressions (exhaustive pattern matching, positional bindings)

**Generics**:

- Generic type parameters with constraints
- Generic structs, traits, enums
- Generic methods: a method's own type parameters, inferred from its
  arguments
- Generic instantiation with type arguments and inference
- Nested generics, generic arity validation
- Monomorphisation pass (`MonomorphisePass`) clones definitions per
  unique argument tuple and devirtualises trait calls on concrete receivers
- Cross-module monomorphisation: imported items (functions, impls,
  pub `let`s, generic types) are inlined into the entry module under
  qualified names so backends see one self-contained `IrModule`

**Module System**:

- Use statements, `pub use` re-exports, and module path resolution
- Visibility control
- Nested modules (`mod` blocks)

**Validation** (semantic analysis):

- Module resolution
- Symbol table building
- Type resolution
- Expression validation
- Trait conformance validation
- Cycle detection
- Function overload resolution by labels, argument count and
  argument types
- Exclusive access: a `mut` or `sink` argument shares its value with no
  other argument of the same call
- Sequences: each one consumed exactly once, and never stored
- Private types in public signatures, and private items across modules
- Monomorphisation depth limit (E148)

**Source Spans** (for tooling / source maps / DWARF):

- Every `IrExpr`, definition, and `IrBlockStatement` carries an
  `IrSpan { span: Span { start, end }, file: FileId }`
- `IrModule.file_table` resolves `FileId` to a `PathBuf`; cross-module
  clones have their `FileId`s remapped onto the entry module's table

**Serde**:

- The `serde` feature (off by default): serialize and deserialize
  for `IrModule` and every type in it. The JSON form is not a stable
  format.
- The AST has no serialized form.
- `#[non_exhaustive]` on public enums and structs

## Not Yet Implemented

- Incremental compilation (salsa)
- Code formatter
- REPL mode
- Evaluation/expansion stage (runtime)
