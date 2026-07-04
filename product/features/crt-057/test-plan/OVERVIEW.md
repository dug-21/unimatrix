# crt-057 Test Plan — OVERVIEW

Fully non-destructive `context_cycle_review` with scoped, honest transcript retrieval
(`transcript{ phase?, anchor?, match?, window? }`).

**Feature:** crt-057 · GH #894 · Phase: Cortical · Stage 3a (test plan design)
**Rooted in:** `RISK-TEST-STRATEGY.md` (18 risks / 61 scenarios), `SPECIFICATION.md` (AC-01..AC-19),
`ACCEPTANCE-MAP.md`, `ARCHITECTURE.md` §1–§12, `IMPLEMENTATION-BRIEF.md`.

> **Rename note (OQ-4 = RENAME, human directive).** `distill_before_purge` is renamed. The exact new
> identifier lives in `pseudocode/OVERVIEW.md` (not authored at the time this plan was written —
> parallel Stage 3a). Throughout this plan `{NEW_NAME}` denotes the renamed helper (was
> `distill_before_purge`). Stage 3b/3c MUST substitute the concrete name from the pseudocode OVERVIEW
> and update every source-assertion string (`distill_handler.rs`). See `distill-before-purge.md` and
> open question OQ-A.

---

## 1. Test Strategy

This is a Rust-server feature (`unimatrix-server`, `unimatrix-observe`) plus a docs/protocol atomic unit
(`uni-retro/SKILL.md`, the tool description, both protocol files). Three test tiers:

| Tier | Vehicle | Carries |
|------|---------|---------|
| **Unit** | `cargo test -p unimatrix-server` / `-p unimatrix-observe`; extend the existing `distill_handler.rs` `#[cfg(test)]` module (C-7 — **no isolated scaffolding**) | The deep matrix that the MCP surface cannot express: the per-loss-condition `search_complete` matrix (R-01), skewed-clock windowed join with explicit fixed offsets (R-05), path-proven four-site fold rows incl. memo-hit (R-07), exhaustive `TranscriptRetention` re-home (R-06), source-assertion migration (R-11), struct-shape / no-new-persistence guards (R-03) |
| **Integration (MCP)** | `product/test/infra-001` pytest harness, `context_cycle_review` over JSON-RPC | The MCP-visible contract: lean default (AC-01), scoped retrieval returns candidates + loss (AC-02), `format:"summary"`→`ERROR_INVALID_PARAMS` (AC-11), non-destructive repeat (AC-03), `transcript:{}` full dump (AC-05), cycle-close-then-retrieve still returns candidates (R-08), no-new-persistence content-scan on the retrieval path (R-03), fold idempotency across repeated reviews (R-14) |
| **Doc / grep / lifecycle** | grep-style guards + per-protocol simulated-cycle trace | Consumer-reconciliation atomic unit (R-02, AC-16), two-protocol merge→close→retro (R-04, AC-17), ADR amendment mechanism (R-17, AC-15) |

**Cross-cutting test-construction constraints (apply to EVERY row in the matrix):**

