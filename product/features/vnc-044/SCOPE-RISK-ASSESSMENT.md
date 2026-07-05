# Scope Risk Assessment: vnc-044

Scope-level risks for splitting the `context_graph` `format` overload into serialization (`format`) + verbosity (`detail`) axes, with a lean node projection. First adopter of a suite-wide ADR standard.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `content_preview` needs the full `content` column read from the DB (no SQL content-drop per D-6). The lean projection saves wire bytes, not DB I/O or hydration cost — a 135KB→few-KB win that could be misread as a query-cost win. | Low | Med | Architect: scope this as an output/serialization change only. Do not promise DB-read reduction; state the win is payload size and agent-context size. |
| SR-02 | UTF-8 char-boundary truncation of `content_preview` at 256 bytes is easy to get wrong (byte-split multibyte char → invalid UTF-8 / serialization panic). Naive `&s[..256]` panics on a non-boundary. | High | Med | Architect: mandate `floor_char_boundary`-style flooring in the ADR; spec must require multibyte-straddle test at byte 256 (AC-03b already names it — keep it non-negotiable). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The ADR binds the ENTIRE context-tool suite (field name, values, default, 256 constant, summary field set) but only `context_graph` is implemented + exercised. The standard can drift from what later adopters (`context_get`, `context_search`, mutations, `context_briefing`) actually need — untested contract. Evidence: #4975 (a locked ADR value drifting downstream when only partially exercised). | High | Med | Architect: keep the ADR's suite-wide claims to what generalizes cleanly; mark tool-specific summary field sets as per-tool overridable (D-6 already allows documented exceptions). Ratify exact spelling (`detail` vs `verbosity`/`view`) once, in the ADR, as the single source — never restate it in the issue body. |
| SR-04 | Default flips `full`→`summary` — a silent behavior change for every existing `context_graph` caller that relied on full `EntryRecord` output without passing a verbosity axis. Non-graph tools keep full defaults, so the suite is temporarily inconsistent by design. | High | High | Spec: make the flip an explicit, documented ACCEPTED behavior change (AC-05). Recommend the tool description and ADR call out the per-tool default divergence during the migration window so agents are not surprised mid-suite. |
| SR-05 | `format=markdown` on graph is rejected loudly (D-4), but `markdown` remains a valid `format` value suite-wide. A caller reasonably expects `markdown` to work on every context tool; graph rejecting it is a discoverability cliff. | Med | Med | Spec: the `ERROR_INVALID_PARAMS` message must name the reason (no graph-markdown renderer) and point to `format=json`. Document the graph exception in the tool description, not only the error. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `ResponseFormat`/`parse_format` are suite-shared (`response/mod.rs`). Any behavior change to the shared enum ripples to ~all context tools (Non-Goal 1 forbids this). Adding a variant/field to a shared wire enum has ~45-site blast radius and exhaustive-match breakage. Evidence: pattern #4831. | High | Med | Architect: keep vnc-044's code change graph-local — a distinct projection type / local serializer, NOT a mutation of the shared enum or `EntryRecord`. If the axis touches the shared enum, front-load the full site enumeration (`cargo test --workspace --no-run`) before building. |
| SR-07 | Projection must be a separate type — adding `skip_serializing_if` to `EntryRecord`/`EdgeRecord` (shared `unimatrix-store` types, wire-locked by ADR-003/004) leaks into every other serializer suite-wide. | High | Low | Architect: forbid touching `EntryRecord`/`EdgeRecord`; require a dedicated projection type or local `serde_json::Value` builder (SCOPE already constrains this — hold the line at gate). |
| SR-08 | `graph_read_subgraph.rs` is already near the 500-line/file limit; a new projection type risks pushing it over (Pattern #4518). | Low | Med | Architect: place the projection in its own module up front. |

## Assumptions

- **A1 (SCOPE Central Finding / lines 31-40):** `format` is parsed then discarded at `graph_read.rs:251`; there is NO existing lean projection anywhere. If a projection or partial wiring already exists undiscovered, effort and blast radius change. Low risk — SCOPE claims code-verified.
- **A2 (D-3 / lines 66-69):** `EntryRecord.status` (lifecycle) is a clean stand-in for what #913 orientation needs. **If wrong, the feature ships a projection that answers a different question than the motivating use case asks** — see SR-09.
- **A3 (D-1 / line 7):** The two-axis model generalizes across all context tools without per-tool exceptions beyond default-verbosity. If later adopters need materially different summary field sets, the "suite-wide standard" claim weakens — see SR-03.

## Design Recommendations

1. **SR-09 (the honestly-carried gap — elevate to architect attention):** The lean projection carries *lifecycle* status (`active/deprecated/...`), not *capability delivery* status (`missing/partial/proven/claimed`, buried in `content`). The #913 orientation use case wants a delivery-status tally. `content_preview` only *partially* softens this (D-6). The feature will look like it satisfies #913 while a subgraph of capabilities returns `active` for every node — misleading. Architect + spec must state this limitation prominently in the ADR, tool description, and AC-06, and keep delivery-status promotion as a named follow-up (Tracking #3). Do not let the projection imply it answers orientation-status.
2. **Hold graph-scope on shared types (SR-06/SR-07):** the ADR sets the suite target contract; vnc-044's *code* must not alter shared-enum/shared-struct behavior for non-graph callers. Guard at gate.
3. **Guard locked ADR values with discriminator tests (SR-03, evidence #4975):** the 256-byte constant, exact axis spelling, and summary field set are locked ADR values — single-source them; do not restate in the issue body; test the exact boundary (SR-02: exactly-256, straddle-256, empty).
