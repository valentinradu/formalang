# API Design Workflow

**Task**: $ARGUMENTS

You are designing or reviewing API changes. This focuses on public interfaces before implementation.

## Phase 1: Context (knowledge)

1. **Knowledge agent**: Gather context
   - Find existing related APIs
   - Review docs and RFCs
   - Understand current patterns

## Phase 2: Research (research)

1. **Research agent**: Research API patterns
   - How do similar tools/languages handle this?
   - Best practices for Rust library APIs
   - Ergonomics considerations

## Phase 3: API Design (api-check)

1. **API agent**: Design the public API
   - FormaLang syntax changes (if any)
   - Rust public types and functions
   - Error types and handling

2. **API agent**: Validate feasibility
   - Cross-language compatibility (Swift/TS/Rust/Kotlin)
   - Compiler implementation feasibility
   - Breaking changes analysis

3. Present API proposal:
   - **FormaLang Changes**: New syntax or semantics
   - **Rust API Changes**: Public types, traits, functions
   - **Feasibility Report**: Per-language assessment
   - **Breaking Changes**: Migration path if any

4. **Checkpoint**: User approval of API design

## Phase 4: Documentation (knowledge)

1. **Knowledge agent**: Document the API design
   - Create or update RFC
   - Document public API with examples
   - FormaLang examples only (no Rust implementation)

---

**Output**: Approved API design + documentation

This workflow does NOT implement. Use `/feature` after API is approved.

**Start now with Phase 1.**
