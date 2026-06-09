# Agent Report — nan-018-agent-2-spec-r2

**Role**: Specification writer (R2 lock-in)
**Task**: Lock four human-ratified decisions into nan-018 SPECIFICATION.md; resolve spec ambiguities #3 and the R-04 gate; correct the penalty consumption-site error.

## Changes Applied

1. **FR-22 + AC-08 + OQ#3 — hash-mismatch behavior LOCKED (corpus-dependent).**
   - Primary fixture corpus → **HARD ERROR (abort, non-zero exit)**.
   - Production snapshot corpus → **WARN (continue)**.
   - Rationale recorded: durable yardstick numbers propagate to product ranking policy; drift guard protects corpus *validity* and deliberately overrides the eval `report` exit-0 quality-verdict convention for the primary corpus only.
   - AC-08(b) split into per-corpus mismatch tests (abort vs warn).
   - Open Question #3 marked RESOLVED.

2. **FR-03 + AC-01(c) + new Integration Surface section — penalty sites corrected and LOCKED.**
   - The two penalty-application sites are **both in `crates/unimatrix-server/src/services/search.rs`**: `:727` (FALLBACK_PENALTY fallback branch) and `:729` (graph_penalty call). Verified directly against source.
   - `background.rs:583` removed as a threading target — it is a `tracing::error!` log string only. Verified directly (line 582-584).
   - Added a dedicated **Integration Surface (LOCKED)** subsection with the two-site table.
   - R-01 enumerated-site grep guard reaffirmed as source of truth for AC-01 bit-for-bit equivalence.

3. **FR-12a (new) + AC-09 — cost-growth gate advisory at ε=0.0 (report-only).**
   - Any cost growth (delta > 0.0) is reported but blocks nothing; `eval report` exit code unchanged. Consistent with eval not being a workflow gate (NOT-in-scope #1).

4. **AC-08(e/f) + OQ#3 R-04 note — column-manifest completeness as named human delivery gate.**
   - AC-08(e): sensitivity test proves hash reacts only to the *declared* manifest.
   - AC-08(f) / R-04: a **named human review at delivery** (not routine code review) certifies the declared manifest is *complete* — no retrieval-relevant entry column mis-classified as display-only.

## Deliberately Left Flagged (delivery-detail — NOT invented)

- **OQ #1** — exact token-proxy formula and error bars (architect, SR-02).
- **OQ #4** — trust-assertion JSONL on-disk schema (`expected` extension vs sibling field) (architect).
- **OQ #2** — precise hash manifest column/confidence-field list (architect); R-04 governs its completeness review but the list itself stays an architect call.

## Confirmation
- **FR-22 LOCKED**: primary = hard error; snapshot = warn.
- **Penalty-site list LOCKED**: `services/search.rs:727` and `services/search.rs:729` only; `background.rs` is not a target.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — re-surfaced nan-018 ADRs (#4889/#4890/#4892/#4893) and config-field-mismatch lessons (#4148/#4333), which corroborate the penalty-site correction as a doc-vs-code mismatch of the same class. No new patterns stored (read-only tier; spec decisions are feature-specific).
