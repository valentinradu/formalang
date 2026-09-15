<br/>

<p align="center">
  <img src="https://formalang.org/logo-180.png" alt="FormaLang" width="200">
</p>

<h3 align="center">A statically typed, declarative DSL compiler written in Rust</h3>

<p align="center">
  <a href="https://crates.io/crates/formalang"><img src="https://img.shields.io/crates/v/formalang.svg?style=flat-square&color=cba6f7&label=crates.io" alt="crates.io"></a>
  <a href="https://docs.rs/formalang"><img src="https://img.shields.io/docsrs/formalang?style=flat-square&color=cba6f7" alt="docs.rs"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT_or_Apache--2.0-cba6f7?style=flat-square" alt="license"></a>
  <a href=".github/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/valentinradu/formalang/ci.yml?style=flat-square&color=cba6f7&label=ci&branch=main" alt="ci"></a>
</p>

<p align="center">
  <a href="#why-formalang">Why</a>
  &nbsp;·&nbsp;
  <a href="#quick-start">Quick Start</a>
  &nbsp;·&nbsp;
  <a href="#language-tour">Language Tour</a>
  &nbsp;·&nbsp;
  <a href="#rust-api">Rust API</a>
  &nbsp;·&nbsp;
  <a href="https://formalang.org/docs/">Full Docs</a>
</p>

<br/>

---

