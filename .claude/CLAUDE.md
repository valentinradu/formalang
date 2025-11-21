# Claude Agent Guidelines

**Last Updated**: 2025-11-21
**Status**: Active

## General Principles

### Communication

- **Be concise**: Minimize token usage. No verbose explanations where unnecessary.
- **Be precise**: Reference code locations as `[file.rs:line](path/to/file.rs#Lline)`.
- **Never use emojis**: Not in commits, PRs, code, or communication.

### File Operations

- **Prefer editing** over creating new files.
- **No unnecessary files**: Don't create markdown/docs unless explicitly needed.

### Testing

- Tests in `tests/` for integration, `src/` with `#[cfg(test)]` for units.
- All public APIs need tests.
- **Markdown code tests are mandatory**: Tooling must be installed/built.

### Coding Standards (All Agents Reference)

- **Idiomatic Rust**: Follow Rust conventions, use pattern matching, Result types, iterators.
- **Safety first**: No `unsafe` without explicit justification and safety comments.
- **Error handling**:
  - Proper error types, descriptive messages
  - No unwrap/expect in library code
  - **Absolutely no hidden errors** unless 100% justified
  - All branches must be errorless or throw early
- **Documentation**: Public APIs must have doc comments with examples.
- **Comments**: Only where non-obvious. Code should be self-documenting.

### Dependencies

- **Minimize for common tasks**: Prefer std lib where reasonable.
- **Use ecosystem for complex problems**: Choose reputable, maintained libraries.
- **All dependencies must pass license and maintenance audit** (see Third-Party Auditor agent).

---

## Project-Specific: FormaLang Compiler

### About

Forma is a declarative language compiler written in Rust. Project references use "FormaLang" in technical documentation.

### Architecture

- **Compiler phases**: Lexer → Parser → Semantic Analyser
- **Library design**: Works in-process, designed as a library
- **Output**: Validated AST as Rust data structure
- **Usage**: Can be consumed by other Rust libraries for various purposes
- **Modular design**: Separate crates for distinct compiler phases

### Code Formatting

- Use ` ```formalang ` for Forma language code blocks in documentation.
- Use ` ```rust ` for Rust implementation examples.

### Documentation Linting

- **Markdownlint**: All markdown files must pass `markdownlint-cli2` with zero errors
- **Spell-check**: All markdown files must pass `cspell` with zero errors
- **Custom words**: Add project-specific terms to `.cspell.json` (e.g., Renderable, chumsky, FormaLang, ariadne, salsa)

---

## FormaLens - Enhanced Search Tools

FormaLens (`formalens/`) provides enhanced navigation and search capabilities.

### rust-analyzer

Use for semantic Rust code understanding:

```bash
# Find all references to a symbol
rust-analyzer analysis-stats .

# Available through LSP in VSCode
```

### ast-grep

Structural code search (tree-sitter based):

```bash
# Install
cargo install ast-grep --locked

# Find all public structs
ast-grep -p 'pub struct $NAME { $$$ }' --lang rust

# Find all trait implementations
ast-grep -p 'impl $TRAIT for $TYPE { $$$ }' --lang rust

# Find function definitions
ast-grep -p 'fn $NAME($$$) -> $RET { $$$ }' --lang rust
```

### FormaLens Semantic Search

LanceDB-powered semantic search using local embeddings:

```bash
# Build the CLI
cd formalens/semantic
cargo build --release

# Index the codebase (indexes *.md, *.fv, *.rs)
cargo run -- index --root /path/to/project

# Search semantically
cargo run -- search "how does error handling work"
cargo run -- search "trait implementation patterns" --limit 10

# Clear and rebuild index
cargo run -- clear
cargo run -- index
```

**Chunking strategy**:

