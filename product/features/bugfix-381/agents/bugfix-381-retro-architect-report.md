# Retrospective Architect Report: bugfix-381-retro-architect

## Feature: bugfix-381 — UDS injection visibility (GH #381 / PR #387)

---

## 1. Patterns: Disposition

### #3457 — DEPRECATED
**Entry:** "UDS obs log group: use target: `unimatrix_server::obs` on debug! for operator-togglable visibility"
**Action:** Deprecated. Fully subsumed by #3461, which covers the same convention with greater depth (all 5 log point locations, bash usage examples, watch script pairing, silent-by-default rationale, prerequisite EnvFilter fix). #3457 added nothing beyond #3461.

### #3461 — RETAINED (no change)
**Entry:** "UDS Observation Log: operator-togglable debug visibility via `unimatrix_server::obs` target"
**Assessment:** Comprehensive operator guide. Covers all 5 log points, RUST_LOG usage patterns, watch script pairing, and the prerequisite EnvFilter fix. Well-tagged (`tracing`, `observability`, `uds`). This is the canonical reference for future agents adding obs log points.
**Note:** Topic is `bugfix-381` — narrower than ideal for long-term discoverability. However, tags are sufficient for semantic search to surface it. No retopic action available.

---

## 2. Procedures: New and Updated

### #3465 — NEW
**Title:** "How to initialize tracing-subscriber in a new unimatrix-server entry point (tokio_main_*)"
**Content:** Copy-paste template for `EnvFilter::try_from_default_env().unwrap_or_else(...)` pattern, fallback semantics, RUST_LOG behavior, comment template, existing site locations. Directly actionable for any future `tokio_main_*` addition.
**Rationale:** The EnvFilter lesson (#3453) documents the bug; this procedure documents the correct implementation for new code.

### Allowlist procedure — NOT stored as new procedure
**Rationale:** The allowlist fix is human-action-only (settings.json), not an agent-executable procedure. The existing lesson chain (#3466, superseding #2803) captures it correctly as a lesson-learned. Storing a "procedure" would imply agents can self-remediate, which they cannot.

---

## 3. ADR Status

### ADR #3467 — NEW (stored)
**Title:** "ADR: UDS observation logs use synthetic target `unimatrix_server::obs` for independent operator control"
**Verdict:** The design reviewer correctly flagged that no existing ADR established a logging target convention. The `unimatrix_server::obs` synthetic namespace is an architectural constraint — future agents adding obs log points in any file must use it or break the operator silencing contract. ADR warranted and stored.

### ADR #3468 — NEW (stored)
**Title:** "ADR: tracing-subscriber initialized with `EnvFilter::try_from_default_env()` at all server entry points — static &str filter forbidden"
**Verdict:** The design reviewer asked whether this warranted an ADR for all future entry points. A procedure (#3465) covers the how; this ADR captures the why and the rejected alternatives (feature flags, --verbose-only, span-based propagation). The gate-enforceability note ("gate should reject static &str filter initialization") makes this worth formalizing as a decision.

**No ADRs superseded.** No prior ADR covered tracing initialization or log target conventions — confirmed by search.

---

## 4. Lessons: New Entries

### #3463 — NEW
**Title:** "Design corrections made interactively must be explicitly reflected in the rust-dev spawn prompt — GH issue comment updates are not enough"
**Trigger:** Rework signal: rust-dev used `debug!` after design reviewer corrected to `info!` interactively. The spawn prompt pointed to the GH issue; the correction lived in the design-reviewer-report. Neither was reconciled into the spawn prompt.
**Generalizable:** Yes — applies to all bugfix and delivery swarms where a design review phase produces corrections after the investigator report is written.

### #3464 — NEW
**Title:** "Implementation checklist items from approved design must be enumerated in the rust-dev spawn prompt — not left implicit in the investigator report"
**Trigger:** LP-4b (goal-absent else branch) was in the investigator's Step 3 and the GH issue body. The spawn prompt did not enumerate all N deliverables as a numbered checklist. The rust-dev implemented 4 of 5 log points and missed LP-4b.
**Distinction from #1372:** #1372 is about distilling library API signatures to reduce context loading. This lesson is about completeness guarantees: deliverables must appear as explicit numbered items in the spawn prompt, not as sub-points in prose reports.

---

## 5. Recommendations Actioned

| Hotspot/Recommendation | Action Taken |
|------------------------|-------------|
| [permission_retries] Add context_search and context_store to settings.json allowlist | Updated allowlist lesson: stored #3466 (supersedes #2803) adding context_search as new entrant. Human action still required on settings.json — flagged in lesson. |
| [compile_cycles] Batch field additions before compiling | Existing lesson #3439 already covers this. bugfix-381's 29-cycle count (rework peak) is consistent with that lesson. No new entry — #3439 updated check confirms the discipline is documented. Evidence not added to #3439 since it adds no new scenario (rework-phase compile pressure is already noted). |
| Gate fail — design correction not in spawn prompt | Stored lesson #3463. |
| Gate fail — LP-4b missing from spawn prompt | Stored lesson #3464. |
| EnvFilter pattern: should it be ADR? | Yes. Stored ADR #3468. |
| obs target convention: should it be ADR? | Yes. Stored ADR #3467. |

---

## Knowledge Stewardship

**Queried:**
- `context_get` on #3457, #3461, #3453 (full content review)
- `context_search` — "MCP tools settings.json allowlist permissions" (category: procedure) → no procedure found; confirmed lesson-chain is correct representation
- `context_search` — "spawn prompt implementation checklist design corrections agent" (category: lesson-learned) → found #765, #3436, #1372; none cover the design-correction-propagation gap
- `context_search` — "compile cycles incremental edits rework read before edit" (category: lesson-learned) → found #3439 (active, covers the scenario)
- `context_search` — "EnvFilter tracing initialization server entry point procedure" (category: procedure) → no existing procedure found
- `context_search` — "settings.json MCP tool allowlist context_search context_store permissions retry" → found #2803, #3387, #1270, #1206 (allowlist lesson chain)
- `context_search` — "tracing logging observability convention ADR server" (category: decision) → no existing ADR; confirmed gap
- `context_get` on #2803, #3387, #1270, #3439, #1372 (full content)

**Stored:**
- #3463 lesson-learned: design corrections must be in spawn prompt (NEW)
- #3464 lesson-learned: implementation checklist items must be enumerated in spawn prompt (NEW)
- #3465 procedure: tracing-subscriber initialization for new tokio_main_* entry points (NEW)
- #3466 lesson-learned: allowlist lesson updated with context_search (supersedes #2803)
- #3467 decision/ADR: unimatrix_server::obs synthetic target convention (NEW)
- #3468 decision/ADR: EnvFilter::try_from_default_env() required at all server entry points (NEW)

**Deprecated:**
- #3457: subsumed by #3461
- #2803: superseded by #3466

**Declined:**
- Allowlist fix as a procedure: human-action-only, not agent-executable
- Adding bugfix-381 evidence to #3439 (compile cycles): existing lesson covers the scenario; marginal evidence value does not justify a correction
- New lesson for rework-agent's knowledge stewardship WARN (queried but not invoked): one-off triviality note in a gate WARN, not a generalizable pattern
