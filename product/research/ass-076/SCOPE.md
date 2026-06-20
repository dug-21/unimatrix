# ASS-076: Surfacing an entry's typed graph edges on `context_get` retrieval

**Date**: 2026-06-12
**Spike type**: Investigation + design-space evaluation (read-code dominant; no PoC required)
**Status**: SCOPE (draft — pending human confirmation of flagged decisions)
**Working number**: ass-076 (provisional)
**Tracking**: GH Issue #708
**Feeds**: the design session for this feature (research → design → delivery, per #708)

## Origin

Issue #708. `context_get` returns an entry's fields (title, content, topic, tags, status, supersession, confidence) but **does not surface the entry's typed graph edges**. Relationships are therefore invisible at the point of retrieval — an agent reading an ADR has no signal about what it depends on, contradicts, or is supported by, and no signal when an entry has *no* edges. Surfacing edges on read would (a) make relationships usable where they are actually read, and (b) close the feedback loop that makes the new author-asserted-edge convention (uni-architect / uni-store-adr) self-correcting — a zero-edge entry becomes *visible* on read. Surfaced from reviewing crt-052 ADRs, whose prose cites predecessors richly while the typed-edge graph stays sparse.

## Goal — questions to answer

- **RQ-1 — Surfacing depth.** Direct neighbors (depth-1) only, or some summarized multi-hop view? Should multi-hop traversal stay exclusively in `context_graph`, with `context_get` limited to the entry's immediate edges?
- **RQ-2 — Per-edge payload.** Which fields to include per edge: `edge_type`, direction (inbound/outbound), `target_id`, `target_title`? Is the `target_title` join worth its cost, or is `target_id` sufficient at the get layer?
- **RQ-3 — Supersession de-duplication.** Supersession is already represented in `supersedes` / `superseded_by`. How is the `Supersedes` typed edge excluded (or reconciled) so the relationship is not double-represented in the response?
- **RQ-4 — Inferred vs. authored edges.** Should inferred edges (CoAccess, similarity-derived) be structurally/visually distinguished from author-asserted edges (Prerequisite/Contradicts/Supports)? They carry very different trust.
- **RQ-5 — Response size / high-degree nodes.** Default-on or opt-in flag? What cap + total-count bound serves the feedback-loop goal (zero-edge entries must be visibly empty) without blowing response size on high-degree nodes?
- **RQ-6 — Format variants.** What does each output format render: `summary`/null (edge counts in text), `markdown` (a "Related" section), `json` (an edges array)? Define each.
- **RQ-7 — Cross-tool consistency.** How is the surfaced edge shape aligned with `context_graph`'s neighbors output so consumers learn one schema, not two?

## Breadth

**`code-only`** (primary) — `context_get` / `context_graph` handlers, the typed-edge storage and edge-type model, response serialization across the three formats. Light **`code+ecosystem`** touch only for MCP response-shape norms (how comparable MCP knowledge tools surface relationships on a point lookup).

## Approach

**Investigation** (what the get/graph paths and edge schema do today) + **evaluation** (rank the design options the issue raises against cost, trust-signal clarity, and cross-tool consistency).

## Confidence required

**Directional** — a recommended design with rationale and ranked options where calls are close. No working PoC required; FINDINGS.md is design input to the subsequent design session.

## Target outputs

Design input. FINDINGS.md recommends:
- depth + per-edge payload (RQ-1, RQ-2)
- supersession de-dup rule (RQ-3)
- inferred-vs-authored treatment (RQ-4)
- default-on/opt-in + cap/count policy (RQ-5)
- per-format rendering for summary/markdown/json (RQ-6)
- a single cross-tool edge shape aligned with `context_graph` (RQ-7)
Ranked options where a decision is genuinely open; one recommendation per RQ.

## Constraints

**Hard** (technically fixed; changing requires rewriting shipped code):
- Must reuse the existing typed-edge model and storage — no new edge type or schema migration to *surface* edges (read-path only).
- Must not double-represent supersession — `supersedes` / `superseded_by` remain authoritative for that relationship.
- Must not break `context_graph`'s existing neighbors contract — alignment may extend, not incompatibly change it.

**Hypothesis** (design positions held going in — researcher must treat as challengeable):
- Default-on surfacing best serves the feedback-loop goal (vs. opt-in flag).
- Depth-1 only at the get layer; multi-hop stays in `context_graph`.
- Include `target_title` despite the join cost (relationships are unreadable as bare IDs).

## Dependencies

- **Pairs with** the author-asserted-edge convention recently added to `uni-architect` + `uni-store-adr` (Prerequisite/Contradicts/Supports; high bar; default-none). Asserting edges without surfacing them gives no feedback; surfacing without asserting shows empty boxes — both ends are needed.
- **Relates to** vnc-035 carry-forward edges (#749) — outgoing edges carry forward by default; the surfaced view should reflect post-carry-forward edge state.
- **Unblocks** the design session for #708 (FINDINGS.md is its prior-art input).

## Non-Goals / Out of Scope

- **Implementing** the change — this spike produces FINDINGS.md only; design → delivery follow.
- **Backfilling historical edges** — deferred until both assert + surface ends exist (per #708).
- **Changing the edge-assertion conventions** (uni-architect / uni-store-adr) — they are an input, not under revision here.
- **Redesigning multi-hop traversal** in `context_graph` — depth-1 surfacing only.

## Prior art

- GH Issue #708 (full open-questions list).
- `context_get` and `context_graph` (neighbors) handlers in the server crate.
- The typed-edge model + storage (edge types incl. Prerequisite/Contradicts/Supports/CoAccess/Supersedes).
- vnc-035 carry-forward edges (#749).
- The author-asserted-edge convention in `uni-architect` and the `uni-store-adr` skill.