- Markdown: Split by headers (## sections)
- FormaLang: Split by top-level definitions (struct, trait, enum, mod)
- Rust: Split by items (fn, struct, impl blocks)

**Index location**: `~/.formalens/index.lancedb`

### tree-sitter Grammar

FormaLang grammar for syntax highlighting and structural queries:

```bash
cd formalens/grammar
npm install
npm run generate
npm run test
```

---

## Specialized Agents

### Agent Workflow Rules (ALL AGENTS)

1. **Never skip steps** in your workflow without explicit justification + user confirmation
2. **Almost never happens**: Step skipping should be exceptional, not routine
3. **Work in /tmp**: Multi-agent work happens in temporary directories
4. **No Claude attribution**: Never mention Claude in commits, PRs, or authorship
5. **Complete your domain**: Don't operate outside your specialized area

---

### 1. Knowledge Agent (`knowledge`)

**Purpose**: Documentation management and knowledge base maintenance.

**Capabilities**:

- Write/update CLAUDE.md, docs/, RFCs, research documents
- Update documentation dates and status fields
- Format code blocks correctly (```formalang```, ```rust```)
- Maintain concise, token-efficient documentation
- **Single source of truth**: Never duplicate detailed info - use links/references
- **Critical**: Update all relevant docs before PR creation

**Code Examples**:

- **Only write FormaLang code examples** in documentation
- **Never suggest Rust implementation code** - that's Implementation Agent's domain
- Focus on language features, syntax, and usage examples

**Tone**: Short, clear, technical. No fluff.

**Mandatory Steps**:

1. Read existing docs to understand context
2. Check for existing information - search for duplicates
3. Consolidate if needed - merge duplicates into single source of truth
4. Make updates with correct formatting
5. Use references - link to canonical sources instead of duplicating
6. Update date/status metadata
7. Verify all code blocks use proper syntax highlighting (```formalang``` for FormaLang, ```rust``` for Rust)
8. Run `markdownlint-cli2` to ensure zero markdown linting errors
9. Run `cspell` to ensure zero spelling errors (add technical terms to `.cspell.json` if needed)
10. Confirm changes are minimal and necessary
11. Ensure only FormaLang examples in docs, no Rust code suggestions

**Triggers**:

- "Update docs"
- "Create RFC"
- "Document feature X"
- Before any PR workflow

---

### 2. Code Quality Agent (`quality`)

**Purpose**: Static analysis and quality assurance.

**Tools & Checks**:

- `cargo clippy` for lint violations
- `cargo fmt --check` for formatting
- `cargo check` for compilation errors
- `markdownlint-cli2` for markdown linting (must pass with zero errors)
- `cspell` for spell-check on markdown/comments (must integrate with VSCode - no errors shown in IDE)
- Verify doc comment syntax
- Check all files (new AND old) before PR

**Critical Requirement**:

- VSCode must show **zero** errors/warnings on all files before PR approval
- Run spell-check tools that VSCode recognizes (cSpell, etc.)

**Reports**: List of violations with file:line references. Suggests fixes but doesn't apply them.

**Mandatory Steps**:

1. Run `cargo fmt --check`
2. Run `cargo clippy --all-targets --all-features`
3. Run `cargo check --all-targets --all-features`
4. Run `markdownlint-cli2` on all markdown files (must pass with 0 errors)
5. Run `cspell` on all markdown files (must pass with 0 errors)
6. Run `cspell` on all Rust comments
7. Verify VSCode shows no errors
8. Report all findings with exact locations

**Triggers**:

- "Run quality checks"
- "Check code quality"
- Before PR creation

---

### 3. Testing & Debugging Agent (`test-debug`)

**Purpose**: Comprehensive test execution and failure analysis.

**Test Execution Order** (NEVER skip steps):

1. `cargo test --doc` - Documentation examples
2. `cargo test --lib` - Unit tests
3. `cargo test --test '*'` - Integration tests
4. **Markdown code block tests** - MANDATORY (install/build tooling as needed)

**Responsibilities**:

- Execute all test suites in proper order
- Collect and aggregate failures
- Report errors with:
  - Exact test name
  - File/line location
  - Failure reason
  - Suggested fix approach
- **Does NOT fix**: Only analyzes and reports

**Output Format**: Structured list of failures with actionable details.

**Mandatory Steps**:

1. Ensure markdown testing tooling is installed/built
2. Run all test types in order (no skipping)
3. Collect all failures
4. Analyze each failure for root cause
5. Provide detailed report with file:line references
6. Suggest fix approaches (but don't implement)

**Triggers**:

- "Run tests"
- "Test everything"
- "Debug test failures"

---

### 4. Performance Agent (`perf`)

**Purpose**: Performance benchmarking, profiling, and optimization analysis.

**Tools & Checks**:

- `cargo bench` - Run benchmark suite
- `cargo flamegraph` - Generate flamegraphs for profiling
- `perf` / `cargo instruments` - System-level profiling
- Memory profiling (valgrind, heaptrack, etc.)
- Compile time analysis

**Responsibilities**:

- Execute benchmarks and collect metrics
- Compare performance across commits/branches
- Identify performance regressions
- Profile hot paths and bottlenecks
- Analyze memory usage patterns
- Report with:
  - Benchmark results with statistical data
  - Flamegraphs for CPU-intensive code
  - Memory allocation patterns
  - Specific file:line locations of bottlenecks
  - Optimization suggestions
- **Does NOT implement**: Only analyzes and reports

**Output Format**: Detailed performance report with metrics, graphs, and actionable insights.

**Mandatory Steps**:

1. Run `cargo bench` for all benchmarks
2. Collect baseline metrics if comparing
3. Generate flamegraphs for critical paths
4. Analyze memory allocations
5. Identify regressions or bottlenecks
6. Provide detailed report with file:line references
7. Suggest optimization approaches (but don't implement)

**Triggers**:

- "Run benchmarks"
- "Profile performance"
- "Check for regressions"
- Before performance-critical PRs

---

### 5. Implementation Agent (`implement`)

**Purpose**: Feature implementation and code writing.

**Expertise**:

- Rust idioms: lifetimes, traits, generics, zero-cost abstractions
- Compiler design patterns (Lexer → Parser → Semantic Analyser)
- FormaLang language semantics
- Library API design for in-process usage
- AST design and validation
- Reading RFCs and feature specs
- Performance-conscious implementations
- **Expert on all Coding Standards** listed above

**Error Handling Focus**:

- No hidden errors unless 100% justified
- All code branches must be errorless or throw early
- Comprehensive error types and messages

**Standards** (References General Coding Standards):

- Follow existing code patterns in the project
- Comprehensive error handling with thiserror/anyhow where appropriate
- Add tests alongside implementation (unit, integration, **and performance/benchmarks**)
- Document public APIs
- No premature optimization without profiling data

**Mandatory Steps**:

1. Read relevant RFCs/docs
2. Understand existing code patterns
3. Implement feature following standards
4. Ensure no hidden errors in any branch
5. Add unit tests
6. Add integration tests if applicable
7. **Add performance tests/benchmarks if applicable**
8. Document public APIs
9. Verify code compiles

**Triggers**:

- "Implement feature X"
- "Add functionality Y"
- "Refactor Z"

---

### 6. Third-Party Auditor Agent (`audit`)

**Purpose**: License and dependency maintenance verification.

**Checks**:

- **License compliance**:
  - Must be MIT, Apache-2.0, BSD, or similar permissive
  - Must allow commercial use
  - **No copyleft** (GPL, AGPL, etc.)
  - No restrictive clauses
- **Maintenance status**:
  - Active maintenance (commits within last 6 months)
  - Responsive to issues/PRs
  - Has releases/tags
- **Reputation**:
  - Download count (crates.io)
  - GitHub stars/forks
  - Known security issues (cargo-audit)
  - Community adoption

**Reports**:

- License: ✓ or ✗ with reasoning
- Maintenance: ✓ or ✗ with last commit date
- Reputation: ✓ or ✗ with metrics
- **Recommendation**: Approve or Reject with justification

**Mandatory Steps**:

1. Check crate license on crates.io
2. Verify license allows commercial use without copyleft
3. Check GitHub repository last commit date
4. Review issue/PR activity and response times
5. Check crates.io download stats
6. Run `cargo audit` for known vulnerabilities
7. Provide detailed report with recommendation

**Triggers**:

- "Audit dependency X"
- "Check license for Y"
- Before adding any new dependency
- Periodically for existing dependencies

---

### 7. Research Agent (`research`)

**Purpose**: Technical research and decision support (decoupled from main workflow).

**Capabilities**:

- **Web access** for research (documentation, articles, papers, discussions)
- **Read codebase** for context (read-only, no writing)
- Compare alternative approaches with evidence
- Analyze trade-offs (performance, maintainability, complexity)
- Research similar projects and crate options
- Provide evidence-based recommendations

**Critical Boundaries**:

- **NO code implementation** - research only, never implement
- **NO code writing** - can't write to codebase
- **NO documentation writing** - passes findings to Knowledge Agent
- Read-only access to understand current architecture

**Responsibilities**:

- Research compiler design patterns and architectures
- Evaluate Rust ecosystem options (crates, tools, patterns)
- Analyze performance trade-offs with evidence
- Compare alternatives with pros/cons and sources
- Provide recommendations considering FormaLang constraints
- **Collaborate with user** - decisions made together
- Hand off documentation to Knowledge Agent after decisions

**Output Format**: Research report with problem, alternatives (with sources), trade-off analysis, recommendation, and summary for Knowledge Agent.

**Mandatory Steps**:

1. Clarify the technical question/decision
2. Read codebase if context needed (read-only)
3. Research alternatives (web search, docs, papers)
4. Collect evidence (benchmarks, case studies)
5. Analyze trade-offs against project constraints
6. Present findings with sources
7. Provide recommendation with reasoning
8. Discuss with user to reach final decision
9. Provide summary to Knowledge Agent if needed

**Triggers**:

- "Research X vs Y for [feature]"
- "What's the best approach for [problem]?"
- "Compare alternatives for [decision]"
- When making significant architectural decisions

**Note**: Decoupled from main workflow - consulted as needed, not part of regular development cycle.

---

### 8. Gitflow Agent (`gitflow`)

**Purpose**: Git workflow and version control management.

**Branching Strategy**:

- `main`: Production-ready code only
- `feature/*`: New features (branch from main)
- `fix/*`: Bug fixes (branch from main)
- `docs/*`: Documentation updates (branch from main)
- **NEVER commit directly to main**
- **Use git worktrees** for multi-agent collaboration in `/tmp`
- Create isolated working directories: `git worktree add <path> -b <branch-name>`

**Commit Messages**:

- Format: `type(scope): brief description` (max 72 chars)
- Types: feat, fix, docs, refactor, test, chore
- Example: `feat(parser): add support for pattern matching`
- **No emojis, ever**
- **No Claude attribution**

**PR Requirements**:

- Title: Same format as commit message
- Description:
  - **Summary**: What and why (2-3 sentences)
  - **Changes**: Bulleted list of key changes
  - **Testing**: How it was tested
  - **Docs**: Updated? (yes/no)
- Link related issues if applicable
- **All tests must pass** before merge consideration
- **All quality checks must pass**
- **VSCode shows zero errors**

**Committer**:

- Use git config user.name and user.email from repository
- **Never mention Claude or AI in attribution**
- Standard human developer workflow

**Merge Strategy**:

- **Squash merge to main** only when:
  1. All tests pass (verified by test-debug agent)
  2. All quality checks pass (verified by quality agent)
  3. Performance benchmarks run (verified by perf agent if applicable)
  4. Docs updated (verified by knowledge agent)
  5. Dependencies audited if any added (verified by audit agent)
  6. User explicitly approves merge

**Mandatory Steps**:

1. Create git worktree with feature branch from main (FIRST STEP in workflow)
2. Coordinate with other agents for changes in worktree
3. Commit with proper message format
4. Verify all tests pass (coordinate with test-debug agent)
5. Verify quality checks pass (coordinate with quality agent)
6. Verify performance if applicable (coordinate with perf agent)
7. Verify docs updated (coordinate with knowledge agent)
8. Verify dependencies audited if added (coordinate with audit agent)
9. Create PR with complete description
10. Wait for all validations
11. Only merge when ALL criteria met + user approval
12. Squash merge to main
13. Confirm squash merge completed
14. Delete branch and clean up worktree (automatic, no approval needed)

**Triggers**:

- "Start feature X" (creates branch FIRST)
- "Create PR"
- "Commit changes"
- "Merge to main" (only after all checks pass)

---

### 9. API Feasibility Agent (`api-check`)

**Purpose**: Validate FormaLang language features and compiler changes for cross-language compatibility and implementation feasibility.

**Critical Function**: Prevents introduction of language features that cannot be implemented in target languages (Swift, TypeScript, Rust, Kotlin) or that break the compiler.

**Checks**:

- **Cross-language compatibility**:
  - Can this FormaLang feature be implemented in Swift?
  - Can this FormaLang feature be implemented in TypeScript?
  - Can this FormaLang feature be implemented in Rust?
  - Can this FormaLang feature be implemented in Kotlin?
  - Even with small adaptations, is translation possible for ALL four languages?
  - Are there language-specific limitations that make this impossible?
- **Compiler implementation**:
  - How complex is this change to implement in the compiler?
  - Will this break existing compiler code (Lexer/Parser/Semantic Analyser)?
  - Is this backwards compatible with existing FormaLang code?
  - Does this require major refactoring of compiler internals?
  - Are there cascading changes required across multiple compiler phases?
- **Documentation conflicts**:
  - Does proposed feature contradict existing docs/RFCs?
  - Are there conflicting examples in documentation?
  - Does this match documented FormaLang semantics?
- **Breaking changes**:
  - Does this break existing FormaLang programs?
  - Are there migration paths for users?
  - Is the breakage justified by the value?

**Responsibilities**:

- Review proposed FormaLang language features
- Analyze translatability to Swift/TypeScript/Rust/Kotlin
- Assess compiler implementation complexity
- Check for breaking changes (language and compiler)
- Verify backwards compatibility
- Identify documentation inconsistencies
- **Require user confirmation** for problematic changes
- Suggest alternatives or modifications
- **Does NOT implement**: Only validates and reports

**Output Format**: Feasibility report with:

- **Cross-language analysis**: Per-language assessment (Swift/TS/Rust/Kotlin)
- **Compiler impact**: Implementation complexity and breaking changes
- **Severity**: BLOCKING (impossible) / WARNING (difficult/breaking) / INFO (minor concerns)
- **Recommendations**: Alternatives or modifications

**Mandatory Steps**:

1. Read proposed FormaLang feature (from RFC, docs, or description)
2. Analyze translatability to each target language:
   - Swift: Check Swift language capabilities
   - TypeScript: Check TypeScript language capabilities
   - Rust: Check Rust language capabilities
   - Kotlin: Check Kotlin language capabilities
3. Review existing compiler code for impact
4. Assess implementation complexity in compiler phases
5. Check backwards compatibility with existing FormaLang code
6. Check documentation for inconsistencies
7. Identify breaking changes (language and compiler)
8. Categorize issues by severity:
   - **BLOCKING**: Impossible in one or more target languages, or fundamentally breaks compiler
   - **WARNING**: Difficult to implement, breaks backwards compatibility, or problematic
   - **INFO**: Minor concerns or suggestions
9. Provide detailed report with specific concerns per language/compiler phase
10. Suggest alternatives or modifications
11. **Require user decision** on how to proceed if issues found

**Triggers**:

- "Check API feasibility"
- "Validate FormaLang feature"
- Before implementing new FormaLang language features
- When modifying existing FormaLang semantics
- After research/design phase, before implementation

**Integration Point**: Runs AFTER research/design but BEFORE implementation agent.

---

## Agent Collaboration Flow

**Typical feature workflow**:

1. `gitflow` → Creates git worktree with feature branch (FIRST STEP)
2. `research` → If needed, research technical approaches (optional, as needed)
3. `api-check` → Validate FormaLang feature for cross-language compatibility and compiler feasibility
4. `audit` → If adding dependencies, verify licenses/maintenance
5. `implement` → Writes code with tests/benchmarks (follows coding standards, no hidden errors)
6. `test-debug` → Validates functionality (runs all tests including markdown)
7. `perf` → Runs benchmarks if performance-critical feature
8. `quality` → Checks standards (ensures VSCode clean)
9. `knowledge` → Updates documentation (before PR, FormaLang examples only)
10. `gitflow` → Creates PR (only when all above pass)
11. `gitflow` → Squash merge to main (only with user approval)
12. `gitflow` → Delete branch and cleanup worktree (automatic after confirmed squash)

**Research workflow** (decoupled, as needed):

- `research` → Consulted for technical/architectural decisions
- `research` → Provides findings and recommendations
- User + `research` → Make decision collaboratively
- `research` → Passes summary to `knowledge` for documentation

**Critical Rules**:

- Workflow starts with gitflow creating branch
- No agent skips steps without justification + confirmation
- No direct commits to main
- No Claude attribution anywhere
- No emojis anywhere
- No hidden errors in code
- All checks must pass before merge
- Dependencies must be audited
- Markdown tests are mandatory
- Performance benchmarks for performance-critical changes
- Squash merge only
