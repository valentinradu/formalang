# AST Overview

The FormaLang compiler produces a validated AST as a Rust data structure.
The AST represents the complete structure of a `.fv` source file after
parsing and semantic validation.

> **Note**: For code generation, use the
> [IR (Intermediate Representation)](../ir/overview.md) instead. The IR
> provides resolved types, linked references, and is optimized for
> backend code generation.

## Obtaining the AST

Use `compile_with_analyzer` for a fully validated AST plus the semantic
analyzer (useful for LSP tooling). For pure syntax inspection without
semantic checks, use `parse_only`.

```rust
use formalang::compile_with_analyzer;

let source = r#"
pub struct User {
    name: String,
    age: I32
}
"#;

match compile_with_analyzer(source) {
    Ok((file, _analyzer)) => {
        // file is the root AST node
        for statement in &file.statements {
            // process statements
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("Error: {}", error);
        }
    }
}
```

Use `parse_only` for syntax-only parsing without semantic validation:

```rust
use formalang::parse_only;

let file = parse_only(source)?;
```

## Parsing

`parse_only` runs the lexer, then the parser. The two phases report
their errors together: a lexer error does not stop the parse.

### Strings and comments

The lexer scans a string literal by hand, not with a regular
expression. The code is in `src/lexer/token/strings.rs`. A line break
or the end of the input stops a `"..."` literal, and the lexer then
reports `UnterminatedString`. A `"""..."""` literal ends at the first
`"""` that no backslash escapes.

The escapes are `\"`, `\\`, `\n`, `\t`, `\r` and `\uXXXX` with four
hex digits. A wrong escape does not stop the scan. The quote still
closes the literal, and the lexer reports one error for each wrong
escape:

| Escape | Error |
| --- | --- |
| `\u` without four hex digits, `\u{41}`, or a surrogate such as `\uD800` | `InvalidUnicodeEscape` |
| A backslash before any other character, such as `\q` | `InvalidEscape` (E035) |

The decoded text holds U+FFFD in place of each wrong escape.

A bidirectional control character (U+202A to U+202E, U+2066 to
U+2069) in a comment or in a string literal gives
`BidirectionalControl` (E036). This rule stops the "Trojan Source"
attack (CVE-2021-42574). The check covers `//`, `///`, `//!` and
`/* */` comments. An escape such as `\u202E` is not the character
itself, so the lexer accepts it.

### Line breaks and statements

A statement has no terminator. The lexer keeps a line break only when
it ends a statement, and it drops all other line breaks. The code is in
`drop_continuation_newlines` in `src/lexer/mod.rs`. The lexer keeps a
line break when all of these conditions are true:

- No `(` or `[` is open. A `{` starts a new count, because its
  contents are statements again.
- The token before the line break can end a statement: a name, a
  literal, `self`, `)`, `]`, `}` or `?`.
- The next token is not `.`, `else` or `{`. These tokens can only
  continue the line above.

Inside a function body or a block, a statement must end at a line
break, at the `}` of the block, or before a `{`. So
`let a = 1 let b = 2` on one line is a parse error. The code is in
`statement_end` in `src/parser/recovery.rs`. At the top level of a
file, the parser does not apply this rule: two top-level statements on
one line parse.

In a `{ ... }` list that is not a block, a line break does not end
anything. A `,` must separate two fields of a struct and two variants
of an enum; a line break alone is not a separator. In a trait, the `,`
between two items is optional. The items of an impl block and of a
`mod` have no separator.

### The nesting limit

The parser and the phases after it are recursive. A deep program can
overflow the stack, and a stack overflow stops the whole process. So
before the parse starts, `nesting_score` in `src/parser/nesting.rs`
reads the tokens once and computes a nesting score:

- Each open `(`, `[` or `{` adds one.
- Inside a level, each operator, `.`, `?`, `->`, `=` and each `if`,
  `else`, `match`, `for` or `let` adds one more. A chain such as
  `1 + 1 + 1` builds a tree as deep as the chain is long.
- A `,` or a line break ends one item of a level. The count of that
  item goes back to zero.

The score is never lower than the depth of the tree. It can be higher.
When the score goes above `MAX_NESTING` (1024), the parser does not
start. It returns one `ParseError` at the first token above the limit:
"the program nests too deeply here; the limit is 1024 levels". The
time of the check grows linearly with the length of the input.

The semantic pass has its own limit. It refuses an expression deeper
than 500 with `ExpressionDepthExceeded`.

### The parser thread

The parser runs on its own thread, named `formalang-parser`. The code
is in `on_parser_stack` in `src/parser/nesting.rs`. The stack of the
thread is 8 MiB plus 512 KiB for each unit of the nesting score. So a
program at the limit gets a stack of about 520 MiB. The operating
system commits a stack page only when the thread uses it, so the size
costs address space, not memory. When the thread cannot start, the
parse runs on the thread of the caller. A panic on the parser thread
continues on the thread of the caller.

### Recovery

The parser reports more than one syntax error in one run:

- At the top level, a statement that fails to parse is skipped up to
  the next token that can start a statement: `use`, `let`, `pub`,
  `struct`, `enum`, `trait`, `impl`, `fn`, `extern` or `mod`.
- In a function body or a block, `skip_failed_statement` in
  `src/parser/recovery.rs` skips the failed statement up to the next
  `let` or `}` of the same block. It skips a `( ... )`, `[ ... ]` or
  `{ ... }` group as a whole, and it never consumes the `}` that
  closes the block. The failed statement becomes a `nil` expression in
  the AST. An unclosed group makes the skip go to the end of the
  input, and the block then reports its missing `}`.

Each token is skipped once, so recovery does not multiply the parse
time for each level of nesting.

### The closure guard

A closure and a tuple both start with `(`, and both can start with
`(name: ...`. The parser tries the closure form only when the matching
`)` is followed by `->`. `closure_ahead` in
`src/parser/exprs/closure_guard.rs` answers this question with one scan
over the tokens of the group. The scan records the answer for each
`(` inside the group too, and a cache keeps the answers for the
parse. So all the scans of one parse take linear time.

### After the parse

`compile_with_analyzer` and `compile_to_ir` change the AST after the
parse. `parse_only` does not. The semantic pass makes these changes:

- It makes `-` directly before a numeric literal part of the literal,
  so `-2147483648` is one `Literal`.
- It writes the type of each unsuffixed numeric literal that takes
  its type from its position as a suffix. In
  `let big: I64 = 3000000000`, the literal gets the suffix `I64`.
- It changes an `EnumInstantiation` whose name is a value, not a type,
  into a `Reference` path or a `MethodCall`. See
  [Expressions](expressions.md).
