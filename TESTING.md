# Testing

This document explains what each test surface guards, when to run it,
and where a new test belongs.

## Quick reference

| Command | Runs | Speed |
| --- | --- | --- |
| `cargo test` | unit + integration + doctests | ~4 min |
| `cargo test -- --ignored` | repros for open defects and unfinished features (none exist today) | seconds |
| `PROPTEST_CASES=N cargo test --release --test suite differential::` | generated programs against an oracle | seconds to minutes |
| `PROPTEST_CASES=N cargo test --release --test suite proptest_frontend::` | property tests with a bigger budget | seconds to minutes |
| `cargo bench` | the divan benchmark suite | ~5 min |
| `cargo bench --bench scaling` | how each phase grows with program size | ~3 min |
| `scripts/fuzz.sh [target] [seconds]` | the cargo-fuzz targets | as long as you give it |
| `RUSTFLAGS="--cfg loom" cargo test --bin fvc loom_watch` | the loom model check | seconds |
| `scripts/check_file_sizes.sh` | the 500-line ceiling on `src/**/*.rs` | instant |
| `scripts/mutate.sh --in-diff` | whether the tests would notice a wrong compiler, over what you changed | minutes |
| `scripts/mutate.sh` | the same, over the whole project | hours |

## Taxonomy

### Correctness, every commit

