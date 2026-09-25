# Compiler Pipeline

```text
Source → Lexer → Parser → Semantic Analyzer → IR Lowering → (Plugin System)
           │        │            │                 │               │
           ▼        ▼            ▼                 ▼               ▼
        Tokens     AST     Validated AST       IrModule      IrPass / Backend
```

The compiler puts the statements of the prelude (`src/prelude.fv`)
before the statements of each module. The prelude declares `Optional`,
`Array`, `Seq`, `Dictionary`, `Range`, the methods of these types and
of `String`, and `assert`. A program uses these names with no `use`.

## Lexer

The lexer (`src/lexer`) makes tokens with `logos`. A callback scans each
string literal by hand, so a long literal does not use the stack, and a
wrong escape does not hide the closing quote. The lexer reports these
errors:

| Variant | Code | Cause |
| --- | --- | --- |
| `InvalidCharacter` | E030 | A character that starts no token |
| `UnterminatedString` | E031 | A line break or the end of the input cuts off a string |
| `InvalidNumber` | E032 | A slice that starts with a digit and is not a numeric literal |
| `UnterminatedBlockComment` | E033 | A `/*` has no `*/` |
| `InvalidUnicodeEscape` | E034 | `\u` has no four hex digits, or they are not a Unicode scalar value |
| `InvalidEscape` | E035 | A backslash starts no escape, for example `\q` |
| `BidirectionalControl` | E036 | A comment or a string holds a bidirectional control character (CVE-2021-42574). The escape `\u202E` stays legal |

## Parser

The parser (`src/parser`) makes the AST from the tokens with `chumsky`.
Binary operators use Pratt precedence. Before the parse, one pass over
the tokens computes a nesting score. A score above 1024 gives a
`ParseError` (E001), and the parser does not start. The parser runs on
a thread of its own, with a stack that grows with the score. So a deep
program gives an error and does not abort the process.

See [Parsing](../ast/overview.md) for the score, the recovery after a
wrong statement, the end of a statement and the closure guard.

## Semantic analyzer

`SemanticAnalyzer` (`src/semantic`) checks the AST and builds the
`SymbolTable`. It does not evaluate or expand the program. It runs
these passes in order (`run_passes` in `src/semantic/mod.rs`):

