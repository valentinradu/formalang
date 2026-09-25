# Core Constructs

## Comments

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

Doc comments use `///` (item-level) or `//!` (parent / file-level). A
`///` comment attaches to the declaration after it. They flow through to
the IR and are available to backends as the `doc:` field on most
definitions.

A `//!` comment documents the file or the `mod` that holds it. Put it
at the start of the file, or at the start of the body of a `mod`. The
AST keeps it in `File.doc` or in the `doc` field of the module. At any
other place, a `//!` comment is a `ParseError`:

```formalang
//! Shapes and their areas.

/// A square.
pub struct Square { side: I32 }

pub mod units {
  //! The units of a length.
  pub struct Metre { value: I32 }
}
```

**Bidirectional control characters**: a comment or a string must not
hold the characters U+202A to U+202E and U+2066 to U+2069. These
characters change the direction of the text around them, so an editor
can show the source in an order that is not the order that the compiler
reads (CVE-2021-42574, "Trojan Source"). The compiler refuses them with
the error `BidirectionalControl`. In a string, write the escape
`\u202E` if you need such a character. The escape is visible in the
source, so the compiler accepts it:

```formalang
pub let marker: String = "\u202E"

pub fn run_checks() {
  assert(condition: marker.len() == 3)   // three bytes of UTF-8
}
```

## Statements

In a function body or a block, a statement ends at a line break, at
the `}` that closes the block, or at a `{` that starts a block
statement. There is no terminator, and two statements on one line are
an error:

```formalang,reject=ParseError
pub fn f() -> I32 {
    let a = 1 let b = 2
    a + b
}
```

A line that ends with an operator, a `,` or an open bracket continues
on the next line. A line that starts with `.`, `else` or `{` continues
the line above:

```formalang
pub fn pick(big: Boolean) -> I32 {
  let base = 10 +
    5
  let total = [1, 2, 3]
    .len()
  if big
  {
    base
  }
  else
  {
    total
  }
}
```

A line that starts with another operator, such as `-` or `+`, is a new
statement.

The rule also applies at module level, in the body of a `mod` and in
the body of an `impl`. Each definition starts on a new line:

```formalang,reject=ParseError
pub let a: I32 = 1 pub let b: I32 = 2
```

## Nesting Depth

The compiler has a limit on how deep a program nests. Each open `(`,
`[` or `{` adds one level, and so does each operator in a chain such
as `1 + 1 + 1`. Above 1024 levels, the parser stops with a
`ParseError`. An expression more than 500 levels deep gets the error
`ExpressionDepthExceeded` (E130). A real program is far below both
limits. If you reach one, put a part of the expression in a `let`.

## Visibility Modifiers

Control export visibility with the `pub` keyword:

```formalang
// Public - can be imported by other modules
pub struct User { name: String }
pub trait Named { name: String }
pub enum Status { active, inactive }
pub let MAX_USERS: I32 = 100

// Private - module-local only (default)
struct Internal { id: I32 }
trait Helper { key: String }
enum State { ready, processing }
let secret_key: String = "xyz"
```

## Keywords

Reserved words that cannot be used as identifiers. `self` is one of
them: it names the receiver of a method, and nothing else. See
[Traits & Impls](traits.md#impl-blocks).

```text
trait    struct   enum     use      pub      impl     mod
let      mut      sink     match    for      in       if
else     true     false    nil      as       extern
fn       self     inline   no_inline cold
```
