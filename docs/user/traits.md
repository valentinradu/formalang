# Traits & Impls

Traits declare requirements (fields and method signatures); `impl` blocks
attach methods to a concrete type or declare conformance.

## Trait Definitions

```formalang
// Fields only
pub trait Named {
  name: String
}

// Fields and methods
pub trait Shape {
  color: String
  fn area(self) -> I32
  fn perimeter(self) -> I32
}

// Methods only
pub trait Drawable {
  fn draw(self) -> Boolean
  fn visible(self) -> Boolean
}

// Trait composition (inheritance)
pub trait Entity: Named + Identifiable {
  createdAt: I32
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

## Impl Blocks

Impl blocks add methods to a struct (inherent impl) or declare trait
conformance (impl Trait for Struct).

**Inherent impl**: methods belong to the struct:

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

## Impl Trait for Type

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
  radius: I32
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
  fn id(self) -> I32
}

pub trait Extended: Base {
  fn name(self) -> String
}

pub struct Item {
  value: I32
}

impl Base for Item {
  fn id(self) -> I32 {
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

## Trait-Bounded Polymorphism

A trait is a **constraint, never the type of a value**. Write
`<T: Printable>`, not `item: Printable`. Every call in `FormaLang`
dispatches statically, so there is no vtable and no indirect call.

```formalang
pub trait Printable {
  fn label(self) -> String
}

pub struct Doc {
  text: String
}

impl Printable for Doc {
  fn label(self) -> String { self.text }
}

// Correct: a generic bound. Monomorphisation makes one specialised
// function per concrete type, and the call is direct.
fn print_it<T: Printable>(item: T) -> String {
  item.label()
}

// Error E063: a trait cannot be the type of a value.
fn print_any(item: Printable) -> String {
  item.label()
}
```

The rule holds at every value position: a parameter, a return type, a
`let` annotation, a struct field, an enum variant field, an array
element, a dictionary value, and a closure parameter.

### When the type is chosen at run time

A generic bound fixes one concrete type per call. For a value that may
hold any of several types, declare an enum with one variant per type
and `match` on it:

```formalang
pub struct Square { side: I32 }
pub struct Rect { w: I32, h: I32 }

impl Square { fn area(self) -> I32 { self.side * self.side } }
impl Rect { fn area(self) -> I32 { self.w * self.h } }

pub enum AnyShape {
  square(value: Square),
  rect(value: Rect)
}

fn area_of(shape: AnyShape) -> I32 {
  match shape {
    .square(value): value.area(),
    .rect(value): value.area()
  }
}

// Two branches, one type — no cast needed.
fn pick(k: I32) -> AnyShape {
  if k == 0 {
    .square(value: Square(side: 2))
  } else {
    .rect(value: Rect(w: 2, h: 3))
  }
}
```

The enum lists its variants in one place, so adding a type means
editing the enum and each `match`. In exchange the compiler checks that
every case is handled, and the generated code needs no vtable.

### Generic Traits

Traits can themselves be generic, and constraints / impls can carry
the concrete arguments:

```formalang
pub trait Container<T> {
  fn get(self) -> T
}

pub struct Box {
  value: I32
}

impl Container<I32> for Box {
  fn get(self) -> I32 { self.value }
}

fn unwrap<T: Container<I32>>(b: T) -> I32 {
  b.get()
}
```

The monomorphisation pass clones generic traits, structs, enums,
and functions per unique argument tuple, then rewrites every
reference (including `DispatchKind::Virtual` on now-concrete
receivers) to point at the specialised clone. After mono runs, no
generic definitions remain in the IR.

**Allowed trait positions**:

- Generic constraint: `<T: Trait>` or `<T: Trait<X>>`
- Impl target: `impl Trait for Foo` or `impl Trait<X> for Foo`
- Trait composition: `trait A: B + C`

**Rejected trait positions** (use a generic bound instead):

- Function parameter type: `fn foo(x: Trait)` ✗
- Function return type: `fn make() -> Trait` ✗
- Let annotation: `let x: Trait = ...` ✗
- Struct/enum field: `field: Trait` ✗
- Closure params/return: `(x: Trait) -> I32` ✗
