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
formalang = "0.0.9-beta"
```

To serialize the IR, turn on the `serde` feature:
`formalang = { version = "0.0.9-beta", features = ["serde"] }`. Then
`IrModule` and every type in it derive `serde::Serialize` and
`serde::Deserialize`. The JSON form of the IR is not a stable format.
The AST has no serialized form.

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
let user = module.user_structs().next().unwrap();
println!("{}", user.name); // User
```

---

## Language Tour

Each block below is a complete program. The test suite compiles each
one, and runs its `run_checks()` when it has one.

### Primitives

```formalang
pub let text: String = "hello"
pub let count: I32 = 42
pub let big: I64 = 9_223_372_036_854_775_807
pub let ratio: F64 = 3.14
pub let small: F32 = 0.5              // the annotation gives F32
pub let tagged = 0.5F32               // a suffix gives the type too
pub let flag: Boolean = true
pub let nothing: String? = nil        // optional; any type can be made optional with ?
```

Numeric primitives are width-tagged: `I32`, `I64`, `F32`, `F64`. An
unsuffixed literal takes its type from its position: an annotation, a
parameter, a field or a return type. Where no position gives a type,
an integer literal is an `I32` and a float literal is an `F64`. A
suffix is uppercase and adjacent to the digits (`42I64`, `3.14F32`).
An operator does not give its type to the other operand: with
`n: I64`, write `n + 1I64`.

### Structs

A comma separates two fields.

```formalang
pub struct Point {
    x: I32,
    y: I32
}

pub struct User {
    name: String,
    email: String,
    nickname: String?,       // optional field
    score: I32 = 0           // field with a default
}

// Instantiate with named arguments
pub let p = Point(x: 10, y: 20)
pub let u = User(name: "Alice", email: "alice@example.com", nickname: nil)

// Mutability is a property of the binding, not the field.
pub fn run_checks() {
    let mut moved = p
    moved.x = 11
    assert(condition: moved.x == 11 && u.score == 0)
}
```

### Methods (impl blocks)

```formalang
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

`self` exists only in a method that declares it, as its first
parameter. A method without `self` is static.

### Parameter Conventions

Every function parameter has a convention controlling how the argument is received. The call site always looks the same as `f(x: v)`; only the function declaration changes.

```formalang
// default: immutable; the callee reads the value
fn area(radius: I32) -> I32 {
    radius * radius
}

// mut: callee may change it, and the caller sees the change;
// the argument binding must be let mut
fn bump(mut n: I32) {
    n = n + 1
}

// sink: ownership transfer; caller cannot use the binding after the call
fn consume(sink label: String) -> String {
    label
}

pub struct Counter { value: I32 }

// Self conventions work the same way
impl Counter {
    fn view(self) -> I32 { self.value }            // default (immutable self)
    fn increment(mut self) { self.value = self.value + 1 }  // mut self
}

pub fn run_checks() {
    let mut n: I32 = 1
    bump(n: n)
    let mut c = Counter(value: 0)
    c.increment()
    assert(condition: n == 2 && c.view() == 1)
}
```

### Traits

Traits declare field and method requirements. Any struct that satisfies all of them can declare conformance.

```formalang
pub trait Named {
    name: String
}

pub trait Shape {
    color: String
    fn area(self) -> I32
}

pub struct Square {
    name: String,
    color: String,
    side: I32
}

pub struct Rectangle {
    name: String,
    color: String,
    width: I32,
    height: I32
}

impl Named for Square {}            // fields checked against struct definition

impl Shape for Square {
    fn area(self) -> I32 { self.side * self.side }
}

impl Shape for Rectangle {
    fn area(self) -> I32 { self.width * self.height }
}

// Trait composition: each trait in the chain needs its own impl block
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

