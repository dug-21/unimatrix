# Component 6: Entry Quality Gate — Pseudocode

## Purpose

Applied to every proposed seed entry before presenting to human. Entries failing the gate are silently discarded (NFR-05). The gate ensures minimum quality before human review.

## Algorithm

```
FUNCTION quality_gate(entry) -> PASS | FAIL:
    // Field: what
    IF entry.what is missing OR empty:
        RETURN FAIL  // silently discard
    IF length(entry.what) > 200:
        RETURN FAIL  // too verbose

    // Field: why
    IF entry.why is missing OR empty:
        RETURN FAIL
    IF length(entry.why) < 10:
        RETURN FAIL  // too terse
    IF entry.why is tautological (restates entry.what without adding motivation):
        RETURN FAIL

    // Field: scope
    IF entry.scope is missing OR empty:
        RETURN FAIL

    // Category check
    IF entry.category NOT IN {"convention", "pattern", "procedure"}:
        RETURN FAIL  // excluded categories: decision, outcome, lesson-learned, duties

    RETURN PASS
```

## Quality Field Definitions

| Field | Rule | Max/Min | Reject Condition |
|-------|------|---------|------------------|
| `what` | One sentence describing the knowledge | <= 200 chars | Missing, empty, or exceeds 200 chars |
| `why` | Consequence or motivation | >= 10 chars | Missing, empty, under 10 chars, tautological |
| `scope` | Where it applies (component, module, context) | n/a | Missing or empty |

## Allowed Categories

| Category | When to use |
|----------|-----------|
| `convention` | Project-level standards (naming, file layout, process) |
| `pattern` | Reusable architectural/implementation approach |
| `procedure` | Step-by-step workflow specific to this repo |

## Excluded Categories (ADR-006)

- `decision` — ADRs emerge from real feature work
- `outcome` — outcomes emerge from shipped features
- `lesson-learned` — lessons emerge from failures
- `duties` — agent responsibilities, not seed knowledge

## Tautology Detection

A `why` is tautological if it merely restates the `what` without explaining the consequence. Examples:

- GOOD: What: "Use snake_case for all module files" / Why: "CI linter rejects non-snake-case and blocks merges"
- BAD: What: "Use snake_case for all module files" / Why: "Because we use snake_case"

## Integration with Seed Flow

1. Model generates candidate entry from repo file reads
2. Model self-applies quality gate before presenting to human
3. Entries that fail are silently discarded — not shown to human
4. Only passing entries are presented for approval (batch at L0, individual at L1+)
5. Only approved entries are stored via context_store
