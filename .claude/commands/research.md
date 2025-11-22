# Research Workflow

**Question**: $ARGUMENTS

You are conducting deep technical research. This workflow is for exploration and decision-making, not implementation.

## Phase 1: Scope Definition

1. Clarify the research question
2. Identify what decisions need to be made
3. **Checkpoint**: Confirm scope with user

## Phase 2: Context Gathering (knowledge)

1. **Knowledge agent**: Retrieve internal context
   - Search codebase for related patterns
   - Find relevant docs, RFCs, prior decisions
   - Identify constraints from existing architecture
   - Summarize findings

## Phase 3: External Research (research)

1. **Research agent**: Deep external research
   - Web search for documentation, articles, papers
   - Analyze similar projects (rustc, tree-sitter, pest, etc.)
   - Find benchmarks, case studies, comparisons
   - Evaluate relevant crates/libraries

2. Present findings in structured format:
   - **Problem/Decision**: Clear statement
   - **Context**: Current state summary
   - **Alternatives**: With pros/cons and sources
   - **Trade-offs**: Performance vs complexity vs maintainability
   - **Recommendation**: Evidence-based

## Phase 4: Negotiation

1. **Research agent**: Discuss findings with user
   - Present options objectively
   - Answer questions
   - Refine based on feedback
   - **Collaboratively reach decision**

## Phase 5: Documentation (knowledge)

1. **Knowledge agent**: Document the decision
   - Create RFC if significant architectural decision
   - Update relevant docs
   - Record rationale for future reference

---

**Output**: Research report + documented decision (no code implementation)

**Start now with Phase 1.**