pub fn run_checks() {
    assert(condition: area(kind: 0, side: 3, w: 0, h: 0) == 9)
    assert(condition: area_of(shape: Rectangle(name: "r", color: "c", width: 2, height: 5)) == 10)
}
```

### Enums

A comma separates two variants.

```formalang
pub enum Status {
    pending,
    active,
    done
}

pub enum Message {
    text(content: String),
    image(url: String, size: I32),
    quit
}

// Instantiate with a leading dot; the annotation gives the enum
pub let s: Status = .active
pub let m: Message = .text(content: "hello")

// Or name the enum
pub let q = Message.quit
```

### Let bindings

```formalang
pub let x = 42
pub let name: String = "Alice"
pub let MAX: I32 = 100

pub fn run_checks() {
    let mut counter: I32 = 0    // mutable binding
    counter = counter + 1
    assert(condition: counter == 1)
}
```

In a body, a line break ends a statement. Two statements on one line
are an error.

### Arrays, Dictionaries, Tuples

```formalang
pub fn run_checks() {
    // Arrays
    let tags: [String] = ["a", "b", "c"]
    let matrix: [[I32]] = [[1, 2], [3, 4]]

    // Dictionaries
    let config: [String: I32] = ["timeout": 30, "retries": 3]
    let empty: [String: Boolean] = [:]

    // Tuples (all fields must be named)
    let point = (x: 10, y: 20)
    let x = point.x

    // Indexing returns an Optional. The bound may be out of range or the
    // key absent, so `xs[i]` and `d[k]` yield `T?` / `V?`. Use `if let` to
    // consume the inner value.
    let timeout: I32? = config["timeout"]
    let first: String? = tags[0]

    assert(condition: matrix.len() == 2 && empty.is_empty() && x == 10)
    assert(condition: if let t = timeout { t == 30 } else { false })
    assert(condition: first != nil)
}
```

### Control Flow

```formalang
pub struct Item { score: I32 }
pub struct User { name: String, nickname: String?, score: I32 }

pub enum Message {
    text(content: String),
    image(url: String, size: I32),
    quit
}

fn greet(name: String) -> String { "Hi, " + name }

pub fn tour(user: User, items: [Item], message: Message) -> I32 {
    // if: branches on a Boolean.
    let score = if user.score > 0 {
        user.score
    } else {
        0
    }

    // if let: Rust-style optional unwrap. Both branches required.
    let hello = if let nickname = user.nickname {
        greet(name: nickname)        // nickname is bound to the unwrapped value
    } else {
        greet(name: user.name)
    }

    // for: yields a lazy sequence. Nothing runs until a terminal
    // combinator consumes it, so a pipeline is one pass with no
    // intermediate array.
    let total: I32 = for item in items { item.score }
        .filter(f: (s) -> s > 0)
        .fold(initial: 0, f: (a, b) -> a + b)

    // `.collect()` for an array, `.run()` for effects alone. A sequence is
    // consumed exactly once: dropping one, or reading it twice, is an error.
    let scores: [I32] = for item in items { item.score }.collect()

    // match: exhaustive, on enums (and on Optional, as .some / .none).
    // The names bind the associated values by position.
    let weight = match message {
        .text(content): content.len(),
        .image(url, size): size,
        .quit: 0
    }

    score + hello.len() + total + scores.len() + weight
}
```

### Closures

Closure types describe a callable shape; closure expressions construct one. Both wrap their parameter list in parentheses so every `->` in the language is preceded by `)`.

```formalang
pub enum Event {
    pressed,
    textChanged(value: String),
    resized(width: I32, height: I32)
}

struct Button<E> {
    onPress:  () -> E,                  // no parameters
    onChange: (String) -> E,            // single parameter
    onResize: (I32, I32) -> E,          // multiple parameters
    onSubmit: ((String) -> E)?          // optional closure
}

fn make_button() -> Button<Event> {
    // The field types give the closures their parameter types
    Button<Event>(
        onPress: () -> .pressed,
        onChange: (x) -> .textChanged(value: x),
        onResize: (w, h) -> .resized(width: w, height: h),
        onSubmit: nil
    )
}