You write `.fv` files; the library parses and validates them, and you get a fully type-resolved IR back. What you do with that IR (generate code, drive a UI framework, configure a system) is up to your backend. The first such backend is [`formawasm`](https://github.com/valentinradu/formawasm), which lowers the IR to WebAssembly.

```text
.fv source → formalang library → IrModule → your Backend → output
```

---

## Contents

- [Why FormaLang?](#why-formalang)
- [Quick Start](#quick-start)
- [Language Tour](#language-tour)
  - [Primitives](#primitives)
  - [Structs](#structs)
  - [Methods (impl blocks)](#methods-impl-blocks)
  - [Parameter Conventions](#parameter-conventions)
  - [Traits](#traits)
  - [Enums](#enums)
  - [Let bindings](#let-bindings)
  - [Arrays, Dictionaries, Tuples](#arrays-dictionaries-tuples)
  - [Control Flow](#control-flow)
  - [Closures](#closures)
  - [Generics](#generics)
  - [Destructuring](#destructuring)
  - [Modules](#modules)
  - [Extern declarations](#extern-declarations)
  - [Function overloading](#function-overloading)
- [Rust API](#rust-api)
  - [Entry points](#entry-points)
  - [The IrModule](#the-irmodule)
  - [Pipeline (passes + backends)](#pipeline-passes--backends)
  - [Error reporting](#error-reporting)
- [File extension](#file-extension)
- [What is not built in](#what-is-not-built-in)
- [Further reading](#further-reading)
- [License](#license)

---

## Why FormaLang?

You're building a Rust application that needs to accept user-authored logic: UI definitions, configuration with computation, state machines, scripted rules. The usual options each have a sharp edge:

- **Ship Rust as the user-facing language.** Rust is a great host but a poor guest: it's AOT-compiled, lifetimes and the borrow checker land on whoever writes the file, and you can't load `.rs` snippets at runtime without dragging in a full toolchain.
- **Embed Lua, Rhai, or JavaScript.** These are dynamically typed. Errors that should have been caught when the file was loaded surface only when the offending branch runs, usually in production.
- **Use JSON, YAML, or TOML.** No expressions, no functions, no real types. The moment your config grows a conditional, you reinvent half a language inside string templates.

FormaLang fills that gap:

- **Statically typed and fully resolved.** The library hands back an `IrModule` where every type, name, and overload is already settled. A broken `.fv` fails at load, not when the user clicks the button that runs the bad branch.
- **Embeddable by design.** A pure compiler frontend with no runtime, no I/O, no globals, no sandbox to maintain. The output is data: walk it, transform it, emit whatever you want.
- **Small surface for users.** Structs, enums, traits, closures, generics, modules. No lifetimes, no async, no unsafe, no macros. Someone fluent in Swift or Rust can read it on day one.
- **Backend-agnostic.** Drive a UI framework, generate code for any target, configure a runtime, layer custom IR passes. The compiler stops at the IR; you decide what comes next.

---

## Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
formalang = "0.0.5-beta"
```

Compile a source string:

```rust
use formalang::compile_to_ir;

let source = r#"
    pub struct User {
        name: String,
        age: I32
    }
"#;

let module = compile_to_ir(source).unwrap();
println!("{}", module.structs[0].name); // User
```

---

## Language Tour

### Primitives

```rust
let text: String = "hello"
let count: I32 = 42
let big: I64 = 9_223_372_036_854_775_807
let ratio: F64 = 3.14
let small: F32 = 0.5F32          // type-suffix pins literal precision
let flag: Boolean = true
let logo: Path = /assets/logo.svg
let pattern: Regex = r/[a-z]+/i
let nothing: String? = nil       // optional; any type can be made optional with ?
```

Numeric primitives are width-tagged: `I32`, `I64`, `F32`, `F64`. Unsuffixed
integer literals default to `I32`; unsuffixed float literals default to
`F64`. Suffix syntax is uppercase and adjacent to the digits (`42I64`,
`3.14F32`).

### Structs

```rust
pub struct Point {
    x: I32,
    y: I32
}

pub struct User {
    name: String,
    email: String,
    nickname: String?,       // optional field
    score: I32
}

// Instantiate with named arguments
let p = Point(x: 10, y: 20)
let u = User(name: "Alice", email: "alice@example.com", nickname: nil, score: 0)
// Mutability is a property of the binding, not the field; to mutate any
// field of `u` you bind it with `let mut u = User(...)`.
```

### Methods (impl blocks)

```rust
pub struct Counter {
    value: I32
}

impl Counter {
    fn increment(self) -> I32 {
        self.value + 1
    }

    fn reset(self) -> Counter {
        Counter(value: 0)
    }
}
```

### Parameter Conventions

Every function parameter has a convention controlling how the argument is received. The call site always looks the same as `f(x)`; only the function declaration changes.

```rust
// default: immutable; the callee reads the value
fn area(radius: I32) -> I32 {
    radius * radius
}

// mut: callee may mutate; argument binding must be let mut
fn bump(mut n: I32) -> I32 {
    n
}

// sink: ownership transfer; caller cannot use the binding after the call
fn consume(sink label: String) -> String {
    label
}

// Self conventions work the same way
impl Counter {
    fn view(self) -> I32 { self.value }         // default (immutable self)
    fn increment(mut self) -> I32 { self.value } // mut self
}
```

### Traits

Traits declare field and method requirements. Any struct that satisfies all of them can declare conformance.

```rust
pub trait Named {
    name: String
}

pub trait Shape {
    color: String
    fn area(self) -> I32
}

// Declare conformance
pub struct Circle {
    name: String,
    color: String,
    radius: I32
}

impl Named for Circle {}            // fields checked against struct definition

impl Shape for Circle {
    fn area(self) -> I32 {
        self.radius * self.radius   // simplified
    }
}

// Trait composition
pub trait NamedShape: Named + Shape {
    label: String
}

// A trait is a constraint, never the type of a value: there is no
// dynamic dispatch. Take it as a generic bound for one concrete type.
fn area_of<T: Shape>(shape: T) -> I32 {
    shape.area()
}

// For a value whose type is chosen at run time, declare an enum with
// one variant per type and match on it.
pub enum AnyShape {
    square(value: Square),
    rectangle(value: Rectangle)
}

fn area(kind: I32, side: I32, w: I32, h: I32) -> I32 {
    let s: AnyShape = if kind == 0 {
        .square(value: Square(name: "sq", color: "black", side: side))
    } else {
        .rectangle(value: Rectangle(name: "rect", color: "white", width: w, height: h))
    }
    match s {
        .square(value): value.area(),
        .rectangle(value): value.area()
    }
}
```

### Enums

```rust
pub enum Status {
    pending
    active
    done
}

pub enum Message {
    text(content: String)
    image(url: String, size: I32)
    quit
}

// Instantiate with leading dot
let s: Status = .active
let m: Message = .text(content: "hello")
```

### Let bindings

```rust
let x = 42
let name: String = "Alice"
pub let MAX: I32 = 100
let mut counter: I32 = 0    // mutable binding
```

### Arrays, Dictionaries, Tuples

```rust
// Arrays
let tags: [String] = ["a", "b", "c"]
let matrix: [[I32]] = [[1, 2], [3, 4]]

// Dictionaries
let config: [String: I32] = ["timeout": 30, "retries": 3]
let empty: [String: Boolean] = [:]

// Tuples (all fields must be named)
let point = (x: 10, y: 20)
let name = point.x

// Indexing returns an Optional. The bound may be out of range or the
// key absent, so `xs[i]` and `d[k]` yield `T?` / `V?`. Use `if let` to
// consume the inner value.
let timeout: I32? = config["timeout"]
let first: String? = tags[0]
```

### Control Flow

```rust
// if: branches on a Boolean.
if user.score > 0 {
    greet(name: user.name)
} else {
    welcome()
}

// if let: Rust-style optional unwrap. Both branches required.
if let nickname = user.nickname {
    greet(name: nickname)        // nickname is bound to the unwrapped value
} else {
    greet(name: user.name)
}

// for: iterates arrays, returns array of results
for item in items {
    process(item: item)
}

// match: exhaustive, on enums (and on Optional, treated as .some / .none)
match message {
    .text(content): display(value: content),
    .image(url, size): showImage(src: url),
    .quit: stop()
}
```

### Closures

Closure types describe a callable shape; closure expressions construct one. Both wrap their parameter list in parentheses so every `->` in the language is preceded by `)`.

```rust
pub enum Event {
    pressed,
    textChanged(value: String),
    resized(width: I32, height: I32)
}

pub struct Button<E> {
    onPress:  () -> E,                  // no parameters
    onChange: (String) -> E,            // single parameter
    onResize: (I32, I32) -> E,          // multiple parameters
    onSubmit: ((String) -> E)?          // optional closure
}
```

Closure expressions wrap their parameter list in parentheses — even
for a single parameter — so every `->` in the language is preceded by
`)`:

```rust
// Untyped — parameter types come from the binding annotation or call context
let onPress  = () -> .pressed
let onChange = (x) -> .textChanged(value: x)
let onResize = (w, h) -> .resized(width: w, height: h)

// Typed parameters — annotate inline with `name: Type`
let increment = (n: I32) -> n + 1
let combine   = (x: I32, y: I32) -> x + y
```

Closures capture values from their surrounding scope. The `ClosureConversionPass` lifts each closure into a top-level function plus a synthetic env struct, so backends only ever consume named functions.

```rust
fn make_adder(sink n: I32) -> (I32) -> I32 {
    (x: I32) -> x + n          // captures n
}

let add5 = make_adder(n: 5)
```

Closure parameters carry the same conventions as regular function parameters (`mut`, `sink`). The convention constrains the **caller of the closure**:

```rust
pub struct Form<E> {
    onScale:   (mut I32) -> E,    // caller must pass a mutable binding
    onConsume: (sink String) -> E // caller's binding is moved
}
```

Closures are pure and single-expression: no statements, no side effects in the language itself. Effects live in the host runtime, reached through `extern` declarations.

### Generics

```rust
pub struct Box<T> {
    value: T
}

pub struct Pair<A, B> {
    first: A,
    second: B
}

pub trait Layout { width: I32 }

pub struct Container<T: Layout> {   // constrained type parameter
    items: [T],
    gap: I32
}

pub enum Result<T, E> {
    ok(value: T)
    error(err: E)
}

let b = Box<String>(value: "hello")
let r: Result<String, I32> = .ok(value: "success")

// Type-argument inference: when every generic parameter shows up in a
// field position, the type args can be omitted at the call site.
let inferred = Box(value: 42)        // Box<I32>
let pair = Pair(first: 10, second: true)  // Pair<I32, Boolean>
```

### Destructuring

```rust
// Arrays
let [first, second, ...rest] = items
let [_, second, ...] = items    // skip with _

// Structs (by field name)
let {name, age} = user
let {name as username} = user   // rename

// Enums (extract associated data)
let (content) = some_text_message
```

### Modules

```rust
// Inline module
pub mod geometry {
    pub struct Point { x: I32, y: I32 }
    pub enum Direction { north, south, east, west }
}

let p: geometry::Point = geometry::Point(x: 0, y: 0)

// Import from other .fv files
use geometry::Point
use ui::{Button, Text}
use data::models::User
```

Files map to module paths: `use geometry::shapes::Circle` resolves to `geometry/shapes.fv`.
Only `pub` items can be imported. Circular imports are a compile error.

### Extern declarations

Describe functions and method surfaces provided by the host runtime; they have no FormaLang body. There is no `extern type`; host-provided types are declared as regular structs and given an `extern impl` so their methods are resolved by the host.

```rust
pub struct Canvas {}
pub struct Connection {}

extern fn create_canvas() -> Canvas
extern fn connect(url: String) -> Connection
extern fn log(message: String)

extern impl Canvas {
    fn width(self) -> I32
    fn height(self) -> I32
    fn clear(self)
}
```

### Function overloading

```rust
fn format(value: I32) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: I32, precision: I32) -> String { "precise" }
```

The compiler resolves overloads by the named-argument label set. Ambiguous or unresolvable calls are compile errors.

---

## Rust API

### Entry points

| Function | Returns | Use case |
| --- | --- | --- |
| `compile_to_ir(src)` | `Result<IrModule, Vec<CompilerError>>` | Code generation (canonical) |
| `compile_with_analyzer(src)` | `Result<(File, SemanticAnalyzer), …>` | LSP hover / completion |
| `compile_and_report(src, filename)` | `Result<IrModule, String>` | CLI: compile + human-readable errors |
| `parse_only(src)` | `Result<File, …>` | Syntax check only |

Custom module resolver (to load `.fv` files from anywhere):

```rust
use formalang::{compile_to_ir_with_resolver, FileSystemResolver};
use std::path::PathBuf;

let resolver = FileSystemResolver::new(PathBuf::from("./src"));
let module = compile_to_ir_with_resolver(source, resolver)?;
```

### The IrModule

```rust
let module = compile_to_ir(source)?;

module.structs    // Vec<IrStruct>
module.traits     // Vec<IrTrait>
module.enums      // Vec<IrEnum>
module.functions  // Vec<IrFunction>   (extern fns: extern_abi = Some(_), body = None)
module.impls      // Vec<IrImpl>
module.lets       // Vec<IrLet>
module.imports    // Vec<IrImport>
module.modules    // Vec<IrModuleNode>  (preserves source `mod foo { ... }` hierarchy)

// ID-based lookup
let id = module.struct_id("User").unwrap();
let s  = module.get_struct(id).unwrap();
```

All types in the IR are fully resolved; no unresolved references remain.

### Pipeline (passes + backends)

```rust
use formalang::{compile_to_ir, Pipeline};
use formalang::ir::{DeadCodeEliminationPass, ConstantFoldingPass};

let module = compile_to_ir(source)?;

let output = Pipeline::new()
    .pass(DeadCodeEliminationPass::default())
    .pass(ConstantFoldingPass::default())
    .emit(module, &my_backend)?;
```

Implement `IrPass` to write your own transforms, and `Backend` to emit code:

```rust
use formalang::{Backend, CompilerError, IrPass};
use formalang::ir::IrModule;

struct MyPass;

impl IrPass for MyPass {
    fn name(&self) -> &str { "my_pass" }
    fn run(&mut self, module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        // transform and return
        Ok(module)
    }
}

struct MyBackend;

impl Backend for MyBackend {
    type Output = String;
    type Error = std::convert::Infallible;

    fn generate(&self, module: &IrModule) -> Result<String, Self::Error> {
        Ok(format!("// {} structs", module.structs.len()))
    }
}
```

### Error reporting

```rust
use formalang::{compile_to_ir, reporting::report_errors};

match compile_to_ir(source) {
    Ok(_) => {}
    Err(errors) => {
        eprintln!("{}", report_errors(&errors, source, "file.fv"));
    }
}
```

---

## File extension

FormaLang source files use the `.fv` extension.

---

## What is not built in

FormaLang is a pure compiler frontend. It does **not** include:

- A runtime or interpreter
- Code generation for any specific target
- A standard library (bring your own via `extern` declarations)
- A package manager

These are responsibilities of the embedding application and its backends.

---

## Further reading

- [Language Reference](docs/user/core.md): user-facing syntax and feature reference
- [Architecture](docs/developer/architecture/design.md): compiler internals
- [IR Reference](docs/developer/ir/overview.md): IrModule structure for backend authors
- [AST Reference](docs/developer/ast/overview.md): AST structure for tooling authors

---

## License

Dual-licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
