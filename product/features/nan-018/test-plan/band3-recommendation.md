# Test Plan — Band-3 recommendation doc + Unimatrix convention/procedure (Wave 2)

**Component**: `product/features/nan-018/RECOMMENDATION-band3-protocol.md` (new, recommendation-only) + Unimatrix `convention`/`procedure` entries.
**Wave**: 2 — deferrable, zero code coupling. **AC-13 is a HARD GATE.**

## Verification method: file-check + shell (git-diff) + manual

### AC-12 — recommendation + knowledge
- (a) File-check: `RECOMMENDATION-band3-protocol.md` exists; `git diff --name-only` shows **zero** `.claude/protocols/` changes.
- (b) Unimatrix `convention` entry coupling shape-change ⇒ corpus migration exists and is surfacable in briefing.
- (c) `procedure` entries (migrate the corpus, author a scenario) exist.

### AC-13 — HARD GATE: no protocol/workflow change at all
- **Mechanical**: `git diff --name-only origin/main -- .claude/protocols/` must be **empty** (cross-ref `corpus-loader.md` `test_no_protocol_edits` — the same git-diff gate; owned mechanically there, certified here for the recommendation doc).
- **No gate-wiring** (review-checklist): no CI/PR hook makes eval **results** a standing decision gate; the one-time migration-validation run is allowed, a standing gate is not.
- **Doc content**: the recommendation states the **deferred-separate-design boundary** and that the recommended trigger is **asset-maintenance only**, explicitly NOT execution-gating; the trigger predicate is "your change alters the retrieval-shape hash" (coupled to the ADR-002 hash, not an enumerated list).

## Boundary note (R-16)
This is the protocol/gate-boundary breach risk. The hard `git diff` assertion is the mechanical guard; the no-gate-wiring and recommendation-only checks are review-checklist items. **No `.claude/protocols/` file may be edited** — this is non-negotiable.