pub fn run_checks() {
    // With no declared type around it, a parameter needs its type
    let increment = (n: I32) -> n + 1
    let combine = (x: I32, y: I32) -> x + y
    assert(condition: combine(increment(1), 3) == 5)   // a closure call takes no labels
}
```

A closure field cannot be part of a `pub struct` or a `pub enum`:
closures stay inside their module.

A closure captures by value, so a returned closure may capture any
binding: a parameter, a local `let` or a module `let`. The
`ClosureConversionPass` lifts each closure into a top-level
function plus a synthetic env struct, so backends only ever consume
named functions.

```formalang
fn make_adder(n: I32) -> (I32) -> I32 {
    (x: I32) -> x + n          // captures a copy of n
}

pub fn run_checks() {
    let add5 = make_adder(n: 5)
    assert(condition: add5(1) == 6)
}
```

Closure parameters carry the same conventions as regular function
parameters (`mut`, `sink`). The convention constrains the **caller of
the closure**:

```formalang
struct Form<E> {
    onScale:   (mut I32) -> E,    // caller must pass a mutable binding
    onConsume: (sink String) -> E // caller's binding is moved
}
```

A closure body is one expression; a block `{ ... }` counts as one.
Effects live in the host runtime, reached through `extern`
declarations.

### Generics

```formalang
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
    ok(value: T),
    error(err: E)
}

pub let b = Box<String>(value: "hello")
pub let r: Result<String, I32> = .ok(value: "success")

// Type-argument inference: when every generic parameter shows up in a
// field position, the type args can be omitted at the call site.
pub let inferred = Box(value: 42)              // Box<I32>
pub let pair = Pair(first: 10, second: true)   // Pair<I32, Boolean>
```

### Destructuring

```formalang
pub struct User { name: String, age: I32 }

pub enum Message { text(content: String), quit }

pub fn run_checks() {
    let items = ["a", "b", "c", "d"]
    let user = User(name: "Ada", age: 36)

    // Arrays
    let [first, second, ...rest] = items
    let [_, again, ...] = items    // skip with _

    // Structs (by field name)
    let {name, age} = user
    let {name as username} = user  // rename

    // An enum value is not destructured: use match (or if let)
    let m: Message = .text(content: "hi")
    let content = match m {
        .text(c): c,
        .quit: ""
    }

    assert(condition: second == again && rest.len() == 2)
    assert(condition: name == username && age == 36 && content == "hi")
    assert(condition: first == "a")
}
```

### Modules

```formalang
// Inline module
pub mod geometry {
    pub struct Point { x: I32, y: I32 }
    pub enum Direction { north, south, east, west }
}

pub let p: geometry::Point = geometry::Point(x: 0, y: 0)

// Import an item of an inline module
use geometry::Direction

pub let d: Direction = .north
```

Files map to module paths: `use geometry::shapes::Circle` resolves to
`geometry/shapes.fv`, and `use ui::{Button, Text}` resolves to `ui.fv`.
Only `pub` items can be imported, and `pub use` exports an imported
item again. An imported type brings its impl blocks, trait impls
included. Circular imports are a compile error.

### Extern declarations

Describe functions and method surfaces provided by the host runtime; they have no FormaLang body. There is no `extern type`; host-provided types are declared as regular structs and given an `extern impl` so their methods are resolved by the host.

```formalang
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

```formalang
fn format(value: I32) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: I32, precision: I32) -> String { "precise" }

pub fn run_checks() {
    assert(condition: format(value: 1) == "number")
    assert(condition: format(value: "a") == "string")
    assert(condition: format(value: 1, precision: 2) == "precise")
}
```

The compiler picks an overload by the labels of the call, the number of
arguments, and the argument types. Ambiguous or unresolvable calls are
compile errors.

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