| File | What it guards |
| --- | --- |
| `src/**/*.rs` (`#[cfg(test)]`) | Module-local invariants |
| `tests/suite/*.rs` | One file per feature area — the parser, the semantic analyser, each IR pass |
| `tests/suite/snapshots.rs`, `tests/suite/closure_conv.rs` | `insta` snapshots of the AST and the IR |
| `tests/suite/metamorphic.rs` | Properties that hold over the whole example corpus |
| `tests/suite/proptest_frontend.rs` | Random inputs that shrink to a minimal counterexample |
| `tests/suite/concurrency.rs` | What the library promises a multi-threaded caller |
| `tests/suite/performance.rs` | How each phase scales — shape, not speed |
| `tests/suite/diagnostics_from_source.rs` | One snippet per diagnostic, and the variants no program can reach |
| `tests/suite/reporting_every_variant.rs` | Every `CompilerError` variant, rendered four ways |
| `tests/suite/editor_surface.rs` | The node finder, the position helpers and the query provider, swept over every offset |
| `tests/suite/closure_captures.rs` | Capture analysis, one case per expression form |
| `tests/suite/cli.rs` | The `fvc` binary, end to end |
| `tests/suite/cross_module.rs` | Compiling through a resolver, and the gaps that remain |
| `tests/suite/test_serde_stability.rs` | An `IrModule` survives a JSON round trip (the `serde` feature) |
| `tests/suite/run_examples.rs` | Every example's `run_checks()`, executed |
| `tests/suite/conformance.rs` + `tests/conformance/**.fv` | One rule per file, run or rejected |
| `tests/suite/type_matrix.rs` | Every context x type pair, as a snapshot |
| `tests/suite/operator_matrix.rs` | Every operator x operand pair, as a snapshot |
| `tests/suite/method_matrix.rs` | Every prelude method x receiver pair, as a snapshot |
| `tests/suite/execution_matrix.rs` | Every value x context pair, run and checked |
| `tests/suite/shape_equivalence.rs` | One computation written several ways, run and compared |
| `tests/suite/renaming.rs` | Renaming something does not change the verdict |
| `tests/suite/matrix_answers.rs` | The cells the matrices accept, run against algebraic laws |
| `tests/suite/tests_assert_something.rs` | No file gains a test that asserts nothing |
| `tests/suite/optimised_answers.rs` | Every optimising pass, run and compared against the unoptimised answer |
| `tests/suite/no_internal_errors.rs` | No wrong program earns an internal error |
| `tests/suite/differential.rs` | Generated programs against an independent oracle |
| `tests/suite/common/verifier.rs` | The IR type contract. The interpreter checks every module with it before a run |
| `tests/suite/ir_verifier.rs` | The verifier over every corpus, after each pass |
| `tests/suite/type_agreement.rs` | The semantic pass and the IR give one expression the same type |
| `tests/suite/fold_width.rs`, `tests/suite/numeric_semantics.rs` | Constant folding at the declared width, and the WebAssembly arithmetic tables |
| `tests/suite/docs_examples.rs` | Every `formalang` block in `docs/` is a complete program. It compiles, or it fails with the variant of its `reject=` tag |
| `tests/suite/near_miss.rs` | One-edit mutants of valid programs are rejected |
| `tests/suite/depth_ladder.rs` | Deep nesting compiles or returns an error, in a child process, and does not abort |
| `tests/suite/pathological_inputs.rs` | Linear time, clean spans and odd characters. See [Timing tests](#timing-tests) |
| `tests/suite/frontend_invariants.rs` | Broken forms of the corpus give no internal error |
| `tests/suite/pass_adversarial.rs`, `tests/suite/pass_contracts.rs` | The stated contract of each pass, over adversarial programs |
| `tests/suite/cross_module_adversarial.rs`, `tests/suite/mined_modules.rs` | Programs over several files, through a resolver |
| `tests/suite/api_contracts.rs` | The claims in the public API documentation |
| `tests/suite/unary_operands.rs` | A unary operator checks its operand |

All of these run in `cargo test`.

#### The `serde` feature

The IR serde is the optional `serde` feature, and it is off by
default. Many tests write the IR as JSON: a round trip, or a canonical
form to compare two modules. `Cargo.toml` has a dev-dependency on the
crate itself with the feature, so a test build always has the feature.
Plain `cargo test` runs these tests, and `cargo test --all-features`
runs the same set.

A test target always turns the feature on. To check the build without
the feature, use `cargo build --no-default-features` or
`cargo clippy --lib --bins --no-default-features`. Do not add
`--all-targets`, because a test target turns the feature on again.

#### One binary, many files

Cargo compiles each `tests/*.rs` file as its own crate, and links each
one against the whole dependency set. At eighty files that link took 40
seconds after a one-line change to `src/`, and every mutation-sweep
copy paid it again.

The files therefore live in `tests/suite/`, and `tests/suite/main.rs`
declares each one as a module. The suite links once: the same rebuild
takes 6 seconds.

What this asks of a new test file:

- put it in `tests/suite/`, and add a `mod <name>;` line to
  `tests/suite/main.rs`;
- reach the shared helpers through `crate::common::`;
- give `include_str!` a path relative to `tests/suite/`, so
  `../fixtures/complete.fv`;
- name the whole test when a test starts itself as a child process.
  The name now carries the module: `crate::common::test_name` builds
  it. See `tests/suite/metamorphic.rs`;
- run one file with a filter rather than with `--test`:
  `cargo test --test suite differential::`.

An `insta` snapshot file is named after the module path of the
assertion, so every snapshot in `tests/suite/snapshots/` carries a
`suite__` prefix.

#### The reference interpreter

`tests/suite/common/interpreter.rs` evaluates a lowered `IrModule`. It is
test-only — not shipped, not fast, and not a specification. It is a
second opinion, and it is what makes the three surfaces below possible:
each of them states what a program should *compute*, which no amount of
inspecting the IR's shape can check.

Before it existed, the twenty example programs ended in a `run_checks()`
holding 93 `assert(condition: ...)` calls that nothing ever executed.

The interpreter looks things up by name, but a backend uses the ids.
So a wrong id can give the right answer here and the wrong answer in a
backend. To stop this, `Interpreter::new` runs the IR verifier
(`tests/suite/common/verifier.rs`) over the module. When the verifier
finds a problem, `Interpreter::run` evaluates nothing and returns
`Fault::IllFormed`. Each run in each suite thus also checks the IR
type contract: each expression carries its resolved type, and each id
names something that exists. `tests/suite/ir_verifier.rs` runs the
same verifier over every corpus after each pass. After
`ResolveReferencesPass`, `verify_resolved` also checks the indices.

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

`tests/suite/type_matrix.rs` generates every (context, declared type, value
type) triple — fourteen contexts by sixteen declared types by eighteen
value types — compiles each, and snapshots the accept/reject grid.

The two extra value columns, `I32 typed` (`1I32`) and `F64 typed`
(`1.5F64`), have a fixed type. An unsuffixed literal takes its type
from the context, so the `I32` column (`1`) fits an `I64` row. The
typed columns keep the check that a value of one numeric type does
not fit a declaration of another.

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

`tests/suite/operator_matrix.rs` is the companion to the type matrix. It
generates every (operator, left operand, right operand) triple —
fourteen operators by sixteen types by sixteen types — and snapshots
the same accept/reject grid.

The grid is the diagonal almost everywhere, with three exceptions worth
knowing: `+` also joins two strings, `..` takes integers only because a
range counts in steps of one, and `==` refuses a closure because a
closure has no structure to compare.

#### The method matrix

`tests/suite/method_matrix.rs` is the third grid. The prelude declares
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

`tests/suite/execution_matrix.rs` closes it. Fourteen values by eighteen
contexts — a `let`, a struct field, a field default, a tuple field, an
array element, a dictionary value, an argument, a return, a default
parameter, two calls deep, a closure return, a closure argument, either
branch of an `if`, a match arm, a captured binding, a block, a nested
block — and each cell is **run**. No oracle is written down: a value put
into a context and read back must equal itself, and the program under
test makes the comparison.

`tests/suite/shape_equivalence.rs` does the same for syntax rather than
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

`tests/suite/optimised_answers.rs` covers them with the property that matters
for an optimiser: **run the program, then run the optimised program,
and the two answers must agree.** The unoptimised run is the
specification, so no oracle has to be written down, and which answer is
"right" never has to be decided. Each pass is applied on its own as
well as in combination, so a disagreement names one pass.

The programs are the hand-written ones aimed at a pass each, plus
**every cell of the value-by-context matrix** that
`tests/suite/execution_matrix.rs` uses — the tables live in
`tests/suite/common/matrix.rs` so both suites widen together. That is roughly
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
| `type_matrix` | 4032 | 350 | 3682 |
| `operator_matrix` | 3584 | 71 | 3513 |
| `method_matrix` | 255 | 25 | 230 |
| **total** | **7871** | **446** | **7425** |

For a rejected cell, "does this compile?" is the whole question —
there is nothing to run. For the 446 that accept there is: each
compiles to a program that produces a value, and for a long time none
of them was ever asked what that value was.

`tests/suite/matrix_answers.rs` asks. It checks laws rather than a table of
expected answers, because a law cannot be copied wrong from the
implementation — it does not mention the implementation. `a == a`,
`a <= a`, `(a + b) - b == a`, `a && a == a`, and the answer every
prelude method owes its own carrier.

#### Renaming

`tests/suite/renaming.rs` asserts that a name is not part of the meaning:
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

`tests/suite/no_internal_errors.rs` pins that shut. It pairs every value
shape with every context that consumes one — an annotation, a return,
an argument, a field read, a method call, an index, a loop, a `match`,
an `if let`, a call — and asserts that no rejection among the 1216
programs is an internal error. What the verdict is does not matter
here; other tests cover that. It may not be a bug report.

`tests/suite/conformance.rs` enforces the same rule on every case in the
corpus.

#### Differential testing

`tests/suite/differential.rs` builds a random expression tree, computes its
value in Rust, renders the same tree as `FormaLang`, compiles it, and
checks the two answers agree. It also checks that constant folding does
not change the answer.

The oracle is the point: a hand-written test only checks what someone
thought to write down.

#### Metamorphic tests

A metamorphic test changes the input in a way that must not change the
output, then checks that the output did not change. It needs no
expected value, so it covers ground that example-based tests cannot.

`tests/suite/metamorphic.rs` holds:

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

`tests/suite/proptest_frontend.rs` generates two kinds of input.

- **Token soup** — a random sequence of real grammar fragments. Fully
  random bytes almost never reach the parser's interesting paths; a
  random sequence of tokens does, and it is what breaks error
  recovery.
- **Valid programs** — small programs built to a template, used for
  properties that only make sense over source that compiles.

`PROPTEST_CASES=N` changes the case count. When proptest shrinks a
counterexample it writes a `tests/suite/*.proptest-regressions` file; commit
it alongside the fix, so every future run replays it first.

#### Timing tests

Each phase must take time that grows with the length of the input at
a linear rate. `tests/suite/pathological_inputs.rs` checks this with
the `linear!` tests. Each one times a phase over an input of size n
and over an input of size 16n:

- a linear phase takes about 16 times as long, and a quadratic phase
  about 256 times;
- the test fails when the large input takes more than 48 times as
  long as the small one, plus 40 milliseconds;
- the test runs each size 25 times, in turns of five, and keeps the
  fastest run of each size. Load from other tests then falls on both
  sizes;
- one lock lets only one timing test measure at a time.

The gap between 48 and 256 is wide. Load from other processes does not
make a linear phase fail, and a quadratic phase still fails by a wide
margin. Do not make the bound tighter to catch a smaller growth: use
`cargo bench --bench scaling` for that.

#### Deep nesting

A stack overflow does not return an error: it aborts the process. For
an editor that holds the compiler, that is a crash of the editor. So
`tests/suite/depth_ladder.rs` runs each probe in a child process, on a
thread with a stack of 8 megabytes. It nests each construct at depths
of 64 and 256, which must compile, and at 1024, 10 000 and 100 000,
which must return an error. No probe may abort or hang.

Two limits make this hold:

- the parser computes a nesting score from the tokens before it
  parses. Above a score of 1024 it returns a `ParseError`. Below it,
  the parser runs on a thread of its own, with a stack that grows with
  the score (`src/parser/nesting.rs`);
- the semantic pass refuses an expression deeper than 500 with
  `ExpressionDepthExceeded`.

### Concurrency

Two calls of the compiler share only the prelude, which the library
parses and lowers once per process, in a `OnceLock`. One call uses one
thread, with one exception: the parser runs on a thread of its own,
with a stack that fits the nesting of the program, and the call waits
for it. The callers of the compiler use many threads: a build tool
compiles many files at once, and an editor runs the analyser on a
worker.

`tests/suite/concurrency.rs` checks the three things that follow from that —
the public types are `Send` and `Sync`, compiling on eight threads
gives what compiling on one gives, and rendering a diagnostic on eight
threads produces a report on each.

`src/bin/fvc.rs` holds the only atomics in the tree: the two flags
that `fvc watch` shares with its Ctrl+C handler, the shutdown request
and the result of the last check. `WatchState`
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
| `parse` | `&str` | The parser never panics; a failure always carries an error |
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
rather than a hang. Each phase must take time that grows with the
length of the input, so a timeout is a defect. The parser was
exponential in nesting depth once (FL-1, and later unclosed `if`
blocks), and the fuzzer found both as timeouts. `depth_ladder` and
the `linear!` tests in `pathological_inputs` now guard them.

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

An `#[ignore]` test fails today, and its reason says why. No such
test exists today: every test in the tree passes. This command runs
them when they exist:

```sh
cargo test --no-fail-fast -- --ignored
```

When you find a defect that you do not fix now, add a repro: a test
that fails, with `#[ignore]` and a reason that names the cause and the
file that holds it. Put it in the file of its feature area. Remove the
attribute when the defect is fixed or the feature lands, and the test
becomes its regression guard.

## Adding a new test

- **Module-local invariant?** Add `#[cfg(test)] mod tests` in the
  source file.
- **A feature that crosses phases?** Add or extend a file in
  `tests/suite/`, named after what it guards, and add its `mod` line to
  `tests/suite/main.rs`.
- **A property that must hold over any program?** Add it to
  `tests/suite/metamorphic.rs` if it holds over the example corpus, or to
  `tests/suite/proptest_frontend.rs` if it needs generated input.
- **An input that must never crash the compiler?** Add a fuzz target
  in `fuzz/fuzz_targets/`, and a seed in `fuzz/seeds/text/` if the target
  takes source text.
- **An atomic or lock-free protocol?** Add a loom model next to the
  code it checks.
- **A question about how something scales?** Add a generator to
  `benches/scaling.rs`.
- **A user-facing diagnostic?** Add a row to
  `tests/suite/diagnostics_from_source.rs` pairing a snippet with the
  variant it must produce.
- **Anything an editor calls?** Add it to `tests/suite/editor_surface.rs`,
  and sweep every offset rather than probing a few.
- **A language rule?** Add a file to `tests/conformance/`. That is
  the cheapest surface and the one to reach for first.
- **A defect you are not fixing now?** Add a repro with `#[ignore]` to
  the file of its feature area, and name the cause.
- **A claim in the documentation?** Write the example as a complete
  ```` ```formalang ```` block; `docs_examples` compiles it. Put a claim
  about what a program means in a `d_` conformance case.

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

The suite calls `Checked::new` 68 times today, in 22 files. Any new
corpus-driven test should use it.

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

`tests/suite/tests_assert_something.rs` reads the test sources and counts the
test functions that compile a program and then ask nothing of the
result. There are 610 of them across 26 files, and the count per file
is a ratchet: a new one has to displace an old one or assert
something.

Those 610 are not worthless — on a valid program, "this compiles" is
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
scripts/mutate.sh --in-diff        # only the code you changed
scripts/mutate.sh                  # the whole project
scripts/mutate.sh src/semantic     # a directory or a file
```

`--in-diff` is the everyday command. It writes `git diff` against
`origin/main` — your commits and your working tree — and keeps only the
mutants that fall on a line the diff touches. A sweep of the whole
project tests 3564 mutants, nearly all of them unchanged since the last
sweep; a sweep of one change tests tens. Give it another base as an
argument: `scripts/mutate.sh --in-diff HEAD~3`. A file that git does
not track yet is not in the diff, so `git add -N <file>` first.

Three more things make a sweep shorter. The script sets them for you:

- the suite is one test binary, so each of the hundreds of builds links
  once rather than eighty times;
- four concurrent jobs, not twelve. Each job runs its own `cargo
  build`, which already uses every core;
- `sccache`, when it is installed, so the working copies share one
  build of the dependency tree instead of building it again each.

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

The whole tree is 3564 mutants and each needs a build and a test run,
so a full pass takes hours. Run `--in-diff`, or a directory at a time.

## What is left

A bug hunt on 2026-09-24 added the files at the end of the taxonomy
table, and conformance cases that failed. The defects that these tests
found are fixed. Every test passes today, and so do all 1290
conformance cases. No test carries `#[ignore]`, and `KNOWN_WRONG` in
`type_matrix` is empty.

These things are not done:

- **The mutation sweep.** `scripts/mutate.sh` ran over one file only,
  and the fixes added code: the tree now holds 3564 mutants, 813 of
  them in `src/semantic/validation` alone. Run `--in-diff` for each
  change. Run the full sweep one directory at a time.
- **The fuzzers after the fixes.** The parser and the semantic pass
  changed a lot. Run `scripts/fuzz.sh` for each target, and add each
  finding as a test and as a seed.
- **The bigger budgets.** `differential` and `proptest_frontend` run
  few cases in `cargo test`. Run them with a large `PROPTEST_CASES` in
  a release build from time to time.
