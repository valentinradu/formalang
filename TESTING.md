# Testing

This document explains what each test surface guards, when to run it,
and where a new test belongs.

## Quick reference

| Command | Runs | Speed |
| --- | --- | --- |
| `cargo test` | unit + integration + doctests | ~1 min |
| `cargo test -- --ignored` | repros for open defects and unfinished features | seconds |
| `PROPTEST_CASES=N cargo test --release --test differential` | generated programs against an oracle | seconds to minutes |
| `PROPTEST_CASES=N cargo test --release --test proptest_frontend` | property tests with a bigger budget | seconds to minutes |
| `cargo bench` | the divan benchmark suite | ~5 min |
| `cargo bench --bench scaling` | how each phase grows with program size | ~3 min |
| `scripts/fuzz.sh [target] [seconds]` | the cargo-fuzz targets | as long as you give it |
| `RUSTFLAGS="--cfg loom" cargo test --bin fvc loom_watch` | the loom model check | seconds |
| `scripts/check_file_sizes.sh` | the 500-line ceiling on `src/**/*.rs` | instant |
| `scripts/mutate.sh` | whether the tests would notice a wrong compiler | hours | instant |

## Taxonomy

### Correctness, every commit

| File | What it guards |
| --- | --- |
| `src/**/*.rs` (`#[cfg(test)]`) | Module-local invariants |
| `tests/*.rs` | One file per feature area — the parser, the semantic analyser, each IR pass |
| `tests/snapshots.rs`, `tests/closure_conv.rs` | `insta` snapshots of the AST and the IR |
| `tests/metamorphic.rs` | Properties that hold over the whole example corpus |
| `tests/proptest_frontend.rs` | Random inputs that shrink to a minimal counterexample |
| `tests/concurrency.rs` | What the library promises a multi-threaded caller |
| `tests/performance.rs` | How each phase scales — shape, not speed |
| `tests/diagnostics_from_source.rs` | One snippet per diagnostic, and the variants no program can reach |
| `tests/reporting_every_variant.rs` | Every `CompilerError` variant, rendered four ways |
| `tests/editor_surface.rs` | The node finder, the position helpers and the query provider, swept over every offset |
| `tests/closure_captures.rs` | Capture analysis, one case per expression form |
| `tests/cli.rs` | The `fvc` binary, end to end |
| `tests/cross_module.rs` | Compiling through a resolver, and the gaps that remain |
| `tests/ast_serde_depth.rs` | How deep an AST can nest and still be read back |
| `tests/run_examples.rs` | Every example's `run_checks()`, executed |
| `tests/conformance.rs` + `tests/conformance/**.fv` | One rule per file, run or rejected |
| `tests/type_matrix.rs` | Every context x type pair, as a snapshot |
| `tests/operator_matrix.rs` | Every operator x operand pair, as a snapshot |
| `tests/method_matrix.rs` | Every prelude method x receiver pair, as a snapshot |
| `tests/execution_matrix.rs` | Every value x context pair, run and checked |
| `tests/shape_equivalence.rs` | One computation written several ways, run and compared |
| `tests/renaming.rs` | Renaming something does not change the verdict |
| `tests/matrix_answers.rs` | The cells the matrices accept, run against algebraic laws |
| `tests/tests_assert_something.rs` | No file gains a test that asserts nothing |
| `tests/optimised_answers.rs` | Every optimising pass, run and compared against the unoptimised answer |
| `tests/no_internal_errors.rs` | No wrong program earns an internal error |
| `tests/differential.rs` | Generated programs against an independent oracle |
| `tests/known_issues.rs` | One `#[ignore]` repro per open defect |

All of these run in `cargo test`.

#### The reference interpreter

`tests/common/interpreter.rs` evaluates a lowered `IrModule`. It is
test-only — not shipped, not fast, and not a specification. It is a
second opinion, and it is what makes the three surfaces below possible:
each of them states what a program should *compute*, which no amount of
inspecting the IR's shape can check.

Before it existed, the twenty example programs ended in a `run_checks()`
holding 93 `assert(condition: ...)` calls that nothing ever executed.

