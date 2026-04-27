# FormaLang

FormaLang is a statically typed, declarative language designed to be embedded in Rust applications. You write `.fv` files, the library parses and validates them, and you get a fully type-resolved IR back. What you do with that IR (generate code, drive a UI framework, configure a system) is up to your backend.

```text
.fv source → formalang library → IrModule → your Backend → output
```

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
formalang = "0.0.1-beta"
```

Compile a source string:

```rust
use formalang::compile_to_ir;

let source = r#"
    pub struct User {
        name: String,
        age: Number
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
let count: Number = 42
let flag: Boolean = true
let logo: Path = /assets/logo.svg
let pattern: Regex = r/[a-z]+/i
let nothing: String? = nil       // optional; any type can be made optional with ?
```

### Structs

```rust
pub struct Point {
    x: Number,
    y: Number
}

pub struct User {
    name: String,
    email: String,
    nickname: String?,       // optional field
    mut score: Number        // mutable field
}

// Instantiate with named arguments
let p = Point(x: 10, y: 20)
let u = User(name: "Alice", email: "alice@example.com", nickname: nil, score: 0)
```

### Methods (impl blocks)

```rust
pub struct Counter {
    value: Number
}

impl Counter {
    fn increment(self) -> Number {
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
fn area(radius: Number) -> Number {
    radius * radius
}

// mut: callee may mutate; argument binding must be let mut
fn bump(mut n: Number) -> Number {
    n
}

// sink: ownership transfer; caller cannot use the binding after the call
fn consume(sink label: String) -> String {
    label
}

// Self conventions work the same way
impl Counter {
    fn view(self) -> Number { self.value }         // default (immutable self)
    fn increment(mut self) -> Number { self.value } // mut self
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
    fn area(self) -> Number
}

// Declare conformance
pub struct Circle {
    name: String,
    color: String,
    radius: Number
}

impl Named for Circle {}            // fields checked against struct definition

impl Shape for Circle {
    fn area(self) -> Number {
        self.radius * self.radius   // simplified
    }
}

// Trait composition
pub trait NamedShape: Named + Shape {
    label: String
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
    image(url: String, size: Number)
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
pub let MAX: Number = 100
let mut counter: Number = 0    // mutable binding
```

### Arrays, Dictionaries, Tuples

```rust
// Arrays
let tags: [String] = ["a", "b", "c"]
let matrix: [[Number]] = [[1, 2], [3, 4]]

// Dictionaries
let config: [String: Number] = ["timeout": 30, "retries": 3]
let empty: [String: Boolean] = [:]

// Tuples (all fields must be named)
let point = (x: 10, y: 20)
let name = point.x
```

### Control Flow

```rust
// if: also unwraps optionals automatically
if user.nickname {
    greet(name: nickname)
} else {
    greet(name: user.name)
}

// for: iterates arrays, returns array of results
for item in items {
    process(item: item)
}

// match: exhaustive, on enums
match message {
    .text(content): display(value: content),
    .image(url, size): showImage(src: url),
    .quit: stop()
}
```

### Closures

Closure types describe a callable shape; closure expressions construct one. Both use the same arrow syntax, with an alternative pipe form for expressions.

```rust
pub enum Event {
    pressed,
    textChanged(value: String),
    resized(width: Number, height: Number)
}

pub struct Button<E> {
    onPress:  () -> E,                  // no parameters
    onChange: String -> E,              // single parameter
    onResize: Number, Number -> E,      // multiple parameters
    onSubmit: (String -> E)?            // optional closure
}
```

Both arrow and pipe forms are accepted at expression sites:

```rust
// Arrow form (Swift-style); parameter types inferred
let onPress  = () -> .pressed
let onChange = x -> .textChanged(value: x)
let onResize = w, h -> .resized(width: w, height: h)

// Pipe form (Rust-style); accepts explicit parameter types
let increment = |n: Number| n + 1
let combine   = |x: Number, y: Number| x + y
```

Closures capture values from their surrounding scope. The `ClosureConversionPass` lifts each closure into a top-level function plus a synthetic env struct, so backends only ever consume named functions.

```rust
fn make_adder(sink n: Number) -> Number -> Number {
    |x: Number| x + n           // captures n
}

let add5 = make_adder(n: 5)
```

Closure parameters carry the same conventions as regular function parameters (`mut`, `sink`). The convention constrains the **caller of the closure**:

```rust
pub struct Form<E> {
    onScale:   mut Number -> E,     // caller must pass a mutable binding
    onConsume: sink String -> E     // caller's binding is moved
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

pub trait Layout { width: Number }

pub struct Container<T: Layout> {   // constrained type parameter
    items: [T],
    gap: Number
}

pub enum Result<T, E> {
    ok(value: T)
    error(err: E)
}

let b = Box<String>(value: "hello")
let r: Result<String, Number> = .ok(value: "success")
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
    pub struct Point { x: Number, y: Number }
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
    fn width(self) -> Number
    fn height(self) -> Number
    fn clear(self)
}
```

### Function overloading

```rust
fn format(value: Number) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: Number, precision: Number) -> String { "precise" }
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

- [Language Reference](docs/user/formalang.md): full syntax reference with all rules
- [Architecture](docs/developer/architecture.md): compiler internals
- [IR Reference](docs/developer/ir.md): IrModule structure for backend authors
- [AST Reference](docs/developer/ast.md): AST structure for tooling authors
