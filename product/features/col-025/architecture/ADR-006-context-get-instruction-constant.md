## ADR-006: CONTEXT_GET_INSTRUCTION — Static Header on format_index_table Output

### Context

`IndexBriefingService::format_index_table` produces a ranked Markdown table of
knowledge-base entries (ID, title, relevance, snippet). This table is returned in
two contexts:

1. **MCP `context_briefing` response** — an agent calls the tool and reads the
   table to decide which entries to retrieve with `context_get`.
2. **UDS `CompactPayload` injection** — the ranked table is prepended to a
   subagent's context before its first token (col-025 col-025 ADR-003: goal-present
   SubagentStart path also produces this payload via `IndexBriefingService::index`).

In both cases, agents receive a table of entry IDs and summaries. The table is
useful only if the agent knows how to act on it: specifically, that `context_get`
with the entry ID retrieves the full content. Without this instruction, agents must
either infer the workflow from prior training or ask, adding unnecessary round-trips.

Two approaches were considered:

**Option A**: Document the `context_get` follow-up pattern in agent definitions and
protocol files, relying on agents to have absorbed the convention before they see
a briefing table. This works for experienced agents but fails for newly spawned
subagents that have not yet loaded their protocol file.

**Option B (chosen)**: Prepend a single, brief, static instruction line to every
`format_index_table` output. This is self-contained: any agent receiving the table
in any context (tool response or UDS injection) immediately knows how to act on it.
The instruction is a named constant — not an inline string — so it can be updated
in one place.

The instruction is a one-line header, not repeated per row. It does not add
material size to the payload (one line vs. the k=20 table rows that follow).

### Decision

Define a public constant in `src/services/index_briefing.rs`:

```rust
pub const CONTEXT_GET_INSTRUCTION: &str =
    "Use context_get with the entry ID for full content when relevant.";
```

In `format_index_table`, prepend this instruction as the first line before the
table header:

```
Use context_get with the entry ID for full content when relevant.

| ID | Title | Relevance | Snippet |
|----|-------|-----------|---------|
| … | …     | …         | …       |
```

The constant is applied to **both** emission paths:
- MCP `context_briefing` responses (via `handle_briefing` in `tools.rs` calling
  `IndexBriefingService::format_index_table`).
- UDS `CompactPayload` injection (via `handle_compact_payload` calling
  `IndexBriefingService::index` then `format_index_table`).
- The revised SubagentStart goal-present branch (ADR-003) also calls
  `IndexBriefingService::index`, so it inherits the instruction automatically.

Because the constant is defined on `format_index_table`, no call site changes
are required — every consumer of `format_index_table` gets the instruction.

The constant name `CONTEXT_GET_INSTRUCTION` must not be inlined at call sites.
If the instruction text needs to change (e.g., tool rename), one constant update
propagates everywhere.

### Consequences

- Every agent receiving a briefing table — whether via tool call or UDS injection —
  immediately understands how to retrieve full content for relevant entries.
- No per-row repetition; the instruction is a single header line.
- The constant is a single source of truth; renaming the tool or updating the
  instruction requires one change in one file.
- `format_index_table` tests must be updated to assert the instruction line is
  present in the output (spec writer must add or update these tests).
- The instruction adds ~60 bytes to every `format_index_table` output. This is
  negligible relative to the k=20 table body and the `MAX_INJECTION_BYTES` limit.
- If a caller wants raw table output without the instruction (e.g., for testing
  row counts), they must strip the first non-empty line or adjust the assertion.
  Spec writer should use a helper that strips the header for raw-table assertions.
