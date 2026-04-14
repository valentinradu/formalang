# FormaLang Language Features Reference

Reference of all supported language features with practical examples.

---

## Table of Contents

- [Core Constructs](#core-constructs)
- [Type System](#type-system)
  - [Closure Types](#closure-types)
- [Definitions](#definitions)
- [Expressions](#expressions)
  - [Closure Expressions](#closure-expressions)
- [Control Flow](#control-flow)
- [Context System](#context-system)
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
else     true     false    nil      as       mount
provides consumes
```

---

## Type System

### Primitive Types

FormaLang has two categories of primitive types.

**General-purpose types:**

```formalang
pub struct Primitives {
  text: String,           // Text data
  count: Number,          // Numeric values (int or float)
  active: Boolean,        // true or false
  logo: Path,             // File/resource paths
  pattern: Regex          // Regular expressions
}
```

**GPU/numeric types** (for math-heavy code and GPU backends):

```formalang
pub struct GpuData {
  a: f32,                 // 32-bit float
  b: i32,                 // 32-bit signed integer
  c: u32,                 // 32-bit unsigned integer
  d: bool,                // GPU boolean
  pos: vec2,              // 2-component float vector
  dir: vec3,              // 3-component float vector
  color: vec4,            // 4-component float vector
  ipos: ivec2,            // 2-component integer vector
  idir: ivec3,            // 3-component integer vector
  m2: mat2,               // 2x2 matrix
  m3: mat3,               // 3x3 matrix
  transform: mat4         // 4x4 matrix
}
```

### Never Type

`Never` is an uninhabited type - it has no values and cannot be instantiated.
It is used to mark terminal structs that have no children:

```formalang
pub trait View {
  mount body: View
}

// Terminal view - body is Never (no children possible)
pub struct Empty: View {
  mount body: Never
}

// Composite view - body is concrete View type
pub struct VStack: View {
  mount body: View    // accepts children
}
```

`Never` automatically satisfies any trait requirement. Fields of type `Never`
require no default value since the containing expression is terminal.

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

pub let user2 = User(
  name: "Bob",
  email: "bob@example.com",
  nickname: nil,
  avatar: nil
)
```

### Dictionary Types

Key-value mappings using bracket syntax with colon:

```formalang
// Dictionary types (keys can be any compiler-supported type)
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

// Nested dictionaries
pub let config: [String: [String: Number]] = [
  "server": ["port": 8080, "timeout": 30],
  "client": ["retries": 3, "delay": 100]
]

// Accessing dictionary values
pub let timeout = settings["timeout"]
pub let grade = scores[100]
pub let logo_type = assets[/logo.svg]
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
// Tuple type syntax
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
  Label(text: person.name)
}

// Tuple with type annotation
for item in items {
  let data: (label: String, value: Number) = (label: item.name, value: item.count)
  Row(data: data)
}

// Accessing tuple fields
for item in items {
  let person = (name: "John", age: 30)
  Label(text: person.name)      // Access by field name
  Label(text: person.age)
}

// Nested tuple access
for item in items {
  let nested = (user: (first: "John", last: "Doe"), active: true)
  Label(text: nested.user.first)
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

// Basic struct with optional fields
pub struct User {
  name: String,
  email: String,
  age: Number,
  verified: Boolean,
  nickname: String?             // Optional field
}

// With trait implementation
pub trait Named {
  name: String
}

pub struct Person: Named {
  name: String,       // Satisfies Named trait
  age: Number
}

// With mount fields (regular fields before mount fields)
pub struct Panel {
  title: String,
  padding: Number,
  mount content: Layout         // Mount field for composition
}

// With mutable fields
pub struct Counter {
  mut count: Number,            // Mutable field (can be updated)
  mut active: Boolean,
  label: String                 // Immutable field
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

**Field Modifiers**:

- `mut` - Mutable field (can be updated after initialization)
- `mount` - Mount field (composition point, cannot be mutable)
- `?` - Optional type (can be nil)

**Important**: Mount fields cannot be marked as `mut`

### Impl Blocks

Impl blocks provide preset values for struct fields. Fields with defaults become
optional at instantiation:

```formalang
// Struct definition
pub struct Button {
  label: String,
  disabled: Boolean,
  color: String,
  mount icon: Image
}

// Impl block
impl Button {
  disabled: false,
  color: "blue",
  icon: DefaultIcon()
}

// Struct with mount
pub struct UserView {
  user: User,
  role: UserRole,
  mount avatar: Image
}

impl UserView {
  user: DefaultUser(),
  role: .guest,
  avatar: PlaceholderImage()
}

// Generic struct with impl block
pub struct Card<T> {
  content: T,
  padding: Number,
  mount header: Layout
}

impl Card<T> {
  padding: 16,
  header: EmptyView()
}
```

**Rules**:

- Impl blocks are optional (not all structs need them)
- Fields with defaults become optional at instantiation
- Fields without defaults must always be provided
- Can override defaults explicitly
- Mount fields can have defaults
- Generic structs can have impl blocks (type parameter in scope)

**Instantiation** with defaults:

```formalang
// Using all defaults (only required fields provided)
Button(label: "Click me")

// Overriding some defaults
Button(label: "Submit", color: "green", disabled: false)

// With mounts (required if no default)
UserView(user: currentUser, role: .admin) {
  avatar: CustomAvatar()
}

// All fields have defaults, omit everything
Button()

// If all mounts have defaults, omit block
UserView(user: currentUser)
```

### Trait Definitions

Traits define interfaces/protocols via structural typing:

```formalang
// Basic trait
pub trait Named {
  name: String
}

// Multiple fields
pub trait Identifiable {
  id: Number,
  name: String
}

// Trait composition (inheritance)
pub trait Entity: Named + Identifiable {
  createdAt: Number
}

// Trait with mount field
pub trait Container {
  gap: Number,
  mount items: Layout
}

// Generic trait
pub trait Collection<T> {
  items: [T]
}

// Struct implementing traits (structural typing)
pub struct User: Named + Identifiable {
  name: String,
  id: Number,
  email: String
}
```

**Trait Rules**:

- Traits do NOT have `impl` blocks
- Structs satisfy traits by matching field names and types (structural typing)
- Mount fields in traits must be matched by mount fields in structs
- Trait composition (`+`) combines field requirements
- Default values would conflict in trait composition
- A struct implements a trait if it has all required fields with correct types

#### Trait-Bounded Polymorphism

Traits can be used as types in field declarations:

```formalang
pub trait View {
  mount body: View
}

pub struct Container {
  mount content: View      // Accepts any View implementor
}

pub struct VStack: View { ... }
pub struct HStack: View { ... }

// Both are valid - concrete type is known at compile time
Container() { content: VStack() { ... } }
Container() { content: HStack() { ... } }
```

FormaLang resolves all trait-bounded types at compile time. The AST always stores
the concrete type, not the trait bound. When you write:

```formalang
let container: Container = Container() {
  content: VStack() { ... }
}
```

The compiler knows `content` is `VStack`, not just "some View". This enables:

- Zero runtime overhead for trait abstraction
- Full type information available in the AST
- Static verification of all trait requirements

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

// With named associated data
pub enum UserRole {
  guest
  member(tier: String)
  admin(department: String)
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
pub let status2: Status = .active

pub let msg1: Message = .text(content: "Hello")
pub let msg2: Message = .image(url: /pic.jpg, size: 1024)

pub let role1: UserRole = .guest
pub let role2: UserRole = .admin(department: "Engineering")

pub let result1: Result<String, Number> = .ok(value: "success")
pub let result2: Result<String, Number> = .error(err: 404)
```

### Let Expressions

Let expressions create local bindings inside blocks (if, for, match, mount children):

```formalang
// Let in for block
for item in items {
  let formatted = item.name + ": " + item.value
  Label(text: formatted)
}

// Let in if block
if condition {
  let temp = computeValue()
  Label(text: temp)
}

// Let with type annotation
for item in items {
  let count: Number = item.count
  Label(text: count)
}

// Mutable let (can be reassigned within scope)
for item in items {
  let mut counter = 0
  Label(text: "test")
}

// Let in mount children block
VStack(gap: 10) {
  items: {
    let user = currentUser
    Label(text: user.name)
    Button(label: "Edit")
  }
}

// Let with tuple value
for item in items {
  let data = (name: item.name, count: item.count)
  Row(label: data.name, value: data.count)
}
```

**Rules**:

- Only inside blocks (for, if, match, mount children)
- Can be `mut` for mutability tracking
- Type inference when annotation omitted
- Scope limited to containing block

### Mutability

The `mut` keyword marks values as mutable (can be updated after initialization).
It can be used in three places:

#### 1. Mutable Fields

```formalang
pub struct Counter {
  mut count: Number,            // Mutable field
  mut active: Boolean,
  label: String                 // Immutable field
}

pub struct App {
  mut state: String,
  version: Number,
  mount content: Layout         // Mount fields cannot be mutable
}
```

#### 2. Mutable Let Expressions

```formalang
// Mutable bindings can be reassigned within their scope
for item in items {
  let mut counter = 0
  let mut theme = "dark"
  Label(text: theme)
}

// Immutable bindings cannot be reassigned
for item in items {
  let version = "1.0"
  Label(text: version)
}
```

#### 3. Mutable Struct Fields in Instantiation

```formalang
pub struct State {
  mut data: [String],
  mut count: Number
}

// Struct with mutable fields
impl Config {
  app_state: State(
    data: ["initial"],
    count: 0
  )
}

// The mut fields can be updated after creation
```

**Restrictions**:

- Mount fields cannot be marked as `mut`
- Mutability is tracked for validation purposes

---

## Expressions

### Literals

All literal types as expressions:

```formalang
pub struct Literals {
  // String literals
  text: String,
  multiline: String,

  // Number literals
  integer: Number,
  negative: Number,
  float: Number,
  withUnderscore: Number,

  // Boolean literals
  yes: Boolean,
  no: Boolean,

  // Nil literal
  nothing: String?,

  // Array literals
  tags: [String],
  numbers: [Number],
  empty: [String],

  // Dictionary literals
  settings: [String: Number],
  scores: [Number: String],
  emptyDict: [String: Boolean]

  // Path literals (not shown in struct, used as values)
  // /assets/logo.svg
  // /images/background.png

  // Regex literals (not shown in struct, used as values)
  // r/[a-z]+/i
  // r/\d{3}-\d{4}/
}

// Example instantiation
pub let literals = Literals(
  text: "Hello, World",
  multiline: """
    Multi-line
    string literal
  """,
  integer: 42,
  negative: -17,
  float: 3.14,
  withUnderscore: 1_000_000,
  yes: true,
  no: false,
  nothing: nil,
  tags: ["icon", "svg", "asset"],
  numbers: [16, 32, 64],
  empty: [],
  settings: ["timeout": 30, "retries": 3],
  scores: [100: "perfect", 90: "great"],
  emptyDict: [:]
)
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

// Closure expressions
impl Form {
  // Single parameter - no parens needed
  onChange: x -> .textChanged(value: x),

  // Multiple parameters - comma separated
  onResize: w, h -> .resized(width: w, height: h),

  // No parameters - empty parens required
  onSubmit: () -> .submit
}
```

**With explicit type annotations** (when inference fails):

```formalang
impl Form {
  onChange: x: String -> .textChanged(value: x),
  onResize: w: Number, h: Number -> .resized(width: w, height: h)
}
```

**Expression syntax**:

| Parameters | Syntax         | Example                        |
| ---------- | -------------- | ------------------------------ |
| None       | `() -> expr`   | `() -> .submit`                |
| One        | `x -> expr`    | `x -> .changed(value: x)`      |
| Multiple   | `x, y -> expr` | `x, y -> .point(x: x, y: y)`   |
| With types | `x: T -> expr` | `x: String -> .text(value: x)` |

**Rules**:

- Closures are **pure** - no side effects, single expression body
- Single parameter does not need parentheses
- Multiple parameters are comma-separated
- Empty parameters require parentheses: `() -> expr`
- Type annotations optional when inferable

### Instantiation

#### Struct Instantiation

Structs use parentheses for regular fields and curly braces for mount fields:

```formalang
pub struct Point { x: Number, y: Number }
pub struct User {
  name: String,
  email: String,
  age: Number
}

// Basic instantiation (parentheses for fields)
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

#### Instantiation with Mount Fields

Mount fields go in curly braces `{}` after the parentheses:

```formalang
pub struct Panel {
  title: String,
  padding: Number,
  mount content: Layout
}

pub struct VStack {
  gap: Number,
  mount items: [Layout]
}

// Single mount field
Panel(title: "Settings", padding: 10) {
  content: Text(text: "Content")
}

// Multiple mount field (no commas in block)
VStack(gap: 10) {
  items: {
    Text(content: "First")
    Button(label: "Click")
    Text(content: "Last")
  }
}

// Mount field with for loop
VStack(gap: 10) {
  items: for item in list {
    Text(content: item)
  }
}

// Multiple mount fields
pub struct Modal {
  title: String,
  mount header: Layout,
  mount footer: Layout
}

Modal(title: "Dialog") {
  header: Text(content: "Header")
  footer: Button(label: "OK")
}

// If all mounts have defaults, omit block
Button(label: "Click")
```

**Instantiation Rules**:

- Regular fields go in `()` with commas: `Type(field: value, other: value)`
- Mount fields go in `{}` without commas between children
- Mount fields NEVER go in `()`
- If all mounts have defaults, the `{}` block can be omitted
- Multiple mounts in one block, separated by newlines

#### Mount Block Syntax

The `{}` mount block groups multiple children without commas or array brackets.
Mount fields always accept one or many children of the specified type:

```formalang
pub struct Container {
  mount body: View           // Accepts one or many Views
}

pub struct Stack {
  mount items: View          // Accepts one or many Views
}

// Multiple children - use mount block
Container() {
  body: {
    Text(content: "First")
    Text(content: "Second")
    Text(content: "Third")
  }
}

// Same syntax for any mount field
Stack() {
  items: {
    Text(content: "Item 1")
    Text(content: "Item 2")
  }
}

// Single child - block optional
Container() {
  body: Text(content: "Single child")
}
```

The mount block wraps children automatically - no array syntax needed in type declarations.

#### Enum Instantiation

```formalang
// Simple variant (leading dot notation)
let status1: Status = .pending
let status2: Status = .active
let user: UserType = .guest

// With named parameters
let msg1: Message = .text(content: "Hello")
let msg2: Message = .image(url: /pic.jpg, size: 1024)

// With named data
let role1: UserRole = .admin(department: "Engineering")
let role2: UserRole = .member(tier: "Gold")

// Generic enum
let result1: Result<String, Number> = .ok(value: "Success")
let result2: Result<String, Number> = .error(err: 404)
let opt1: Option<Number> = .some(value: 42)
let opt2: Option<Number> = .none
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
pub let [first, ..., last] = items  // Get first and last, ignore middle

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
// permissions = ["read", "write"], articles = ["article1", "article2"]

// Nested destructuring with enums
pub let ([firstPerm, ...], articles) = account
// firstPerm = "read", articles = ["article1", "article2"]

// Partial destructuring (remaining fields ignored)
pub let {name} = user               // Only extract name
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
pub struct Operations {
  // Arithmetic operators
  sum: Number,
  difference: Number,
  product: Number,
  quotient: Number,
  remainder: Number,

  // Comparison operators
  greater: Boolean,
  less: Boolean,
  greaterEq: Boolean,
  lessEq: Boolean,

  // Equality operators
  equal: Boolean,
  notEqual: Boolean,

  // Logical operators
  andResult: Boolean,
  orResult: Boolean,

  // String concatenation
  greeting: String,

  // Complex expressions with precedence
  complex: Number,
  condition: Boolean
}

// Example instantiation
pub let ops = Operations(
  sum: 10 + 20,                             // Addition
  difference: 50 - 30,                      // Subtraction
  product: 4 * 5,                           // Multiplication
  quotient: 100 / 4,                        // Division
  remainder: 17 % 5,                        // Modulo
  greater: 10 > 5,                          // Greater than
  less: 3 < 7,                              // Less than
  greaterEq: 10 >= 10,                      // Greater or equal
  lessEq: 5 <= 5,                           // Less or equal
  equal: 5 == 5,                            // Equal
  notEqual: 5 != 10,                        // Not equal
  andResult: true && false,                 // Logical AND
  orResult: true || false,                  // Logical OR
  greeting: "Hello, " + "World",            // Concatenation
  complex: (10 + 20) * 3,                   // 90
  condition: (5 > 3) && (10 < 20)           // true
)
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

All control flow is **compile-time validated** (expanded in future stage).

### For Expressions

Iterate over arrays:

```formalang
// Basic for loop
for item in items {
  Text(content: item)
}

// With field access
for email in user.emails {
  EmailCard(address: email)
}

// With literal array
for n in [1, 2, 3, 4, 5] {
  Badge(value: n)
}

// Nested loops
for row in matrix {
  for cell in row {
    Cell(value: cell)
  }
}

// Real example with mount field
pub struct EmailList {
  emails: [String],
  mount items: Layout
}

impl EmailList {
  items: for email in emails {
    Text(content: email)
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
  Text(content: "Has items")
} else {
  Text(content: "Empty")
}

// Without else (returns nil if false)
if isAdmin {
  AdminPanel()
}

// Optional unwrapping (auto-unwrap)
if user.nickname {
  // nickname is unwrapped and available here
  Text(content: "Hi, " + nickname)
}

// Optional with else
if session.token {
  AuthView(token: token)    // token unwrapped
} else {
  LoginView()
}

// Chained conditions
if x > 100 {
  Text(content: "Large")
} else if x > 50 {
  Text(content: "Medium")
} else {
  Text(content: "Small")
}

// Boolean operators in conditions
if isActive && hasPermission {
  Dashboard()
}

if isGuest || isTrial {
  LimitedView()
}
```

**Optional Unwrapping**:

When condition is an optional value:

- If not nil: unwraps and binds value in true branch
- If nil: takes else branch (or returns nil)

### Match Expressions

Pattern matching on enums (exhaustive):

```formalang
// Simple enum matching
pub enum Status { pending, active, completed }

match status {
  .pending: Text(content: "Waiting")
  .active: Text(content: "Active")
  .completed: Text(content: "Done")
}

// With data binding (named parameters)
pub enum Message {
  text(content: String)
  image(url: String, size: Number)
  video(url: String, duration: Number)
}

match message {
  .text(content): TextMessage(text: content)
  .image(url, size): ImageMessage(src: url, bytes: size)
  .video(url, duration): VideoPlayer(src: url, length: duration)
}

// Named data binding
pub enum UserRole {
  guest
  member(tier: String)
  admin(department: String)
}

match user.role {
  .guest: GuestView()
  .member(tier): MemberView(level: tier)
  .admin(department): AdminView(dept: department)
}

// Generic enum
pub enum Result<T, E> {
  ok(value: T)
  error(err: E)
}

match result {
  .ok(value): SuccessView(data: value)
  .error(err): ErrorView(error: err)
}
```

**Rules**:

- Must be exhaustive (cover all variants)
- Pattern uses `.variant` syntax (short form)
- Associated data bound to identifiers using parameter names
- Unmatched branches not included in output

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
  Text(content: "Hello", color: theme.primary)
}

// Nested provides
let user_text = provides dark_theme as theme {
  provides alice as user {
    Text(content: user.name, color: theme.primary)
  }
}

// Multiple provides
let multi_context = provides dark_theme as theme, alice as user {
  Text(content: user.email, color: theme.secondary)
}

// Consumes expression
let consumed = provides dark_theme as theme {
  consumes theme {
    Text(content: "Consumed", color: theme.secondary)
  }
}

// Consumes with multiple names
let multi_consume = provides dark_theme as theme, alice as user {
  consumes theme, user {
    Text(content: user.name, color: theme.primary)
  }
}
```

**How it works**:

- Use `provides expr as name` to provide values to component trees
- Direct access via the provided name (e.g., `theme.primary`)
- Use `consumes name` to explicitly mark consumption points
- Supports nesting and multiple provides

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

pub struct Triple<A, B, C> {
  first: A,
  second: B,
  third: C
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
pub trait Renderable { render: Boolean }
pub trait Clickable { onClick: String }

pub struct Widget<T: Renderable + Clickable> {
  component: T
}

// Nested generics
pub struct NestedBox<T> {
  inner: Box<T>
}

// Generic with optional
pub struct MaybeBox<T> {
  value: T?
}

// Generic array container
pub struct ArrayHolder<T> {
  items: [T]
}
```

### Generic Traits

```formalang
pub trait Collection<T> {
  items: [T]
}

pub trait Comparable<T> {
  compare: T
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

// Multi-parameter generic enum
pub enum Either<L, R> {
  left(value: L)
  right(value: R)
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

// Nested generics
pub let nested = NestedBox<String>(
  inner: Box<String>(value: "nested")
)

// Arrays of generics
pub let boxes = [
  Box<Number>(value: 1),
  Box<Number>(value: 2),
  Box<Number>(value: 3)
]
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

// Constraint on trait
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
use utils::helpers      // From utils/helpers.fv or utils/helpers/mod.fv
```

**Module Resolution**:

- Modules map to `.fv` files
- Path separators use `::`
- Can only import `pub` items
- No circular imports allowed

### Nested Modules

Use `mod` blocks to create nested namespaces within a file:

```formalang
// Define nested module
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

**Nested Module Rules**:

- Use `mod name { ... }` to define a nested module
- Access items with `::` path separator: `module::Type`
- Items inside `mod` blocks must be `pub` to be accessible outside
- Can nest `mod` blocks multiple levels deep
- Module names follow same rules as other identifiers

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

// Access nested items
pub let direction: ui::layout::Direction = .horizontal
pub let theme: ui::Theme = ui::Theme(
  primary: "#007AFF",
  secondary: "#5856D6"
)

// Import nested items
use ui::layout::Direction
use ui::Theme

pub let dir: Direction = .vertical
pub let t: Theme = Theme(primary: "red", secondary: "blue")
```

**Private vs Public in Modules**:

```formalang
mod utils {
  // Public - accessible outside module
  pub struct Config {
    name: String
  }

  // Private - only accessible within module
  struct Internal {
    secret: String
  }

  pub enum Status {
    active
    inactive
  }
}

// OK: Config is public
pub let config: utils::Config = utils::Config(name: "app")

// ERROR: Internal is private
// pub let internal: utils::Internal = ...  // Would fail
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

## Complete Feature Checklist

### ✅ Implemented Features

**Core Language**:

- ✅ Comments (single-line `//`, multi-line `/* */`)
- ✅ Visibility modifiers (`pub`)
- ✅ Use statements (Rust-style imports with `::` and `{}`)

**Type System**:

- ✅ Primitive types (`String`, `Number`, `Boolean`, `Path`, `Regex`)
- ✅ GPU numeric types (`f32`, `i32`, `u32`, `bool`, `vec2`–`vec4`, `ivec2`–`ivec4`, `mat2`–`mat4`)
- ✅ Array types (`[Type]`, `[Type, N]` for fixed-size)
- ✅ Dictionary types (`[KeyType: ValueType]` with String/Number/enum keys)
- ✅ Optional types (`Type?`)
- ✅ Generic types (`Type<T>`, `Type<T: Constraint>`)
- ✅ Closure types (`T -> U`, `T, U -> V`, `() -> T`)
- ✅ Type inference

**Definitions**:

- ✅ Struct definitions (with optionals, traits, mount fields)
- ✅ Impl blocks (preset field values)
- ✅ Trait definitions (with composition via `+`, structural typing)
- ✅ Enum definitions (with associated data, generics)
- ✅ Let bindings (file-level, with `pub`, `mut`)
- ✅ Generic parameters on structs, traits, enums

**Expressions**:

- ✅ All literals (string, number, boolean, nil, path, regex, array, dictionary)
- ✅ Binary operators (arithmetic, comparison, equality, logical, concatenation)
- ✅ Field access (including nested)
- ✅ Destructuring (arrays, structs, enums)
- ✅ Instantiation (struct with `()` for fields and `{}` for mounts, enum with `.variant`)
- ✅ Closure expressions (`x -> expr`, `x, y -> expr`, `() -> expr`)
- ✅ Operator precedence (correct order)

**Control Flow**:

- ✅ For expressions (array iteration)
- ✅ If expressions (with boolean and optional unwrapping)
- ✅ Match expressions (exhaustive pattern matching)

**Context System**:

- ✅ Context system (`provides` / `consumes`)

**Generics**:

- ✅ Generic type parameters
- ✅ Generic constraints (single and multiple)
- ✅ Generic structs, traits, enums
- ✅ Generic instantiation with type arguments
- ✅ Type inference for generics
- ✅ Nested generics
- ✅ Generic arity validation

**Module System**:

- ✅ Use statements
- ✅ Module path resolution
- ✅ Visibility control
- ✅ Multi-item imports
- ✅ Nested modules (`mod` blocks)

**Validation** (6-pass semantic analysis):

- ✅ Module resolution (use statements)
- ✅ Symbol table building (all definitions)
- ✅ Type resolution (all types exist)
- ✅ Expression validation (type compatibility, exhaustiveness)
- ✅ Trait validation (complete implementation)
- ✅ Cycle detection (no circular dependencies)

**Tooling**:

- ✅ Lexer (logos-based tokenization)
- ✅ Parser (chumsky-based parsing)
- ✅ AST (complete representation)
- ✅ Error reporting (ariadne-based diagnostics)
- ✅ CLI tool (`fvc check`, `fvc watch`)
- ✅ LSP server (diagnostics, completion, hover)

### 📋 Not Yet Implemented

**Future Enhancements**:

- ⏳ Incremental compilation (salsa)
- ⏳ Code formatter
- ⏳ REPL mode
- ⏳ VSCode extension (full integration)
- ⏳ Evaluation/expansion stage (runtime)

---

## Summary

FormaLang is a **feature-complete compile-time validated language** with:

- **Semantic analyzer** ensuring correctness
- **Full generic type system** with constraints and inference
- **Context system**
- **Exhaustive pattern matching** on enums
- **Module system** with Rust-style imports
- **Beautiful error reporting** with ariadne
- **Real-time tooling** (CLI + LSP)

All features listed in this document should be **fully implemented, tested,
and production-ready**.
