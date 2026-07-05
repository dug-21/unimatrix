# Agent Report — vnc-044-agent-2-spec

**Role:** Specification writer
**Feature:** vnc-044 — split `context_graph` `format` overload into serialization (`format`) + verbosity (`detail`) axes with a lean node projection.

## Deliverable

`product/features/vnc-044/specification/SPECIFICATION.md`

Covers all nine SCOPE acceptance criteria (AC-01..AC-09; AC-03b broken out as its own row) with concrete verification methods, 12 functional requirements, 6 non-functional requirements, domain models, user workflows, constraints, dependencies, and explicit exclusions.

## Key decisions / interpretations

- **Axis spelling treated as placeholder.** Wrote the spec against SCOPE's recommended `detail: summary|full`, flagged every load-bearing dependency on the architect's final spelling in OQ-A, and stated the placeholder disclaimer up front. No semantic change expected on ratification.
- **AC-01 (the ADR) marked out-of-implementation-scope** but retained for traceability — it is a Phase-2a companion deliverable owned by uni-architect, not something this feature's code satisfies.
- **SR-09 elevated into AC-06 and the domain model.** Made the lifecycle-vs-delivery status gap explicit: a capability subgraph returns `active` for every node and does NOT deliver a delivery-status tally. This is stated in Domain Models, FR-12 (tool description requirement), and AC-06's verification. Delivery-status surfacing kept as an out-of-scope follow-up.
- **`content_preview`/`content_truncated` test matrix pinned as non-negotiable** in AC-03b: empty, <256B, exactly-256B, 257B ASCII, and multibyte-straddling-256. Prohibited naive `&s[..256]`; required `floor_char_boundary`-style flooring (SR-02).
- **Shared-type guardrails codified** as NFR-2 + C-2/C-3/C-4: no `skip_serializing_if` on `EntryRecord`/`EdgeRecord`, projection is a distinct graph-local type, shared `ResponseFormat` behavior unchanged for non-graph callers (SR-06/SR-07).
- **Payload-size-not-DB-cost** framing captured as NFR-4 (SR-01): preview still requires the full `content` read; the win is wire/context bytes only.

## Open questions carried to architect

- OQ-A: exact suite-wide axis name/value spelling (ADR ratifies).
- OQ-B: projection placement (dedicated module vs local `serde_json::Value` builder) given the 500-line limit on `graph_read_subgraph.rs`.
- OQ-C: the 256 constant's value and single-source location.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing (task: spec for context_graph two-axis split + lean projection) — surfaced relevant prior art: #4490/#4491 (GraphParams layout lock, ADR-003 vnc-018/019), #4478 (EdgeRecord placement/lock, ADR-004 vnc-018), #4500 (per-mode coordinated-change pattern), #4518 (extract-module when graph_read files near 500-line limit), #4502/#4477 (context_graph mode increments), #952/#87 (format-selectable response prior art), #5449 (vnc-043 description source-of-truth). All reinforced constraints already in SCOPE; folded into the spec's Constraints/Dependencies. No new generalizable pattern to store — spec decisions are feature-specific (read-only tier, as expected for this role).
