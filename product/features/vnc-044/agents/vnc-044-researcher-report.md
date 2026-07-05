# vnc-044 Researcher Report

## Deliverable
- SCOPE.md: `/workspaces/unimatrix/product/features/vnc-044/SCOPE.md`

## Key findings (code-grounded)
- The `context_graph` `format` param conflates two orthogonal axes: serialization (markdown/json) and verbosity (summary/full). `summary` is not a peer of markdown/json.
- **#913's premises are false against the code.** `chain`/`current` do NOT return lean records — every node-returning graph mode (chain, current, subgraph, inverse, filter) serializes the full `EntryRecord` via `serde_json::to_string`, format-blind. `neighbors`/`path` return only edges. No lean projection exists anywhere in the graph path — it must be built.
- `format` is parsed (`ResponseFormat`, `response/mod.rs:59-81`) but **discarded** in the graph path: `handle_graph` binds it `_ctx` and never reads it (`graph_read.rs:251`). Fixing this parse-and-drop seam is the core wiring change.
- Graph output is **JSON-only in practice** (all modes end in `serde_json::to_string`); no markdown graph renderer exists, so `format=markdown` on a graph today is a silent no-op.
- `EntryRecord.status` (schema.rs:57) IS a first-class field — but it is the **lifecycle** enum (Active/Deprecated/Proposed/Quarantined), NOT the capability **delivery** status (missing/partial/proven/claimed) that #913's orientation use case actually wants. That delivery status lives inside the capability's `content` blob. This is the highest-value nuance in the feature (stored as pattern #5505).
- Prior art: `context_cycle_review` (crt-057/vnc-011, tools.rs:446) already treats `format` as a render-only axis and rejects `summary` — the suite is already inconsistent; good template for a generalizable design.
- `GraphParams` layout is locked (ADR-003) — new verbosity axis must be additive. `EdgeRecord`/`EntryRecord` must not get `skip_serializing_if` (shared types); projection must be a distinct type.

## Settled (all four OQs, human 2026-07-05 — SCOPE Status: SETTLED)
- **D-1 (was OQ-1):** the two-axis model is a **suite-wide standard captured as an ADR** (Phase-2a, uni-architect). vnc-044 is the **first adopter** — implements it for `context_graph` only; other tools migrate in follow-up features referencing the ADR. Suite *consistency of the model* = a GOAL; suite *implementation* = deferred.
- **D-2 (was OQ-2):** default verbosity = `summary`, suite-wide norm (minimum context by default; opt into full or `context_get` a single entry). Accepted behavior change for graph. Legacy `format=summary`→`detail=summary`, serialization `json`.
- **D-3 (was OQ-3):** option (c) — projection carries lifecycle `EntryRecord.status` only; capability delivery status = dependent follow-up feature. Goal 6 reworded to not promise a status tally.
- **D-4 (was OQ-4):** recommend `detail: summary|full` (architect ratifies exact spelling in ADR); neighbors/path accept-and-ignore; `format=markdown` on graph rejected loudly (no silent JSON fallback).
- **D-5:** reconciles crt-057/vnc-011 (GH #894) — crt-057 got render-axis right but lacks the verbosity axis; the ADR generalizes it and restores summarized content as `detail=summary`.

## Remaining (delegated, non-blocking)
- uni-architect ratifies the exact suite-wide axis name/value spelling in the Phase-2a ADR. SCOPE recommends `detail: summary|full`.

## Risks / concerns
- The #913 headline "one-call capability status tally" is **NOT** delivered by vnc-044 (D-3 projects lifecycle status only) — expectation set explicitly in Goal 6 and filed as dependent follow-up.
- Default-summary (D-2) is a real behavior change for existing full-node graph consumers — intentional, called out in AC-05.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced graph_read mode/param patterns (#4500 four-coordinated-changes-per-mode, #4518 file-split-at-500-lines, #4490/#4491 GraphParams lock, ADR-004 EdgeRecord no-skip). Applied as constraints.
- Stored: entry #5505 "Two distinct 'status' concepts: EntryRecord.status (lifecycle) vs capability delivery status (in content)" via context_store — generalizable trap for any projection/orientation/status-tally work. Feature-specific scope details kept in SCOPE.md, not stored.