| Pass | Work |
| --- | --- |
| 0 | Resolve each `use`. Analyze and lower each imported module (see [Modules](#modules)) |
| 1 | Build the symbol table. Bring in the names of each `use` of an inline module |
| 1.1 | Change each `EnumInstantiation` whose name is a value into a field access or a method call (`value_paths.rs`). Make each `-<numeric literal>` one negative literal |
| 1.5 | Validate the generic parameters |
| 1.6 | Infer the types of the module-level `let` bindings, and record the captures of their closures |
| 2 | Resolve the type references |
| 3 | Validate the expressions. Then write the literal types into the AST |
| 4 | Validate the trait implementations |
| 5 | Detect circular dependencies in `let` bindings and in field types |

Inference and validation use `SemType`, a structural type. A type that
the analyzer cannot compute is `SemType::Unknown`, and a check asks
`is_indeterminate()` before it reports. No string sentinel such as
`"Unknown"` stays in the analyzer. `LetInfo::inferred_type` holds a
`SemType` too.

The analyzer returns each error once, in source order. Several passes
can find one mistake, so `analyze_and_classify` removes the copies.

### Types from the position

An unsuffixed numeric literal takes its type from its position, when
the position declares a type of the same kind. `let big: I64 =
3000000000` holds an `I64`. An integer literal takes `I32` or `I64`. A
float literal takes `F32` or `F64`. With no such type, the literal is
an `I32` or an `F64`.

The expected type goes down from a declared type (a `let` annotation, a
parameter, a field or a return type). It goes through a group, the
branches of `if` and `match`, the result of a block, and the elements of
an array, a tuple and a dictionary (`validation/expected.rs`). IR
lowering follows the same positions.

Pass 3 records the type of each such literal in a map. The key is the
address of the literal node (`literal_types::node_key`). After pass 3,
`apply_literal_types` walks the AST with `ast_walk::visit_exprs_mut` and
writes each type into the literal as a suffix. So IR lowering reads the
same type from the AST. The key is an address, so the AST must not move
between the check and the write. Validation thus reads the AST in place
and never clones it.

The dot form of an enum value, `.ok(value: 1)`, takes its enum and the
type arguments of a generic enum from the expected type too
(`validation/expr/enums.rs`). The analyzer records the full type of the
value by node address, for the inference of the containers around it.

### Calls and overloads

A call gives each argument by position or by label
(`invocation/call_shape.rs`). The call must give each parameter that has
no default. It must not give a parameter twice, and each label must
name a parameter.

When several functions have one name, the analyzer chooses the overload
whose parameter types fit the arguments (`invocation/resolution.rs`).
It compares the types exactly first. When no overload fits exactly, an
unsuffixed literal fits any numeric type of its kind. Among the
overloads that fit, the one with the fewest defaults wins. No fit gives
`NoMatchingOverload`, and two equal fits give `AmbiguousCall`.

The analyzer records each choice in the `SymbolTable`, keyed by the span
of the call and the name of the function
(`record_overload_choice`). IR lowering reads the record
(`overload_choice`), so the two phases always choose the same overload.

### Match arms

A `match` arm, and an `if let`, push a frame of typed bindings on the
inference scope stack (`inference/match_scope.rs`). The bindings are
positional: the first binding takes the type of the first field of the
variant, and so on. For a generic enum, the frame puts in the type
arguments of the scrutinee: an arm of a `Result<I32, String>` binds the
`ok` payload as an `I32`. For an optional, `.some(x)` binds `x` to the
inner type. The arm body sees these types, and the branch types unify
with them.

### Bound methods

In a generic function, a method call on a value of a type parameter
reads the method from the bound (`bound_methods.rs`,
`validation/method_call/bound.rs`). With `T: Container<I32>`, the call
`b.get()` means `get` of `Container`, and the trait's own type
parameters read as `I32`. The call gets the same checks as any other
method call: the labels, the count and the type of each argument. The
search goes through the parent traits of the bound.

### Dictionary keys

A dictionary key needs an equality that a backend can hash
(`type_resolution/key_types.rs`). `String`, `I32`, `I64` and `Boolean`
are key types. A struct or an enum is a key type when each field and
each payload is one. A float, a closure, an array, a dictionary, a
tuple and an optional are not key types.

### Sequences

A `Seq<T>` is not a value that a program can store
(`validation/sequence_placement.rs`). A sequence may be a local `let`,
a `for` source, a `sink` parameter and the return type of an
`extern fn`. It may not be a field, a module-level `let`, the return
type of a function or a closure, an element of an array, a tuple or a
dictionary, or a part of another type. A wrong position gives
`SeqInvalidPosition` (E137).

A sequence runs once, so the program must read it exactly once on each
path (`validation/sequence_linear.rs`). A second read gives
`SeqUsedTwice` (E136). A read in a body that can run more than once, a
`for` body or a closure argument such as the closure of `fold`, is a
second read too. A sequence that a path does not read gives
`SeqNotConsumed` (E135): for example, one branch of an `if` reads it and
the other does not.

### Ownership

The language uses Mutable Value Semantics. A `sink` argument consumes
its binding, and a later use gives `UseAfterSink` (E071). Two arguments
of one call must not reach the same place when one of them is `mut` or
`sink`: `OverlappingArguments` (E144, `validation/exclusivity.rs`).

Each `for` body and each closure body pushes an outer frame
(`validation/outer_frames.rs`). The frame holds the names in scope where
the body starts. These rules use it:

- A `sink` of an outer binding in such a body runs again on the next
  pass. That is a use after the sink.
- A closure does not assign to a binding that it captures, and it does
  not give one to a `mut` parameter.

### Private in public

A public definition must not name a private type
(`validation/private_in_public.rs`). The check covers the parameters,
the return type and the bounds of a public function, the fields of a
public struct, the payloads of a public enum, the fields and the
methods of a public trait, and the type of a public `let`, written or
inferred. A wrong definition gives `PrivateTypeInPublic` (E141).

### Other checks

- An impl on a primitive type must be an `extern impl`:
  `ImplOnPrimitive` (E149).
- An expression deeper than 500 levels gives `ExpressionDepthExceeded`
  (E130). The parser limit comes first for most programs.

### Modules

Pass 0 resolves each `use` through the `ModuleResolver`. Each imported
module gets a new analyzer of its own with the prelude, so no state of
the importing module goes into it. Its errors go to the caller.

`module_links.rs` gives each module file an id when a `use` first names
it, and keeps the module path that the source wrote. When the analysis
of an imported module succeeds, the module lowers to IR on top of the
modules that it imports, and `ModuleLinks` keeps that linked IR. The
entry module lowers last, on top of all of them.

A `use` path whose first name is both an inline `mod` of the file and a
module file gives `AmbiguousModulePath` (E160). A module cannot import
a name of the prelude, and an imported name leaves a module again only
through a `pub use`.

## IR lowering

IR lowering turns the checked AST and the symbol table into a
type-resolved `IrModule`.

Module nesting is **flattened in the per-type vectors**: an inline
`mod foo { struct Bar { ... } }` lowers to a top-level
`IrStruct { name: "foo::Bar", ... }`. A backend that does not care about
the source structure sees a flat list of definitions with qualified
names. A parallel tree, `IrModule.modules: Vec<IrModuleNode>`, mirrors
the `mod` hierarchy with the ids of each module, for a backend that
needs namespaced output.

The result is one self-contained module. Each lowering starts from a
copy of the lowered prelude. The linker then copies each item of each
imported module into it. The items of an imported module have hidden
names such as `@2::helper` while the program lowers. At the end,
`finish_entry` gives them the module path that the source wrote, such
as `utils::helper`. See [Obtaining the IR](../ir/obtaining.md) for the
details.

## Plugin system

External `IrPass` transforms and `Backend` emitters compose through a
`Pipeline`. See [Plugin System](plugins.md) and
[Built-in Passes](passes.md).

## Compiler outputs

| Output | Type       | Use case                                   |
| ------ | ---------- | ------------------------------------------ |
| AST    | `File`     | Syntax analysis, source-level tooling, LSP |
| IR     | `IrModule` | Code generation, type-aware analysis       |

See the [AST Reference](../ast/overview.md) and [IR Reference](../ir/overview.md)
for the data shapes each phase produces.
