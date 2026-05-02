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

Doc comments use `///` (item-level) or `//!` (parent / file-level) and
attach to the following declaration. They flow through to the IR and are
available to backends as the `doc:` field on most definitions.

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

Reserved words that cannot be used as identifiers:

```text
trait    struct   enum     use      pub      impl     mod
let      mut      sink     match    for      in       if
else     true     false    nil      as       extern
fn       self     inline   no_inline cold
```