1. **Negative assertions key on synchronous observable state** (#4879, R-10). "No purge" / "buffer intact" /
   "no candidates" is proven by reading the buffer *after* the review and asserting it is **still present
   with the same content**, or by a purge/reclaim spy captured **at handler return** — never by asserting
   zero async-audit rows. The positive side (a backstop reclamation firing) MAY poll; the negative side may not.
2. **Every path-specific row proves which of the four success returns executed** (#4452). The memo-hit fold
   row asserts a memo-hit indicator / no-recompute — it must not vacuously route through the full-pipeline path.
3. **Clock/window tests use explicit fixed offsets and on/inside/outside boundaries** (#4195/#4236), never
   `now_ts()`. The cross-plane join is **windowed, never exact**.
4. **AC-10 uses a populated fixture and a ratio** (#3548), guarded against the empty-buffer vacuous pass.
5. **Loss propagation is structural, not incidental** (R-01): no `match` path may return a bare `matched`
   without its per-session `SessionLossInfo`.
6. **Lifecycle + consumer changes are verified end-to-end and PER protocol** (#5383 reachability, not a
   store-layer read-back). A server-only or single-protocol green suite does NOT satisfy the gate.
7. **No dead code:** orphaned purge functions are DELETED (not `#[allow]`ed); the exhaustive retention match
   is re-homed with every variant covered (#4831).

---

## 2. Risk → Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Pri | Component test-plan file | Primary AC(s) | Tier |
|------|-----|--------------------------|---------------|------|
| **R-01** silent false negative (raison d'être) | Critical | `distill-before-purge.md` | AC-06 | unit (matrix) |
| **R-02** consumer partial-ship | Critical | `consumer-reconciliation.md` | AC-16, AC-19 | grep + integration e2e |
| **R-03** no-new-persistence leak | Critical | `attach-to-response-assembly.md`, `backstop-reclaim.md` | AC-14, AC-04 | unit + integration content-scan |
| **R-04** two-protocol mis-wiring | Critical | `retro-lifecycle.md` | AC-17 | per-protocol lifecycle + grep |
| **R-05** clock/skew normalization | High | `window.md`, `distill-before-purge.md` | AC-07, AC-08, AC-18 | unit (fixed offsets) |
| **R-06** orphan-deletion / backstop regression | High | `orphan-deletion.md`, `backstop-reclaim.md` | AC-13 | unit + dead-code guard |
| **R-07** fold four-site lockstep drift | High | `activity-fold.md`, `cycle-review-handler.md` | AC-04, AC-12 | unit (path-proven) |
| **R-08** cycle-close non-purging regression | High | `retro-lifecycle.md` | AC-17 | integration + unit |
| **R-09** scoped-filter correctness | High | `transcript-scope.md` | AC-02, AC-05 | unit + integration |
| **R-10** negative-assertion unreliability | High | cross-cutting (all files) | AC-03 | construction constraint |
| **R-11** source-assertion-removal side-effects | Med | `distill-before-purge.md`, `orphan-deletion.md` | AC-12 | unit (source-assertion) |
| **R-12** render divergence / `"summary"` drop | Med | `render-dispatch.md`, `retrospective-params.md` | AC-11 | unit + integration |
| **R-13** AC#10 vacuous/brittle | Med | `cycle-review-handler.md` | AC-10 | unit (ratio + vacuity guard) |
| **R-14** fold double-count | Med | `activity-fold.md` | AC-04 | unit + integration idempotency |
| **R-15** long-merge residency fidelity | Med | `backstop-reclaim.md` | AC-13 (NFR-8) | unit (aged-buffer) |
| **R-16** degraded/second-retrieval | Med | `snapshot-reuse.md`, `backstop-reclaim.md` | AC-03 | unit |
| **R-17** ADR via deprecate+store | Low | this OVERVIEW §5 | AC-15 | manual verification |
| **R-18** NG-7 creep / force non-orthogonality | Low | `cycle-review-handler.md` | AC-09 | unit |

Every AC-01..AC-19 is covered; AC-19 (ownership boundary, negative — the least-verified AC) gets a
**dedicated** negative schema+code-path test in `consumer-reconciliation.md` (§AC-19), NOT leaned on R-18.

---

## 3. Cross-Component Test Dependencies

- **Four-site lockstep** couples `cycle-review-handler.md` (threads scope + hosts the four returns),
  `distill-before-purge.md` (the shared candidate helper), `activity-fold.md` (the SOLE surviving gated
  side-effect), and `orphan-deletion.md` (the purge that left the seam). The source-assertion suite in
  `distill_handler.rs:651-726` is the seam of record — see §4.
- **Loss propagation** in `distill-before-purge.md` depends on `SessionLossInfo` fixtures that carry each
  loss signal independently; `snapshot-reuse.md` and `backstop-reclaim.md` reuse the same fixtures for the
  degraded/aged paths (R-15/R-16).
- **Clock normalization** in `window.md` (the `Window` type + default) feeds `distill-before-purge.md` (the
  windowed join over skewed Plane-B `ts` + `byte_offset` fallback).
- **Consumer atomic unit + lifecycle** (`consumer-reconciliation.md` + `retro-lifecycle.md`) share the
  four-doc grep set and the end-to-end "harvest fires" reachability check; a green server suite does not
  satisfy either.

---

## 4. Source-Assertion Migration (C-5 / R-11 — human watch-item)

`distill_handler.rs:651-726` currently holds two source-assertion tests over the `context_cycle_review`
handler body:

- `test_exhaustiveness_fifth_return_fails` — asserts `purge_cycle_transcripts(&feature_cycle)` appears
  **×4**, and that `distill_before_purge(` and `attach_to_response_assembly(` each match the purge count.
- `test_distill_strictly_before_purge_at_each_return` — asserts each purge is textually preceded by an
  `attach_to_response_assembly(` (the attach-before-purge ordering).

**Migration mandate (do NOT drop coverage — re-home it):**

| Current assertion | Disposition under crt-057 | Where it lands |
|-------------------|---------------------------|----------------|
| `purge_cycle_transcripts(&feature_cycle)` ×4 count | **REMOVED with in-source rationale** (purge gone, NG-6) | rationale comment in `distill_handler.rs` (R-11 sc.1) |
| attach-before-purge ordering (`test_distill_strictly_before_purge...`) | **REMOVED with rationale** (no purge to order against) | rationale comment; test deleted deliberately |
| `distill_before_purge(` ×4 → `{NEW_NAME}(` ×4 | **MIGRATED**: update the counted string to the new name; the ×4 count STANDS | `distill-before-purge.md` |
| `attach_to_response_assembly(` ×4 | **PRESERVED** (still gated ×4) | `activity-fold.md` / `cycle-review-handler.md` |
| **content-opaque fold-read ×4 gate** (`activity_snapshots_for_feature`, the sole surviving side-effect) | **PRESERVED / re-anchored** — now the primary source-assertion invariant; asserted ×4 at the success returns | `activity-fold.md` |

The exhaustive `TranscriptRetention` match that lived **inside** the deleted purge is re-homed onto the
surviving backstop reclaim paths and re-verified exhaustively (`RetainDays` a no-op, no `_` arm) — see
`orphan-deletion.md` and `backstop-reclaim.md`. **The C-5 obligation relocates; it does not disappear.**

---

## 5. Non-Code Verifications (owned by the tester/leader, not a Rust test)

- **AC-15 / R-17 (ADR amendment).** Verify the amendment used `context_correct` on #4742 and #4857
  (NOT deprecate+store — preserves provenance), and the amended content states all five points: purge
  removed / fully non-destructive; residency bounded by unchanged cap+TTL; disk posture unchanged;
  `force`/`format` orthogonal; fold read the sole surviving side-effect. Method: `context_get` on
  #4742/#4857, inspect the correction chain.
- **AC-16 grep guard (R-02).** See `consumer-reconciliation.md` — the corrected four-doc atomic unit
  (`uni-retro/SKILL.md`, tool description, both protocols). **`uni-agent-routing.md` is EXCLUDED** — a
  guard that greps it FAILS spuriously (passive mention, no live protocol loads it).
- **AC-17 protocol-parity grep (R-04).** Both protocol files contain the post-close `/uni-retro` step and
  neither retains a pre-merge `context_cycle(stop)`. A fix in only one protocol fails the gate.

---

## 6. Integration Harness Plan (infra-001)

`context_cycle_review` is exercised over JSON-RPC in `suites/test_lifecycle.py` and `suites/test_tools.py`
today. crt-057 changes the tool's param surface and behavior, so the MCP-visible contract needs coverage.

### 6a. Suites to run (Stage 3c — per suite-selection table)

| Feature touches | Suite | Why |
|-----------------|-------|-----|
| Any server tool logic (params/response) | **`tools`, `protocol`** | new `transcript` param, dropped `"summary"` |
| Store/retrieval behavior | **`lifecycle`, `edge_cases`** | store→review→retrieve, non-destructive repeat, cycle-close-then-retrieve |
| Longer residency / reclamation | **`volume`** (light) | residency envelope bounded; no schema change so not primary |
| Content scanning / no-persistence | **`security`** | retrieval-path content-scan, invalid-regex rejection |
| Any change | **`smoke`** | MANDATORY minimum gate |

Commands (from `product/test/infra-001/`, after `cargo build --release`):
`python -m pytest suites/ -v -m smoke --timeout=60` then the selected suites `-v --timeout=60`.

### 6b. Existing coverage to preserve (regression watch)

`test_phase_tag_store_cycle_review_flow`, `test_cycle_review_knowledge_reuse_cross_feature_split`,
`test_cycle_review_persists_across_restart`, `test_cycle_review_to_briefing_blending_chain`,
`test_cycle_review_curation_health_cold_start` all call `context_cycle_review` with `format=...` and no
`transcript`. Under crt-057 these MUST stay green (lean default unchanged; `markdown`/`json` intact). The
`cycle_review_index` fold persistence tests are the R-14 idempotency anchor — a non-purging repeat review
must not double-count.

### 6c. New integration tests to add (Stage 3c implements — extend existing suites, conventions below)

`suites/test_tools.py`:
- `test_cycle_review_default_no_candidates` — no `transcript` → response has NO candidate section (AC-01).
- `test_cycle_review_transcript_empty_returns_candidates` — `transcript:{}` → candidate section present with
  per-session `SessionLossInfo`; ≡ `transcript:{match:".*"}` under the cap (AC-02/AC-05).
- `test_cycle_review_format_summary_invalid_params` — `format:"summary"` → `ERROR_INVALID_PARAMS`, exact
  message `Valid values: "markdown", "json"` (AC-11).
- `test_cycle_review_invalid_match_regex_invalid_params` — malformed `match` regex → `ERROR_INVALID_PARAMS`,
  no panic (R-09 / security).

`suites/test_lifecycle.py`:
- `test_cycle_review_non_destructive_repeat` — review with `transcript:{}` twice → identical candidate set
  the second time (buffer survived; AC-03).
- `test_cycle_close_then_transcript_retrieval_returns_candidates` — register buffers → `context_cycle(stop)`
  → `transcript:{}` retrieval returns non-empty candidates (R-08 / AC-17; the two-protocol lifecycle's
  server-observable half).
- `test_cycle_review_fold_idempotent_across_repeats` — 3× default review → `cycle_review_index` fold metrics
  stable, no accumulation (R-14).

`suites/test_security.py`:
- `test_cycle_review_transcript_no_new_persistence` — after a `transcript:{}` retrieval, scan the DB / logs
  for candidate byte-content (no 64+ hex run, no verbatim delta text); assert none (R-03 / AC-14). Reuse the
  `#[traced_test]`/#5089 content-scan shape adapted to the harness.

**Fixtures:** default `server` (fresh DB, no leakage) for the tool-surface tests; `shared_server` /
`populated_server` where candidate volume must accumulate (AC-05, R-14). Follow the naming convention
`test_{tool_or_concept}_{specific_behavior}`.

### 6d. What integration CANNOT cover (unit-only, called out so the gate isn't fooled)

The per-loss-condition `search_complete` matrix (needs injected `Reconstructed`/`has_holes`/`elided_bytes`),
the skewed-clock windowed join (needs a fixture Plane-B `ts` offset from Plane-A), the path-proven memo-hit
fold row, the exhaustive `TranscriptRetention` re-home, orphan-deletion dead-code guards, and the
struct-shape no-candidate-field guard are **Rust unit tests**. Integration proves the MCP contract; it does
NOT substitute for the R-01/R-05/R-06/R-07 unit matrices. The two-protocol lifecycle (R-04) is a
doc-grep + per-protocol simulated-cycle verification, not a server unit test.

---

## 7. Open Questions

- **OQ-A (`{NEW_NAME}` resolution).** The concrete renamed identifier for `distill_before_purge` must be
  pulled from `pseudocode/OVERVIEW.md` (not yet authored). Stage 3b/3c substitutes it and updates the
  counted source-assertion strings. Until then every `distill-before-purge.md` assertion is written against
  `{NEW_NAME}`.
- **OQ-B (anchor/phase id representation).** The caller-facing `anchor` (finding id, e.g. `F-03`) and
  `phase` (phase id string) surface is a pseudocode detail (SPEC OQ-2). Test fixtures bind to whatever the
  pseudocode fixes; the resolution PATH (evidence-ts span / `cycle_events` bounds) is fixed and tested.
- **OQ-C (harness content-scan reach).** Confirm the infra-001 harness can read the server's SQLite DB and
  captured logs for the R-03 content-scan (6c). If not, R-03 leans fully on the Rust `#[traced_test]` unit
  guard and the integration security test is downgraded to a response-shape check.
