# FormaLang Language Features Reference

Reference of all supported language features with practical examples.

---

## Table of Contents

- [Core Constructs](#core-constructs)
- [Type System](#type-system)
  - [Closure Types](#closure-types)
- [Definitions](#definitions)
  - [Struct Definitions](#struct-definitions)
  - [Impl Blocks](#impl-blocks)
  - [Trait Definitions](#trait-definitions)
  - [Impl Trait for Type](#impl-trait-for-type)
  - [Enum Definitions](#enum-definitions)
  - [Extern Declarations](#extern-declarations)
  - [Function Definitions](#function-definitions)
- [Expressions](#expressions)
  - [Closure Expressions](#closure-expressions)
- [Control Flow](#control-flow)
- [Generics](#generics)
- [Module System](#module-system)

---

## Core Constructs

### 1. Comments

**Single-line comments**:

```formalang
// This is a single-line comment
pub struct User { name: String }  // Inline comment
```

**Multi-line comments**:

```formalang
/*
 * This is a multi-line comment
 * spanning several lines
 */
pub struct Post { title: String }
```

### 2. Visibility Modifiers

Control export visibility with the `pub` keyword:

```formalang
// Public - can be imported by other modules
pub struct User { name: String }
pub trait Named { name: String }
pub enum Status { active, inactive }
pub let MAX_USERS: Number = 100

// Private - module-local only (default)
struct Internal { id: Number }
trait Helper { key: String }
enum State { ready, processing }
let secret_key: String = "xyz"
```

### 3. Keywords

Reserved words that cannot be used as identifiers:

```text
trait    struct   enum     use      pub      impl     mod
let      mut      match    for      in       if
else     true     false    nil      as       extern
provides consumes
```

---

## Type System

### Primitive Types

```formalang
pub struct Primitives {
  text: String,           // Text data
  count: Number,          // Numeric values (int or float)
  active: Boolean,        // true or false
  logo: Path,             // File/resource paths
  pattern: Regex          // Regular expressions
}
```

### Never Type

`Never` is an uninhabited type — it has no values and cannot be instantiated.
It is used as a return type for functions that diverge (infinite loops, panics):

```formalang
extern fn abort() -> Never
```

### Array Types

Arrays hold multiple values of the same type:

```formalang
pub struct Collections {
  names: [String],             // Variable-length array of strings
  scores: [Number],            // Variable-length array of numbers
  flags: [Boolean],            // Variable-length array of booleans
  matrix: [[Number]],          // Nested arrays
  users: [User],               // Array of custom types
  coords: [Number, 3]          // Fixed-size array (3 numbers)
}

// Array literals
pub let tags = ["urgent", "bug", "frontend"]
pub let numbers = [1, 2, 3, 4, 5]
pub let empty = []

// Fixed-size arrays
pub let rgb: [Number, 3] = [255, 128, 0]

// Array destructuring
pub let [first, second] = ["a", "b", "c"]     // Positional destructuring
pub let [user, ...] = ["John", "pass", "etc"] // Rest pattern
```

### Optional Types

Optional types can be a value or `nil`:

```formalang
pub struct User {
  name: String,
  email: String,
  nickname: String?,            // Optional field
  avatar: String?               // Optional field
}

pub let user1 = User(
  name: "Alice",
  email: "alice@example.com",
  nickname: "Ally",             // Provide a value
  avatar: nil                   // Explicitly nil
)
```

### Dictionary Types

Key-value mappings using bracket syntax with colon:

```formalang
pub struct AppConfig {
  settings: [String: Number],        // String keys to Number values
  scores: [Number: String],          // Number keys to String values
  cache: [String: User],             // String keys to custom types
  assets: [Path: String]             // Path keys to String values
}

// Dictionary literals (string keys must be quoted)
pub let settings: [String: Number] = ["timeout": 30, "maxRetries": 3]
pub let scores: [Number: String] = [100: "perfect", 95: "excellent"]
pub let assets: [Path: String] = [/logo.svg: "icon", /bg.png: "background"]
pub let empty: [String: Boolean] = [:]
```

**Rules**:

- Keys can be any compiler-supported type (String, Number, Path, enum, etc.)
- String keys must be quoted in literals: `["key": value]`
- Number keys are unquoted: `[42: value]`
- Path keys use path literal syntax: `[/path: value]`
- Empty dict: `[:]`
- No destructuring support for dictionaries

### Tuples

Named tuples group related values with field names:

```formalang
pub struct Config {
  person: (name: String, age: Number),
  point: (x: Number, y: Number),
  nested: (user: (first: String, last: String), active: Boolean)
}

// Tuple literals
for item in items {
  let person = (name: "John", age: 30)
  let point = (x: 10, y: 20)
  let nested = (user: (first: "John", last: "Doe"), active: true)
}

// Accessing tuple fields
for item in items {
  let person = (name: "John", age: 30)
  let name = person.name      // Access by field name
}
```

**Rules**:

- Tuples use parentheses: `(name: value, ...)`
- All fields must be named (no positional tuples)
- Access fields with dot notation: `tuple.fieldName`
- Trailing comma allowed: `(x: 1, y: 2,)`
- Tuples can be nested

### Closure Types

Closure types define function signatures for callbacks and transformations:

```formalang
pub struct Controls<E> {
  // No parameters - returns E
  onPress: () -> E,

  // Single parameter
  onChange: String -> E,

  // Multiple parameters (comma-separated, no parens needed)
  onResize: Number, Number -> E,

  // Optional closure (can be nil)
  onFocus: (String -> E)?,

  // Closure returning optional
  validate: String -> Boolean?
}
```

**Type syntax**:

| Parameters | Syntax       | Example                   |
| ---------- | ------------ | ------------------------- |
| None       | `() -> T`    | `() -> Event`             |
| One        | `T -> U`     | `String -> Event`         |
| Multiple   | `T, U -> V`  | `Number, Number -> Point` |

**Rules**:

- Arrow `->` separates parameters from return type
- Multiple parameters are comma-separated (no parentheses required)
- Empty parameters require parentheses: `() -> T`
- Parser uses `->` to determine grouping in ambiguous contexts

### Generic Types

Types parameterized with type variables:

```formalang
Box<T>                      // Single type parameter
Pair<A, B>                  // Multiple type parameters
Container<T: Layout>        // With trait constraint
Widget<T: Render + Click>   // Multiple trait constraints
Result<String, Number>      // Instantiated generic
```

See [Generics](#generics) section for details.

---

## Definitions

### Struct Definitions

Structs define data types:

```formalang
// Basic struct
pub struct Point {
  x: Number,
  y: Number
}

// With optional fields
pub struct User {
  name: String,
  email: String,
  age: Number,
  verified: Boolean,
  nickname: String?
}

// With mutable fields
pub struct Counter {
  mut count: Number,    // Mutable field (can be updated)
  label: String         // Immutable field
}

// Generic struct
pub struct Box<T> {
  value: T
}

pub struct Pair<A, B> {
  first: A,
  second: B
}

// Generic with constraints
pub struct Container<T: Layout> {
  items: [T],
  gap: Number
}
```

### Impl Blocks

Impl blocks add methods to a struct (inherent impl) or declare trait conformance
(impl Trait for Struct).

**Inherent impl** — methods belong to the struct:

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

### Trait Definitions

Traits declare field requirements and method signatures that conforming types must satisfy:

```formalang
// Fields only
pub trait Named {
  name: String
}

// Fields and methods
pub trait Shape {
  color: String
  fn area(self) -> Number
  fn perimeter(self) -> Number
}

// Methods only
pub trait Drawable {
  fn draw(self) -> Boolean
  fn visible(self) -> Boolean
}

// Trait composition (inheritance)
pub trait Entity: Named + Identifiable {
  createdAt: Number
}

// Generic trait
pub trait Collection<T> {
  items: [T]
}
```

**Trait Rules**:

- Fields listed without `fn` are structural requirements (the struct must have them)
- `fn` signatures listed without a body are method requirements
- Trait composition (`+`) combines requirements from multiple traits
- A type satisfies a trait by providing all required fields and all required methods

### Impl Trait for Type

Declare that a type conforms to a trait using `impl Trait for Type`:

```formalang
pub trait Named {
  name: String
}

pub trait Drawable {
  fn draw(self) -> Boolean
}

pub struct Circle {
  name: String,
  radius: Number
}

// Declare conformance (fields are checked against struct definition)
impl Named for Circle {}

// Provide required methods
impl Drawable for Circle {
  fn draw(self) -> Boolean {
    self.radius > 0
  }
}
```

Trait composition requires a separate impl block for each trait in the hierarchy:

```formalang
pub trait Base {
  fn id(self) -> Number
}

pub trait Extended: Base {
  fn name(self) -> String
}

pub struct Item {
  value: Number
}

impl Base for Item {
  fn id(self) -> Number {
    self.value
  }
}

impl Extended for Item {
  fn name(self) -> String {
    "item"
  }
}
```

**Conformance rules**:

- `impl Trait for Type` is the only way to declare trait conformance
- Struct fields required by the trait must be present in the struct definition
- All `fn` signatures in the trait must be implemented in the impl block
- Method signatures (parameter count and return type) must match exactly
- Separate impl blocks for inherited traits; only provide methods declared in that trait

#### Trait-Bounded Polymorphism

Traits can be used as types in field and parameter declarations:

```formalang
pub trait Printable {
  label: String
}

pub struct Doc {
  label: String
}

impl Printable for Doc {}

fn print_it(item: Printable) -> String {
  item.label
}
```

### Enum Definitions

Enums define sum types (tagged unions):

```formalang
// Simple enum
pub enum Status {
  pending
  active
  completed
}

// With associated data (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: Number)
  video(url: String, duration: Number)
}

// Generic enum
pub enum Result<T, E> {
  ok(value: T)
  error(err: E)
}

pub enum Option<T> {
  some(value: T)
  none
}

// Enum instantiation (leading dot notation)
pub let status1: Status = .pending
pub let msg1: Message = .text(content: "Hello")
pub let result1: Result<String, Number> = .ok(value: "success")
```

### Extern Declarations

Extern declarations describe types and functions defined outside FormaLang
(in the host runtime or a linked library). They have no FormaLang body.

**Extern type** — an opaque type provided by the host:

```formalang
extern type Canvas
extern type Connection
```

**Extern function** — a bodyless function provided by the host:

```formalang
extern fn create_canvas() -> Canvas
extern fn connect(url: String) -> Connection
extern fn log(message: String)
```

**Extern impl** — methods on an extern type provided by the host:

```formalang
extern type Canvas

extern impl Canvas {
  fn width(self) -> Number
  fn height(self) -> Number
  fn clear(self)
}
```

**Rules**:

- Extern types are opaque: no struct fields are visible inside FormaLang
- Extern functions and extern impl methods have no body
- You can read fields of extern types via dot access (host supplies the value)
- Extern types can implement FormaLang traits via `impl Trait for ExternType`

### Function Definitions

Top-level functions with a body:

```formalang
fn add(a: Number, b: Number) -> Number {
  a + b
}

pub fn greet(name: String) -> String {
  "Hello, " + name
}

// No return type (returns unit)
fn log_value(value: Number) {
  value
}

// Generic function
pub fn identity<T>(value: T) -> T {
  value
}
```

---

## Expressions

### Literals

All literal types as expressions:

```formalang
// String literals
let text = "Hello, World"
let multiline = """
  Multi-line
  string literal
"""

// Number literals
let integer = 42
let negative = -17
let float = 3.14
let with_underscore = 1_000_000

// Boolean literals
let yes = true
let no = false

// Nil literal
let nothing: String? = nil

// Array literals
let tags = ["urgent", "bug", "frontend"]
let numbers = [1, 2, 3, 4, 5]
let empty: [String] = []

// Dictionary literals
let settings: [String: Number] = ["timeout": 30, "maxRetries": 3]
let emptyDict: [String: Boolean] = [:]

// Path literals
let logo = /assets/logo.svg

// Regex literals
let pattern = r/[a-z]+/i
```

**Escape sequences** (strings): `\"`, `\\`, `\n`, `\t`, `\r`, `\uXXXX`

**Regex flags**: `g`, `i`, `m`, `s`, `u`, `v`, `y`

### Closure Expressions

Closures are pure, single-expression functions:

```formalang
pub enum Event {
  textChanged(value: String),
  resized(width: Number, height: Number),
  submit
}

pub struct Form<E> {
  onChange: String -> E,
  onResize: Number, Number -> E,
  onSubmit: () -> E
}

impl Form {
  // Single parameter - no parens needed
  onChange: x -> .textChanged(value: x),

  // Multiple parameters - comma separated
  onResize: w, h -> .resized(width: w, height: h),

  // No parameters - empty parens required
  onSubmit: () -> .submit
}
```

**Expression syntax**:

| Parameters | Syntax         | Example                        |
| ---------- | -------------- | ------------------------------ |
| None       | `() -> expr`   | `() -> .submit`                |
| One        | `x -> expr`    | `x -> .changed(value: x)`      |
| Multiple   | `x, y -> expr` | `x, y -> .point(x: x, y: y)`   |
| With types | `x: T -> expr` | `x: String -> .text(x: x)`     |

**Rules**:

- Closures are **pure** — no side effects, single expression body
- Single parameter does not need parentheses
- Multiple parameters are comma-separated
- Empty parameters require parentheses: `() -> expr`
- Type annotations optional when inferable

### Instantiation

#### Struct Instantiation

```formalang
pub struct Point { x: Number, y: Number }

// Basic instantiation
pub let point = Point(x: 10, y: 20)

// Multi-line instantiation
pub let user = User(
  name: "Alice",
  email: "alice@example.com",
  age: 30
)

// Generic instantiation with type arguments
pub let box_str = Box<String>(value: "hello")
pub let pair = Pair<Number, Boolean>(first: 42, second: true)

// Type inference (type arguments optional when inferrable)
pub let box_inferred = Box(value: "inferred as String")
```

#### Enum Instantiation

```formalang
// Simple variant (leading dot notation)
let status1: Status = .pending
let status2: Status = .active

// With named parameters
let msg1: Message = .text(content: "Hello")
let msg2: Message = .image(url: /pic.jpg, size: 1024)

// Generic enum
let result1: Result<String, Number> = .ok(value: "success")
let result2: Result<String, Number> = .error(err: 404)
```

### Field Access

```formalang
user.name                   // Access field
point.x                     // Access coordinate
config.timeout              // Access config field
user.profile.avatar         // Nested access
theme.colors.primary        // Multiple levels
```

### Destructuring

Extract values from arrays, structs, and enums:

```formalang
// Array destructuring (positional)
pub let items = ["first", "second", "third", "fourth"]
pub let [a, b] = items              // a="first", b="second"
pub let [x, ...rest] = items        // x="first", rest=["second", "third", "fourth"]
pub let [_, second, ...] = items    // Skip first, get second, ignore rest

// Struct destructuring (by field name)
pub struct User { name: String, age: Number }
pub let user = User(name: "Alice", age: 30)
pub let {name, age} = user          // name="Alice", age=30
pub let {name as username} = user   // Rename: username="Alice"

// Enum destructuring (extract associated data)
pub enum AccountType {
  admin
  user(permissions: [String], articles: [String])
}

pub let account: AccountType = .user(
  permissions: ["read", "write"],
  articles: ["article1", "article2"]
)

// Destructure enum to extract associated data
pub let (permissions, articles) = account
```

**Rules**:

- Array destructuring is positional (order matters)
- Struct destructuring is by field name
- Enum destructuring extracts associated data in parameter order
- Use `as` to rename fields during destructuring
- Use `_` to skip array elements
- Use `...` for rest pattern (can appear anywhere in array destructuring)
- Dictionaries do not support destructuring

### Binary Operators

All operators with examples:

```formalang
// Arithmetic operators
let sum = 10 + 20
let difference = 50 - 30
let product = 4 * 5
let quotient = 100 / 4
let remainder = 17 % 5

// Comparison operators
let greater = 10 > 5
let less = 3 < 7
let greaterEq = 10 >= 10
let lessEq = 5 <= 5

// Equality operators
let equal = 5 == 5
let notEqual = 5 != 10

// Logical operators
let andResult = true && false
let orResult = true || false

// String concatenation
let greeting = "Hello, " + "World"

// Complex expressions with precedence
let complex = (10 + 20) * 3
let condition = (5 > 3) && (10 < 20)
```

### Operator Precedence

From highest to lowest:

1. **Parentheses**: `( )`
2. **Field access**: `.`
3. **Multiplicative**: `*`, `/`, `%`
4. **Additive**: `+`, `-`
5. **Comparison**: `<`, `>`, `<=`, `>=`
6. **Equality**: `==`, `!=`
7. **Logical AND**: `&&`
8. **Logical OR**: `||`

Examples:

```formalang
10 + 20 * 3              // 70 (multiplication first)
(10 + 20) * 3            // 90 (parentheses override)
x > 5 && y < 10          // Comparison before AND
true || false && false   // true (AND before OR)
user.age > 18 && user.verified  // Field access → comparison → AND
```

---

## Control Flow

All control flow is **compile-time validated**.

### For Expressions

Iterate over arrays:

```formalang
// Basic for loop
for item in items {
  process(item: item)
}

// With field access
for email in user.emails {
  validate(address: email)
}

// With literal array
for n in [1, 2, 3, 4, 5] {
  record(value: n)
}

// Nested loops
for row in matrix {
  for cell in row {
    process(value: cell)
  }
}
```

**Rules**:

- Expression must be an array type
- Returns array of body results
- Loop variable scoped to body

### If Expressions

Conditional expressions:

```formalang
// Boolean condition
if count > 0 {
  showItems()
} else {
  showEmpty()
}

// Without else (returns nil if false)
if isAdmin {
  showAdminPanel()
}

// Optional unwrapping (auto-unwrap)
if user.nickname {
  // nickname is unwrapped and available here
  greet(name: nickname)
}

// Chained conditions
if x > 100 {
  showLarge()
} else if x > 50 {
  showMedium()
} else {
  showSmall()
}
```

**Optional Unwrapping**:

When condition is an optional value:

- If not nil: unwraps and binds value in true branch
- If nil: takes else branch (or returns nil)

### Match Expressions

Pattern matching on enums (exhaustive):

```formalang
pub enum Status { pending, active, completed }

match status {
  .pending: waitFor()
  .active: runNow()
  .completed: finalize()
}

// With data binding (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: Number)
}

match message {
  .text(content): displayText(value: content)
  .image(url, size): displayImage(src: url, bytes: size)
}
```

**Rules**:

- Must be exhaustive (cover all variants)
- Pattern uses `.variant` syntax (short form)
- Associated data bound to identifiers using parameter names

---

## Function Overloading

Multiple functions with the same name are allowed when their signatures differ.
The compiler selects the right overload at each call site.

**Mode A — named-argument label set match** (exact label set determines the overload):

```formalang
fn format(value: Number) -> String { "number" }
fn format(value: String) -> String { "string" }
fn format(value: Number, precision: Number) -> String { "precise" }
```

**Mode B — first-positional-arg type match** (when call has no labels):

```formalang
fn process(Number) -> String { "number" }
fn process(String) -> String { "string" }
```

**Rules**:

- Overloads are distinguished by their named-argument label sets
- Calling with an ambiguous or unknown label set is a compile error
- An unresolvable call site produces `AmbiguousCall` or `NoMatchingOverload`

---

## Generics

Full generic type system with constraints and type inference.

### Generic Structs

```formalang
// Single type parameter
pub struct Box<T> {
  value: T
}

// Multiple type parameters
pub struct Pair<A, B> {
  first: A,
  second: B
}

// With constraints
pub trait Layout {
  width: Number
}

pub struct Container<T: Layout> {
  items: [T],
  gap: Number
}

// Multiple constraints
pub trait Renderable { fn render(self) -> Boolean }
pub trait Clickable { fn click(self) -> Boolean }

pub struct Widget<T: Renderable + Clickable> {
  component: T
}
```

### Generic Traits

```formalang
pub trait Collection<T> {
  items: [T]
}

pub trait Comparable<T> {
  fn compare(self, other: T) -> Number
}
```

### Generic Enums

```formalang
pub enum Result<T, E> {
  ok(value: T)
  error(err: E)
}

pub enum Option<T> {
  some(value: T)
  none
}
```

### Generic Instantiation

```formalang
// With explicit type arguments
pub let string_box = Box<String>(value: "hello")
pub let number_box = Box<Number>(value: 42)
pub let pair = Pair<Number, Boolean>(first: 42, second: true)

// Type inference (when inferrable)
pub let inferred_box = Box(value: "inferred as String")
pub let inferred_pair = Pair(first: 10, second: true)

// Generic enums
pub let success: Result<String, Number> = .ok(value: "success")
pub let failure: Result<String, Number> = .error(err: 404)
pub let maybe: Option<Number> = .some(value: 42)
pub let nothing: Option<Number> = .none
```

### Type Constraints

```formalang
// Single constraint
pub struct Wrapper<T: Named> {
  item: T
}

// Multiple constraints
pub struct Interactive<T: Renderable + Clickable> {
  component: T
}

// Constraint on trait field
pub trait Container<T: Layout> {
  items: [T]
}
```

**Rules**:

- Type parameters use `<T>`, `<A, B>`, etc.
- Constraints use `:` syntax: `<T: Constraint>`
- Multiple constraints use `+`: `<T: A + B>`
- Type arguments must match parameter count (arity)
- Type inference works when types can be determined
- Constraints must reference existing traits

---

## Context System

FormaLang uses the `provides` / `consumes` context system:

```formalang
struct Theme {
  primary: String,
  secondary: String
}

struct User {
  name: String,
  email: String
}

let dark_theme = Theme(primary: "#007AFF", secondary: "#5856D6")
let alice = User(name: "Alice", email: "alice@example.com")

// Provides expression (with alias)
let themed_text = provides dark_theme as theme {
  apply(color: theme.primary)
}

// Multiple provides
let multi_context = provides dark_theme as theme, alice as user {
  format(name: user.email, color: theme.secondary)
}

// Consumes expression
let consumed = provides dark_theme as theme {
  consumes theme {
    apply(color: theme.secondary)
  }
}
```

---

## Module System

### Use Statements

Import definitions from other modules:

```formalang
// Import single item
use components::Button

// Import multiple items
use components::{Button, Text, VStack}

// Import from nested paths
use ui::controls::Slider
use data::models::User

// Import from file
use types::User         // From types.fv
use utils::helpers      // From utils/helpers.fv
```

**Module Resolution**:

- Modules map to `.fv` files
- Path separators use `::`
- Can only import `pub` items
- No circular imports allowed

### Nested Modules

Use `mod` blocks to create nested namespaces within a file:

```formalang
mod alignment {
  pub enum Vertical {
    top
    center
    bottom
  }

  pub enum Horizontal {
    left
    center
    right
  }
}

// Use with namespace path
pub let vertical: alignment::Vertical = .top
pub let horizontal: alignment::Horizontal = .center

// Can also import nested items
use alignment::Vertical

pub let v: Vertical = .bottom
```

**Multiple Levels**:

```formalang
mod ui {
  pub mod layout {
    pub enum Direction {
      horizontal
      vertical
    }
  }

  pub struct Theme {
    primary: String,
    secondary: String
  }
}

pub let direction: ui::layout::Direction = .horizontal
pub let theme: ui::Theme = ui::Theme(
  primary: "#007AFF",
  secondary: "#5856D6"
)
```

### File Structure Example

```text
project/
├── main.fv
├── types.fv
├── components/
│   ├── button.fv
│   └── text.fv
└── utils/
    └── helpers.fv
```

```formalang
// In main.fv
use types::User
use components::{Button, Text}
use utils::helpers::formatDate
```

---

## Serde Stability

The `File` AST type carries a `format_version` field. Serialized ASTs produced
by this version of the compiler will always have `format_version == 1`. Tools
that consume serialized ASTs should check this field to detect incompatible
wire-format changes.

```formalang
// All parsed files automatically have format_version: 1 set
```

All public AST types implement `Serialize` / `Deserialize` and are marked
`#[non_exhaustive]` so that adding new variants or fields in future releases
does not break existing consumers at the API boundary.

---

## Complete Feature Checklist

### Implemented Features

**Core Language**:

- Comments (single-line `//`, multi-line `/* */`)
- Visibility modifiers (`pub`)
- Use statements (Rust-style imports with `::` and `{}`)

**Type System**:

- Primitive types (`String`, `Number`, `Boolean`, `Path`, `Regex`, `Never`)
- Array types (`[Type]`, `[Type, N]` for fixed-size)
- Dictionary types (`[KeyType: ValueType]`)
- Optional types (`Type?`)
- Generic types (`Type<T>`, `Type<T: Constraint>`)
- Closure types (`T -> U`, `T, U -> V`, `() -> T`)
- Type inference

**Definitions**:

- Struct definitions
- Inherent impl blocks (methods)
- Trait definitions (field requirements and method signatures)
- `impl Trait for Type` conformance blocks
- Enum definitions (with associated data, generics)
- `extern type` declarations
- `extern fn` declarations
- `extern impl` blocks
- Function definitions with optional overloading
- Let bindings (file-level, with `pub`, `mut`)
- Generic parameters on structs, traits, enums

**Expressions**:

- All literals (string, number, boolean, nil, path, regex, array, dictionary)
- Binary operators (arithmetic, comparison, equality, logical, concatenation)
- Field access (including nested)
- Destructuring (arrays, structs, enums)
- Struct and enum instantiation
- Closure expressions
- Correct operator precedence

**Control Flow**:

- For expressions (array iteration)
- If expressions (with boolean and optional unwrapping)
- Match expressions (exhaustive pattern matching)

**Context System**:

- Context system (`provides` / `consumes`)

**Generics**:

- Generic type parameters with constraints
- Generic structs, traits, enums
- Generic instantiation with type arguments and inference
- Nested generics, generic arity validation

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

### Not Yet Implemented

- Incremental compilation (salsa)
- Code formatter
- REPL mode
- VSCode extension (full integration)
- Evaluation/expansion stage (runtime)