#### The conformance corpus

`tests/conformance/` holds one `FormaLang` program per rule, grouped by
feature. Each states what should happen in its first comment:

```text
// expect: run                 compile, then run run_checks()
// expect: compile             compile, and stop there
// expect: reject TypeMismatch fail to compile, reporting that variant
```

Adding a case is adding a file — no Rust required. The corpus is meant
to grow to thousands. Its taxonomy follows the way other languages
organise theirs, and the matrices are translated from their suites: the
ownership cases follow Rust's move and borrow tests, the optional cases
follow Swift's, the generics cases follow `TypeScript`'s
`conformance/types/typeParameters`, the numeric width cases follow Go's
conversion tests, and the visibility cases follow Rust's privacy tests.

A rejection may not be an internal compiler error. The runner enforces
that on every case; see [No wrong program earns an internal
error](#no-wrong-program-earns-an-internal-error).

#### Mining another language's suite

The taxonomies above were not recalled; they were read. Shallow clones
of `rust-lang/rust` (`tests/ui`), `microsoft/TypeScript`
(`tests/cases/conformance`), `golang/go` (`test`) and `swiftlang/swift`
(`test`) give roughly 14 000 real test files. Each suite marks the
error a case expects — `//~ ERROR` in Rust, `expected-error {{...}}` in
Swift, `// ERROR` in Go — so the set of rules a language enforces can
be extracted and counted:

```bash
scripts/mine_suites.sh            # clone at pinned revisions, print the rankings
scripts/mine_suites.sh --clean    # delete the clones
```

The revisions are pinned in the script so a re-run gives the same
ranking. The clones live outside the repository, under the system
temporary directory: they are other languages' source code, they do not
run against `FormaLang`, and each carries its own licence.

Rank by frequency, drop what does not apply, and each remaining line is
a rule to check `FormaLang` against. That ranking is the value: it
surfaces what a mature compiler spends its checks on, which is not the
same as what seems worth testing from memory. "Member cannot be used on
a value of type T; consider a generic constraint" appears 175 times in
Swift's suite, "cannot be declared public" 68 times, and "missing
return" is the single most common mark in Go's — none would have been
an obvious thing to try, and each turned out to be a real hole.

#### The type matrix

`tests/type_matrix.rs` generates every (context, declared type, value
type) triple — fourteen contexts by sixteen types by sixteen types —
compiles each, and snapshots the accept/reject grid.

The snapshot is not an oracle; it records what the compiler does today,
so any change to the type checker arrives as a reviewable diff over the
whole grid. On top of it, two tests assert the cells nobody needs to
think about: a type always satisfies itself, and the pairs in
`NEVER_COMPATIBLE` never do. `let x: I32 = ""` is one row of the
second.

Cells whose recorded verdict is wrong live in `KNOWN_WRONG` with what
they should say, and a test fails when one starts behaving correctly so
the entry comes out.

#### The operator matrix

`tests/operator_matrix.rs` is the companion to the type matrix. It
generates every (operator, left operand, right operand) triple —
fourteen operators by sixteen types by sixteen types — and snapshots
the same accept/reject grid.

The grid is the diagonal almost everywhere, with three exceptions worth
knowing: `+` also joins two strings, `..` takes integers only because a
range counts in steps of one, and `==` refuses a closure because a
closure has no structure to compare.

#### The method matrix

`tests/method_matrix.rs` is the third grid. The prelude declares
twenty methods across six carriers — `Optional`, `Array`, `Seq`,
`Dictionary`, `Range` and `String` — and each method belongs to exactly
one of them, so most cells are rejections. Each cell also gets a call
with the wrong argument count and one with a wrong argument type.

The axis was worth adding: it found that a method call on any generic
receiver skipped both checks, because the receiver was matched by its
rendered name (`Seq<I32>`) against impl blocks declared on the bare
name. `s.collect(1, 2, 3)` compiled.

#### Running the generated programs

The matrices above generate programs and ask the compiler for a
verdict. That leaves a whole class of defect invisible: one that lets a
program compile and then compute the wrong answer.

`tests/execution_matrix.rs` closes it. Fourteen values by eighteen
contexts — a `let`, a struct field, a field default, a tuple field, an
array element, a dictionary value, an argument, a return, a default
parameter, two calls deep, a closure return, a closure argument, either
branch of an `if`, a match arm, a captured binding, a block, a nested
block — and each cell is **run**. No oracle is written down: a value put
into a context and read back must equal itself, and the program under
test makes the comparison.

`tests/shape_equivalence.rs` does the same for syntax rather than
types. One computation is written six ways — a plain body, a closure in
a call argument, a nested call, a `let`-bound block, an
immediately-called closure, behind a second function — all six are run,
and the answers must agree. The shapes are each other's oracle.

Both were built after a defect that compiled cleanly and answered 9
where it should have answered -1, and both were checked by reverting
that fix and watching them fail.

#### The optimising passes

`compile_to_ir` lowers and stops. Monomorphisation, reference
resolution, closure conversion, dead-code elimination, constant folding
and defunctionalisation all run in a pipeline that, for a long time, no
test called — six passes that rewrite the IR, with no behavioural
coverage at all.

`tests/optimised_answers.rs` covers them with the property that matters
for an optimiser: **run the program, then run the optimised program,
and the two answers must agree.** The unoptimised run is the
specification, so no oracle has to be written down, and which answer is
"right" never has to be decided. Each pass is applied on its own as
well as in combination, so a disagreement names one pass.

The programs are the hand-written ones aimed at a pass each, plus
**every cell of the value-by-context matrix** that
`tests/execution_matrix.rs` uses — the tables live in
`tests/common/matrix.rs` so both suites widen together. That is roughly
270 programs through 8 pipeline configurations: about 2160 optimised
runs, each compared against its own unoptimised answer. A pass rewrites
whole shapes, and a handful of hand-written programs cannot reach them
all; the matrix already enumerates the shapes, so the passes now get
the same surface the lowerer does.

It found two real defects. The first came from its first generic
program. Monomorphisation
rewrites a call's path to the specialised function but left its
`function_id` pointing at the old index, which compaction had since
given to another function — `probe` itself. Nothing downstream repaired
it, so a backend dispatching on the id (which is what the field is for)
emitted a function calling itself.

The second came from feeding the matrix through
`DefunctionalisePass`. That pass collected closure shapes only from
closure *values*, so a function declaring a callback that nothing in
the module supplies — `fn apply(f: (I32) -> I32)` with no caller — had
a call site with no enum to dispatch through. The pass then failed its
own stated post-condition, "no indirect call remains", which is the
whole security argument in its documentation. Shapes are now collected
from call sites as well; an uninhabited shape gets an enum with no
variants, which is the right answer for a callback nobody supplies.

Running a converted module needs the reference interpreter to
understand the shapes the passes produce. It now evaluates
`IrExpr::ClosureRef` by calling the lifted function with the
environment as its first argument, which is the convention
`ClosureConversionPass` documents on the parameter it generates, and it
calls a defunctionalised closure through the `__call_Fn<K>` dispatcher
that `DefunctionalisePass` generates for the tag.

#### What the matrices actually ask

Counting the three snapshot matrices:

| | cells | accepted | rejected |
| --- | --- | --- | --- |
| `type_matrix` | 3586 | 277 | 3309 |
| `operator_matrix` | 3586 | 72 | 3514 |
| `method_matrix` | 257 | 26 | 231 |
| **total** | **7429** | **375** | **7054** |

For a rejected cell, "does this compile?" is the whole question —
there is nothing to run. For the 375 that accept there is: each
compiles to a program that produces a value, and for a long time none
of them was ever asked what that value was.

`tests/matrix_answers.rs` asks. It checks laws rather than a table of
expected answers, because a law cannot be copied wrong from the
implementation — it does not mention the implementation. `a == a`,
`a <= a`, `(a + b) - b == a`, `a && a == a`, and the answer every
prelude method owes its own carrier.

#### Renaming

`tests/renaming.rs` asserts that a name is not part of the meaning:
renaming a generic parameter, or a binding, must not change the
verdict. Eight shapes by eleven adversarial names, each a substring of
a type the language ships — `S` of `String`, `I` of `I32`, `A` of
`Array`.

It exists because three separate checks tested "does this declared type
mention a generic parameter?" with a substring match, so `String`
mentioned a parameter called `S` and the check was skipped. Every
generic test in the corpus used `T`, which appears in no type name.

#### No wrong program earns an internal error

An internal error tells the user that the compiler broke and asks them
to file a bug. That answer is right only when the compiler really did
break. For a mistake in the user's own program it is the worst answer
available: it blames the tool and gives the user nothing to act on.

Several passes assume the semantic pass checks a shape before IR
lowering reaches it. The assumption is written into the source — the
words "semantic should have caught this" appear in the lowering code —
and wherever the semantic check was missing, ordinary wrong programs
came back as bug reports. One sweep found 107 of them.

`tests/no_internal_errors.rs` pins that shut. It pairs every value
shape with every context that consumes one — an annotation, a return,
an argument, a field read, a method call, an index, a loop, a `match`,
an `if let`, a call — and asserts that no rejection among the 1216
programs is an internal error. What the verdict is does not matter
here; other tests cover that. It may not be a bug report.

`tests/conformance.rs` enforces the same rule on every case in the
corpus.

#### Differential testing

`tests/differential.rs` builds a random expression tree, computes its
value in Rust, renders the same tree as `FormaLang`, compiles it, and
checks the two answers agree. It also checks that constant folding does
not change the answer.

The oracle is the point: a hand-written test only checks what someone
thought to write down.

#### Metamorphic tests

A metamorphic test changes the input in a way that must not change the
output, then checks that the output did not change. It needs no
expected value, so it covers ground that example-based tests cannot.

`tests/metamorphic.rs` holds:

- **Determinism** — two compiles of one source, in one process and in
  two, must produce byte-identical IR.
- **Idempotence** — the codegen pipeline, and each pass on its own,
  run twice over their own output must change nothing.
- **Source transformations** — adding comments or trailing whitespace
  must not change the IR beyond its spans and its ids.
- **Structural invariants** — every id in a lowered module points at
  something that exists; every span names a file in the file table;
  dead-code elimination never removes a public definition.

The comparisons run over a canonical view of the module: spans and
numeric ids removed, top-level vectors sorted by name. That makes the
test independent of the order the lowerer happened to register
definitions in. The lowerer hands out ids while walking `HashMap`s,
so this used to differ between two runs of the same binary; sorting
every such walk fixed it, and
`compile_is_deterministic_across_processes` guards it.

#### Property tests

`tests/proptest_frontend.rs` generates two kinds of input.

- **Token soup** — a random sequence of real grammar fragments. Fully
  random bytes almost never reach the parser's interesting paths; a
  random sequence of tokens does, and it is what breaks error
  recovery.
- **Valid programs** — small programs built to a template, used for
  properties that only make sense over source that compiles.

`PROPTEST_CASES=N` changes the case count. When proptest shrinks a
counterexample it writes a `tests/*.proptest-regressions` file; commit
it alongside the fix, so every future run replays it first.

### Concurrency

The compiler is single-threaded. Its callers are not: a build tool
compiles many files at once, and an editor runs the analyser on a
worker.

`tests/concurrency.rs` checks the three things that follow from that —
the public types are `Send` and `Sync`, compiling on eight threads
gives what compiling on one gives, and rendering a diagnostic on eight
threads produces a report on each.

`src/bin/fvc.rs` holds the only atomic state in the tree: the shutdown
flag that `fvc watch` shares with its Ctrl+C handler. `WatchState`
exists as its own type so that protocol can be model-checked without
the filesystem polling around it. Run the model with:

```sh
RUSTFLAGS="--cfg loom" cargo test --bin fvc loom_watch
```

Under `cfg(loom)` the atomics come from `loom`, which explores every
interleaving the memory model allows instead of the one this machine
happens to produce. Keep the model small: preemptions blow up
combinatorially.

### Fuzzing

`fuzz/` is a `cargo-fuzz` crate, excluded from the parent package's
`include` list so it never ships to crates.io. It needs a nightly
toolchain and `cargo install cargo-fuzz --locked`.

| Target | Input | What it guards |
| --- | --- | --- |
| `lex` | `&str` | The lexer never panics; every span it reports is a real byte range with one-based positions |
| `parse` | `&str` | The parser never panics; a failure always carries an error; a parsed AST round-trips through JSON |
| `compile` | `&str` | Lex, parse, semantic analysis and lowering over text no grammar generator would write |
| `program` | `Arbitrary` program | The deep phases. Renders a generated program to source, then checks IR JSON stability and pipeline idempotence |
| `ir_json` | `&[u8]` | The IR decode surface. A backend reading an `IrModule` another tool wrote is reading untrusted input |
| `report` | `(&str, &str)` | The diagnostic renderer, driven with spans that do not match the source it renders against |

`program` is the one that reaches the semantic analyser and the IR
lowerer. Raw bytes rarely parse; `fuzz/src/lib.rs` builds a program
out of a small fixed pool of names, so references resolve often and
the generated program compiles.

Run one target, or all of them:

```sh
scripts/fuzz.sh              # every target, 60 s each
scripts/fuzz.sh program 600  # one target, 600 s
```

The script passes `-timeout=10`, so a slow input is a reported finding
rather than a hang. That matters here: parse time is exponential in
nesting depth (FL-1), so a timeout is a defect.

The live corpus under `fuzz/corpus/` is not checked in. The curated
seeds under `fuzz/seeds/text/` are, and the script passes `examples/` as an
extra corpus directory, so a fresh clone starts from real source.

### Benchmarks

`benches/` uses `divan`. Chosen over criterion because CI runs on a
small disk: divan pulls three transitive crates, criterion pulls about
forty.

- `cargo bench --bench frontend` — the cost of each phase over the
  example programs, plus each IR pass on an already-lowered module,
  plus IR JSON encode and decode. `compile_minimal` and
  `prelude_parse` bound the fixed cost every caller pays.
- `cargo bench --bench scaling` — one generator per dimension of a
  program: structs, functions, match arms, expression length, local
  bindings, closures, generic instantiations, nesting depth. Each
  size is 4x the one before it.

Read the scaling results as ratios. When the input grows 4x, a linear
phase grows about 4x. A phase that grows 16x is quadratic in that
dimension, and that is the finding. `lex_scaling` is the clearest
detector: the lexer touches every byte once, so anything but a 4x step
is a defect.

### Open defects and unfinished features

Every `#[ignore]` test fails today, and its reason says why.

```sh
cargo test --no-fail-fast -- --ignored
```

They sit in two places:

- `tests/known_issues.rs` — open defects, one repro each, with the
  file that holds the cause.
- `tests/cross_module.rs` — shapes the cross-module inline pass has
  not reached: an imported type in a function signature, an imported
  return type, a generic import, and an imported trait's conformance.
  Each reports a `CompilerError::InternalError` rather than emitting a
  module a backend cannot use.

Drop the attribute when the defect is fixed or the feature lands, and
the test becomes its regression guard.

When you find a defect you are not fixing now, add a repro the same
way: a test that fails, `#[ignore]`d, with a reason that names the
cause and the file that holds it.

## Adding a new test

- **Module-local invariant?** Add `#[cfg(test)] mod tests` in the
  source file.
- **A feature that crosses phases?** Add or extend a file in `tests/`,
  named after what it guards.
- **A property that must hold over any program?** Add it to
  `tests/metamorphic.rs` if it holds over the example corpus, or to
  `tests/proptest_frontend.rs` if it needs generated input.
- **An input that must never crash the compiler?** Add a fuzz target
  in `fuzz/fuzz_targets/`, and a seed in `fuzz/seeds/text/` if the target
  takes source text.
- **An atomic or lock-free protocol?** Add a loom model next to the
  code it checks.
- **A question about how something scales?** Add a generator to
  `benches/scaling.rs`.
- **A user-facing diagnostic?** Add a row to
  `tests/diagnostics_from_source.rs` pairing a snippet with the
  variant it must produce.
- **Anything an editor calls?** Add it to `tests/editor_surface.rs`,
  and sweep every offset rather than probing a few.
- **A language rule?** Add a file to `tests/conformance/`. That is
  the cheapest surface and the one to reach for first.
- **A defect you are not fixing now?** Add a repro to
  `tests/known_issues.rs` with `#[ignore]`, and name the cause.

## Assertions must actually run

Many tests here walk a corpus and skip whatever does not apply. That
is the right shape, but it means the test passes when the corpus is
empty, when the path is wrong, or when every item started failing for
an unrelated reason — the assertions quietly stop protecting anything.

`common::Checked` guards against that. Call `hit()` each time an
assertion really ran, and the floor is enforced when the guard drops:

```rust
let mut checked = Checked::new("examples compiled", 20);
for (name, source) in examples() {
    let Ok(module) = compile_to_ir(&source) else { continue };
    assert!(...);
    checked.hit();
}
```

Twenty-one loops use it today. Any new corpus-driven test should.

## Are the tests actually testing anything?

A passing suite is not evidence on its own. This one has been wrong
about itself three times:

- `test_overload_in_impl_block` declared two methods of one name,
  compiled them, and asserted nothing. It passed for as long as method
  overloading was impossible — the feature it covered could not be
  performed at all.
- `test_if_without_else_branch` asserted the opposite of what
  `docs/user/control-flow.md` says.
- `test_inferred_enum_in_let` asserted that a hole was correct.

Coverage counts all three as covered, because the lines did execute.
Two checks answer the question properly.

### The cheap one: does the test look at what it compiled?

`tests/tests_assert_something.rs` reads the test sources and counts the
test functions that compile a program and then ask nothing of the
result. There are 615 of them across 26 files, and the count per file
is a ratchet: a new one has to displace an old one or assert
something.

Those 615 are not worthless — on a valid program, "this compiles" is
the assertion that it is not falsely rejected, and the parser suites
rest almost entirely on it. But such a test cannot tell a working
feature from a missing one, which is exactly how the first example
above survived.

It is a text search, so it sees shape and not meaning. It runs in
milliseconds, every time.

### The thorough one: would the tests notice if the compiler were wrong?

`scripts/mutate.sh` changes the compiler so it is wrong and checks
whether any test fails. A change nothing notices marks a line the
suite does not really cover.

```bash
scripts/mutate.sh                  # the modules most worth checking
scripts/mutate.sh src/semantic     # a directory or a file
```

This works. Run by hand during the session that wrote these tests, it
showed that:

- changing `Sub` to `Add` in the **lowerer** broke eight laws in
  `matrix_answers`, and the same change in the **constant folder**
  broke nothing — which is how the gap in pass coverage was found;
- reverting the `function_id` repair produced a precise failure naming
  the program and the pass;
- three mutants survived in `ir::overload`, because every overload
  test until then differed by arity, so the label comparison was never
  what decided. `methods/overloads_differ_by_label.fv` and its two
  neighbours exist because of those three.

The whole tree is about 2600 mutants and each needs a test run, so a
full pass takes hours. Run a directory at a time.

## What is left

Nothing is parked: no `#[ignore]`, no `KNOWN_WRONG` cell, no `TODO`.
The two behaviours this section used to list as undecided are decided
and implemented — a value wraps into an optional inside a container,
and an optional compares to `nil`. See
[Types / Optional Elements](docs/user/types.md) and
[Expressions](docs/user/expressions.md).

What is unfinished is the mutation sweep. `scripts/mutate.sh` has been
run over one file — 20 of about 2600 mutants — and it found a real gap
there within minutes: `ir::overload::defaults_fired` decides which
overload a call means, and nothing noticed when it always answered
zero. On that evidence a full pass will find more, and
`src/semantic/validation` alone holds 191 mutants. It is an overnight
job rather than an interactive one.

