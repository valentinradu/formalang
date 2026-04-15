# FormaLang

FormaLang is a statically typed, declarative language designed to be embedded in Rust applications. You write `.fv` files, the library parses and validates them, and you get a fully type-resolved IR back. What you do with that IR — generate code, drive a UI framework, configure a system — is up to your backend.

```text
.fv source → formalang library → IrModule → your Backend → output
```

---

## Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
formalang = "0.1"
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

```formalang
let text: String = "hello"
let count: Number = 42
let flag: Boolean = true
let logo: Path = /assets/logo.svg
let pattern: Regex = r/[a-z]+/i
let nothing: String? = nil       // optional — any type can be made optional with ?
```

### Structs

```formalang
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

```formalang
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

### Traits

Traits declare field and method requirements. Any struct that satisfies all of them can declare conformance.

```formalang
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

```formalang
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

```formalang
let x = 42
let name: String = "Alice"
pub let MAX: Number = 100
mut let counter = 0
```

### Arrays, Dictionaries, Tuples

```formalang
// Arrays
let tags: [String] = ["a", "b", "c"]
let matrix: [[Number]] = [[1, 2], [3, 4]]
let fixed: [Number, 3] = [255, 128, 0]   // fixed-size

// Dictionaries
let config: [String: Number] = ["timeout": 30, "retries": 3]
let empty: [String: Boolean] = [:]

// Tuples (all fields must be named)
let point = (x: 10, y: 20)
let name = point.x
```

### Control Flow

```formalang
// if — also unwraps optionals automatically
if user.nickname {
    greet(name: nickname)
} else {
    greet(name: user.name)
}

// for — iterates arrays, returns array of results
for item in items {
    process(item: item)
}

// match — exhaustive, on enums
match message {
    .text(content): display(value: content)
    .image(url, size): showImage(src: url)
    .quit: stop()
}
```

### Closures

```formalang
pub struct Button<E> {
    onPress: () -> E,
    onChange: String -> E,
    onResize: Number, Number -> E
}

// Closure expressions
let handler = () -> .submit
let mapper = x -> .textChanged(value: x)
let resizer = w, h -> .resized(width: w, height: h)
```

### Generics

```formalang
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

```formalang
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

```formalang
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

Describe types and functions provided by the host runtime — they have no FormaLang body.

```formalang
extern type Canvas
extern type Connection

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

```formalang
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
| `compile_to_ir(src)` | `Result<IrModule, Vec<CompilerError>>` | Code generation |
| `compile(src)` | `Result<File, Vec<CompilerError>>` | Validation, tooling |
| `compile_with_analyzer(src)` | `Result<(File, SemanticAnalyzer), …>` | LSP hover / completion |
| `compile_and_report(src, filename)` | `Result<File, String>` | Human-readable errors |
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

module.structs          // Vec<IrStruct>
module.traits           // Vec<IrTrait>
module.enums            // Vec<IrEnum>
module.functions        // Vec<IrFunction>
module.impls            // Vec<IrImpl>
module.lets             // Vec<IrLet>
module.extern_types     // Vec<IrExternType>

// ID-based lookup
let id = module.struct_id("User").unwrap();
let s  = module.get_struct(id).unwrap();
```

All types in the IR are fully resolved — no unresolved references remain.

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
use formalang::{IrPass, IrModule, Backend};

struct MyPass;

impl IrPass for MyPass {
    fn name(&self) -> &str { "my_pass" }
    fn run(&self, module: IrModule) -> Result<IrModule, Vec<CompilerError>> {
        // transform and return
        Ok(module)
    }
}

struct MyBackend;

impl Backend for MyBackend {
    type Output = String;
    fn generate(&self, module: &IrModule) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!("// {} structs", module.structs.len()))
    }
}
```

### Error reporting

```rust
use formalang::{compile, reporting::report_errors};

match compile(source) {
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

- [Language Reference](docs/user/formalang.md) — full syntax reference with all rules
- [Architecture](docs/developer/architecture.md) — compiler internals
- [IR Reference](docs/developer/ir.md) — IrModule structure for backend authors
- [AST Reference](docs/developer/ast.md) — AST structure for tooling authors
