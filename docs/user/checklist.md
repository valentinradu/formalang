# Feature Checklist

## Implemented Features

**Core Language**:

- Comments (single-line `//`, multi-line `/* */`, doc `///` and `//!`)
- Visibility modifiers (`pub`)
- Use statements (Rust-style imports with `::` and `{}`)

**Type System**:

- Primitive types (`String`, `I32`, `I64`, `F32`, `F64`, `Boolean`, `Path`, `Regex`, `Never`)
- Array types (`[Type]`)
- Dictionary types (`[KeyType: ValueType]`)
- Optional types (`Type?`)
- Tuple types (named-only)
- Generic types (`Type<T>`, `Type<T: Constraint>`)
- Closure types (`T -> U`, `T, U -> V`, `() -> T`)
- Type inference

**Definitions**:

- Struct definitions
- Inherent impl blocks (methods)
- Trait definitions (field requirements and method signatures)
- `impl Trait for Type` conformance blocks
- Enum definitions (with associated data, generics)
- `extern fn` declarations (with `"C"` / `"system"` ABI selection)
- `extern impl` blocks
- Function definitions with optional overloading
- Codegen attribute prefixes (`inline`, `no_inline`, `cold`)
- Let bindings (file-level, with `pub`, `mut`)
- Generic parameters on structs, traits, enums

**Expressions**:

- All literals (string, multi-line string, number with suffix, boolean, nil, path, regex, array, dictionary)
- Binary operators (arithmetic, comparison, equality, logical, concatenation)
- Field access (including nested)
- Destructuring (arrays, structs, enums)
- Struct and enum instantiation
- Closure expressions
- Range operator (`..`)
- Correct operator precedence

**Control Flow**:

- For expressions (array iteration)
- If expressions (with boolean and optional unwrapping)
- Match expressions (exhaustive pattern matching)

**Generics**:

- Generic type parameters with constraints
- Generic structs, traits, enums
- Generic instantiation with type arguments and inference
- Nested generics, generic arity validation
- Monomorphisation pass (`MonomorphisePass`) clones definitions per
  unique argument tuple and devirtualises trait calls on concrete receivers

**Module System**:

- Use statements and module path resolution
- Visibility control
- Nested modules (`mod` blocks)

**Validation** (semantic analysis):

- Module resolution
- Symbol table building
- Type resolution
- Expression validation
- Trait conformance validation
- Cycle detection
- Function overload resolution

**Serde**:

- `format_version` on `File`
- Full serialize/deserialize round-trip for all public AST types
- `#[non_exhaustive]` on public enums and structs

## Not Yet Implemented

- Incremental compilation (salsa)
- Code formatter
- REPL mode
- VSCode extension (full integration)
- Evaluation/expansion stage (runtime)
