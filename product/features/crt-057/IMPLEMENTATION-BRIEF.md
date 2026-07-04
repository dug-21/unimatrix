# crt-057 Implementation Brief — Fully Non-Destructive `context_cycle_review` with Scoped, Honest Transcript Retrieval

**Feature:** crt-057 · **Tracking:** GH #894 (promoted from bug #871, closed) · **Phase:** Cortical (crt)
**Contract status:** REWORKED 2026-07-04 after research spike **ass-091 (#898)**. The prior boolean contract
(fused `include_transcript_candidates` emit+purge) is **superseded**. The transcript axis is now a **read-only
scoped retrieval** (`transcript{ phase?, anchor?, match?, window? }`); the review **loses its purge verb entirely**.
**Session type:** delivery (design complete; this brief is the delivery entry point).

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-057/SCOPE.md |
| Scope Risk Assessment | product/features/crt-057/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/crt-057/specification/SPECIFICATION.md |
| Architecture | product/features/crt-057/architecture/ARCHITECTURE.md |
| ADR-001 (fully non-destructive review + residency; amends #4742/#4857) | product/features/crt-057/architecture/ADR-001-purge-trigger-and-residency.md |
| ADR-002 (three-axis API surface; read-only `transcript{}`; `"summary"` dropped) | product/features/crt-057/architecture/ADR-002-three-axis-api-surface.md |
| ADR-003 (loss-propagation contract; INDETERMINATE no-match) | product/features/crt-057/architecture/ADR-003-warn-and-proceed-on-incomplete-extraction.md |
| ADR-004 (four-site seam now gates ONLY the content-opaque fold read) | product/features/crt-057/architecture/ADR-004-flag-gating-lockstep.md |
| ADR-005 (retro lifecycle; non-purging cycle-close; both protocols) | product/features/crt-057/architecture/ADR-005-retro-lifecycle-and-cycle-close.md |
| ADR-006 (scoped retrieval mechanism + clock normalization + ±120 s/±3-block window default) | product/features/crt-057/architecture/ADR-006-scoped-retrieval-and-clock-normalization.md |
| Risk-Test Strategy | product/features/crt-057/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-057/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/crt-057/ACCEPTANCE-MAP.md |

> ADR-003 / ADR-004 file *slugs* retain boolean-era names ("warn-and-proceed", "flag-gating"); their
> *content* is reworked to the non-destructive contract (loss propagation / fold-only gating). Do not
> read the filenames as the design.

---

## Component Map

Components derived from ARCHITECTURE §3 (component breakdown) and §12 (integration surface).
Pseudocode / test-plan paths are populated in Stage 3a. `[NEW]`/`[REMOVE]`/`[UNCHANGED]` per ARCH §12.

| Component | File | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| `RetrospectiveParams` — remove `include_transcript_candidates`; add `transcript: Option<TranscriptScope>` (`#[serde(default)]`); keep `format`/`force` | `unimatrix-server/src/mcp/tools.rs:~431` | pseudocode/retrospective-params.md | test-plan/retrospective-params.md |
| `TranscriptScope` `[NEW]` — `{ phase?, anchor?, r#match?, window? }` all-optional AND-composed filter block | `unimatrix-observe` or `mcp/tools.rs` | pseudocode/transcript-scope.md | test-plan/transcript-scope.md |
| `Window` type + default `[NEW]` — ±N events / ±T millis; default ±120 000 ms / ±3 blocks | new type | pseudocode/window.md | test-plan/window.md |
| `context_cycle_review` handler — thread `transcript` scope; **delete the four `purge_cycle_transcripts` calls**; drop `"summary"` alias | `unimatrix-server/src/mcp/tools.rs:~2125` | pseudocode/cycle-review-handler.md | test-plan/cycle-review-handler.md |
| `distill_before_purge` — scoped filter + clock normalization + per-session `SessionLossInfo`; returns `None` when `transcript` absent; **name vestigial** (no purge follows) | `unimatrix-server/src/mcp/distill_handler.rs:48` | pseudocode/distill-before-purge.md | test-plan/distill-before-purge.md |
| `attach_to_response_assembly` `[UNCHANGED]` — no-ops on `None`/`Err` | `unimatrix-server/src/mcp/distill_handler.rs:281` | pseudocode/attach-to-response-assembly.md | test-plan/attach-to-response-assembly.md |
| `snapshot()` `[UNCHANGED]` — the SOLE content reader reused; no new reader (#4848) | `unimatrix-server/src/infra/session_transcript.rs:296` | pseudocode/snapshot-reuse.md | test-plan/snapshot-reuse.md |
| `purge_cycle_transcripts` `[REMOVE]` — orphaned once the four calls go; delete it + `clear_transcripts_for_feature` + `purge_held_for_feature` | `unimatrix-server/src/server.rs:661` | pseudocode/orphan-deletion.md | test-plan/orphan-deletion.md |
| Content-opaque fold read (crt-054/055) `[UNCHANGED]` — STAYS gated ×4; sole surviving success side-effect | `unimatrix-server/src/mcp/activity_fold_handler.rs`, `session.rs:566` | pseudocode/activity-fold.md | test-plan/activity-fold.md |
| Backstop reclaim (sole path) `[UNCHANGED]` — `sweep_expired` / cap-eviction / session-close; re-home the exhaustive `TranscriptRetention` match here | `unimatrix-server/src/infra/transcript_hold.rs:308` | pseudocode/backstop-reclaim.md | test-plan/backstop-reclaim.md |
| Render dispatch — drop `"summary"` arm ×4 | `unimatrix-server/src/mcp/tools.rs` (:2532, :3359, :4268, :4324) | pseudocode/render-dispatch.md | test-plan/render-dispatch.md |
| Consumer reconciliation (5-site atomic unit) | `uni-retro/SKILL.md`, tool description, `uni-delivery-protocol.md`, `uni-bugfix-protocol.md` | pseudocode/consumer-reconciliation.md | test-plan/consumer-reconciliation.md |
| Retro-lifecycle restructure (BOTH protocols) | `.claude/protocols/uni/uni-delivery-protocol.md`, `.claude/protocols/uni/uni-bugfix-protocol.md` | pseudocode/retro-lifecycle.md | test-plan/retro-lifecycle.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

`context_cycle_review` today appends the full crt-052 raw candidate payload to every response (pushing the
`markdown` default to ~75 KB, ~88% raw bytes, breaking vnc-011 AC#10's ≥80% token reduction) and purges the
in-memory transcript buffers on the first successful review (`purge_cycle_transcripts` at the four success
returns), permanently destroying the only source of verbatim candidates. crt-057 restores a lean, non-destructive
default and makes the review **fully non-destructive** — the eager purge is removed with **no purge verb of any
kind**, reclamation delegated entirely to the unchanged backstops (24h TTL / 64-cap / session-close). The
transcript payload becomes a **scoped, read-only, AND-composed retrieval** (`phase`/`anchor`/`match`/`window`)
with per-session loss propagation and server-side cross-plane clock normalization, so a caller queries in its own
units and never receives a silent false negative. The tool exposes three mutually orthogonal, non-destructive axes:
render (`format`), recompute (`force`), scoped retrieval (`transcript`).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Transcript axis shape | **Read-only scoped `transcript: { phase?, anchor?, match?, window? }`** (all-optional, AND-composed) over the EXISTING candidate pipeline via `snapshot()`; NOT a boolean, NOT destructive | ass-091 Q3, Goal §3 | ADR-002, ADR-006 |
| Purge verb | **REMOVED entirely** — no flag, no default. The four `purge_cycle_transcripts` calls deleted; `context_cycle_review` gains zero destructive capability (NG-6) | Goal §2 | ADR-001 |
| Reclamation | Delegated **entirely** to the unchanged backstops (24h TTL sweep, 64-cap eviction, per-turn session-close); no new cycle-close purge trigger | NG-2, C-4 | ADR-001 |
| Memory-residency posture | Every path leaves the buffer intact; raw bytes reside until a backstop reclaims, bounded by UNCHANGED 64-cap + 24h TTL; disk posture unchanged (NG-1); human-ratified | Accepted Residency Trade-off | ADR-001 |
| Loss propagation | Every returned session carries `SessionLossInfo`; `search_complete == false` **iff** `elided_bytes>0 ∥ has_holes ∥ provenance==Reconstructed ∥ dropped_candidates>0`; a no-match over `search_complete==false` is **INDETERMINATE**, never a bare false | Goal §4 | ADR-003 |
| Clock normalization | Server parses candidate `ts` to a canonical epoch, joins over a WINDOW (never exact), `byte_offset` fallback for `ts:None`; caller never sees Plane B's clock | Goal §5 | ADR-006 |
| Window default (OQ-2) | **±120 000 ms (±2 min)** for ts-bearing candidates; **±3 candidate blocks** by `byte_offset` for `ts:None`; caller-overridable, cap-bounded; over-inclusion is the safe direction | OQ-2 | ADR-006 |
| Four-site seam | Now gates **ONLY the content-opaque fold read** (crt-054/055) — the sole surviving success side-effect; the ×4 purge-count / attach-before-purge source assertions are **deliberately removed with rationale** | C-1 | ADR-004 |
| `"summary"` alias | **DROP** (not fold) → `ERROR_INVALID_PARAMS` at all four render loci. Breaking for any live `format:"summary"` caller — delivery must sweep consumers | D-1 / OQ | ADR-002 |
| ADR amendment mechanism | `context_correct` on #4742 and #4857 (NOT deprecate+store) — preserves provenance | SR-13 | ADR-001 |
| Retro lifecycle (both protocols) | pr-review/bug-review phase kept OPEN through the human merge decision; cycle closed only AFTER merge (`phase-end` then `stop`); `/uni-retro` post-close. Strict ordering **merge → close → retro** | D-7 | ADR-005 |
| Cycle-close is non-purging | Code-traced: `context_cycle(stop)` drains only the retrospective queue + writes an audit row; touches no buffer. merge→close→retro composes only because close is inert | ADR-005 | ADR-005 |
| `distill_before_purge` rename | **OQ-4 delivery decision** — keep the vestigial name (preserves counted strings, minimal churn) OR rename with a deliberate source-assertion-test update. Flagged; not blocking | OQ-4 | ADR-002/§12 |

---

## Files to Create / Modify

**Server (D-1..D-4) — the atomic unit's code half:**
- `unimatrix-server/src/mcp/tools.rs` — `RetrospectiveParams` (:~431): remove `include_transcript_candidates`,
  add `#[serde(default)] transcript: Option<TranscriptScope>`; thread the scope through the four success returns;
  **delete the four `purge_cycle_transcripts` calls** (:2379, :2558, :3328, :3451); drop the `"summary"` render arm
  at four dispatch loci (:2532, :3359, :4268, :4324).
- `unimatrix-server/src/mcp/distill_handler.rs` — `distill_before_purge` (:48): add `scope: Option<&TranscriptScope>`
  + `reviewer_session_id: Option<&str>`; return `None` when `scope` is `None` (no buffer read); apply
  `phase`/`anchor`/`match`/`window` filters + clock normalization; emit per-session `SessionLossInfo`. Extend existing
  fixtures + source-assertion tests (:651-726) — **remove the ×4 purge-count assertion with rationale; preserve the
  fold-read four-site assertion**.
- `unimatrix-observe/src/types.rs` — `TranscriptScope` + `Window` `[NEW]`; `SessionLossInfo` / `CandidateProvenance` /
  `TranscriptCandidate` / `TranscriptCandidatesSection` `[UNCHANGED]` (`:611-663`); derive `search_complete`
  response-transient, OUTSIDE `RetrospectiveReport`.
- `unimatrix-server/src/server.rs` — **DELETE** `purge_cycle_transcripts` (:661) + its now-orphaned helpers
  `clear_transcripts_for_feature` / `purge_held_for_feature`; re-home the exhaustive `TranscriptRetention` match onto
  the surviving backstop reclaim paths (`RetainDays` a no-op, no `_` arm).
- `unimatrix-server/src/infra/transcript_hold.rs` — backstops UNCHANGED; confirm they carry the full reclamation load
  and emit the content-free terminal audit.

**Consumer reconciliation (D-6) — the atomic unit's docs half (must ship together — see Constraints):**
- `.claude/skills/uni-retro/SKILL.md` — the candidate-bearing call uses the `transcript: {}` full-candidate block
  (NOT the old boolean); the retro AGENT owns synthesis (Ownership Boundary); retrieval is repeatable / non-destructive.
- `context_cycle_review` **tool description** — document the three orthogonal axes (`format` render-only; `force`
  recompute-only; `transcript{}` read-only scoped retrieval returning candidates + `SessionLossInfo`); **state plainly
  the tool has no purge verb**.
- **`uni-agent-routing.md` is EXCLUDED** — a passing descriptive mention in an overview doc no live protocol loads; it
  sets no flag and carries no candidate behavior. Do NOT reconcile it; a grep guard that greps it fails spuriously.

**Retro-lifecycle restructure (D-7) — BOTH protocols, part of the same atomic unit:**
- `.claude/protocols/uni/uni-delivery-protocol.md` (pr-review phase) — keep the phase OPEN through the human merge
  decision; move `context_cycle(phase-end)`+`context_cycle(stop)` to AFTER merge; add `/uni-retro` post-close. Order
  **merge → close → retro**.
- `.claude/protocols/uni/uni-bugfix-protocol.md` (bug-review phase) — same restructure; add `/uni-retro` post-close.

**ADR (D-5):**
- Amend #4742 and #4857 in Unimatrix via `context_correct` (five points: purge removed / fully non-destructive;
  residency bounded by unchanged cap+TTL; disk posture unchanged; `force`/`format` orthogonal; fold read the sole
  surviving side-effect). ADR-001 is the authoritative amendment record; ADR-005 records the retro-lifecycle restructure.

---

## Key Signatures

```
// RetrospectiveParams (tools.rs:~431)
#[serde(default)] pub transcript: Option<TranscriptScope>   // NEW; omit = summary only
pub format: Option<String>   // "markdown" | "json"; "summary" DROPPED → ERROR_INVALID_PARAMS
pub force:  Option<bool>     // UNCHANGED (None ≡ false)
// REMOVED: pub include_transcript_candidates: bool

// TranscriptScope  [NEW] — all optional, AND-composed
struct TranscriptScope {
    phase:  Option<String>,   // phase id → cycle_events bounds; self-bounding (ignores window)
    anchor: Option<String>,   // finding/anchor id → HotspotFinding.evidence[].ts span
    r#match: Option<String>,  // regex over whole TranscriptCandidate.text; #[serde(rename="match")]
    window: Option<Window>,   // ±N events / ±T millis; default ±120_000 ms / ±3 blocks
}

// distill_before_purge (distill_handler.rs:48) — NEW signature; name vestigial (no purge follows)
fn distill_before_purge(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
    scope: Option<&TranscriptScope>,      // NEW — returns None early when None
    reviewer_session_id: Option<&str>,    // NEW
) -> Option<TranscriptCandidatesSection>

// attach_to_response_assembly (distill_handler.rs:281) — UNCHANGED; no-ops on None/Err
// snapshot() (session_transcript.rs:296)  — UNCHANGED &self; SOLE content reader, no new reader (#4848)

// DELETED (orphaned): purge_cycle_transcripts (server.rs:661),
//   clear_transcripts_for_feature (session.rs), purge_held_for_feature (transcript_hold.rs:331)
```

**Source-assertion invariant (distill_handler.rs:651-726):** the `distill_before_purge(` /
`attach_to_response_assembly(` ×4 counts STAND; the **`self.purge_cycle_transcripts(&feature_cycle)` ×4 assertion is
REMOVED** with an in-source rationale comment (purge gone). The content-opaque fold-read ×4 assertion is PRESERVED.

---

## Data Structures

- **Report source = durable observations.** `build_report(...)` (`unimatrix-observe/src/report.rs:15-53`) takes **no
  transcript argument**. Report content is byte-identical whether the buffer is present, partial, or gone (A-1,
  re-verified ARCH §5). This is the enabling fact for render/force ⊥ retrieval — summary prose ⟂ Plane B content.
- **Candidates source = in-memory buffer (read-only, survives review).** Read via `snapshot()` (`session_transcript.rs:296`,
  `&self`), the single content reader. Scanned by the `feature_cycle` string, independent of cycle open/closed state
  (grounds the post-close retro). Once a backstop reclaims → empty or `Reconstructed`-only.
- **`TranscriptCandidatesSection { candidates, loss }`** — attached out-of-band at assembly level, NEVER onto
  `RetrospectiveReport` (#4850). `search_complete` derived per-session at render, response-transient (AC-14).
- **`SessionLossInfo { session_id, elided_bytes, has_holes, provenance, dropped_candidates }`** (`observe/types.rs:633-646`,
  UNCHANGED) — the discriminator for `search_complete`.
- **`TranscriptCandidate { session_id, byte_offset, ts: Option<String>, family_hints, text }`** (`observe/types.rs:611-624`,
  UNCHANGED) — `match` runs over whole `text`; `ts` parsed to canonical epoch; `byte_offset` is the `ts:None` fallback key.
- **Content-opaque fold** — `activity_snapshots_for_feature` (`session.rs:566`) yields counters only (`bytes_total`,
  `*_delta_count`, `class_counts`), persisted to `cycle_review_index`. Must stay idempotent across repeated
  non-purging reviews (R-14/SR-12).

---

## Constraints

- **CON-1 (5-site atomic unit, SR-04/R-02 — dominant scope risk, Critical):** Server (D-1..D-4) + `uni-retro/SKILL.md`
  + the `context_cycle_review` tool description + BOTH protocol files ship as ONE indivisible deliverable. **No
  partial-ship story** — server alone silently starves every candidate consumer (harvest #5219) with no error.
  `uni-agent-routing.md` is EXCLUDED. AC-16 fails if any consumer implies old boolean/purge semantics; AC-17 fails if
  either protocol omits merge → close → retro.
- **CON-2 (four-site lockstep now guards only the fold read, C-1/#4750/R-07):** the surviving fold-read gate is
  expressed once and applied identically at all four success returns; the memo-hit site (site 3) is the highest-drift
  risk and is behaviorally, not source-assertably, enforced. The purge-count / attach-before-purge assertions are
  **deliberately removed with recorded rationale (R-11) — not silently.**
- **CON-3 (single buffer reader, C-2/#4848):** scoped retrieval reuses the existing `snapshot()` (`&self`); no new reader.
- **CON-4 (secrets / retention invariants, C-3/C-5):** never-persist-raw-transcript-to-disk (#4721/#4850/#4742)
  absolute; candidates + loss fields response-transient outside the memoized struct; reclamation audits content-free;
  the exhaustive `TranscriptRetention` match re-homed onto the backstops with `RetainDays` a no-op and no `_` arm.
- **CON-5 (`"summary"` DROP, C/SR-07):** `format` accepts exactly `markdown|json`; unknown → `ERROR_INVALID_PARAMS`
  (`Valid values: "markdown", "json"`). No third render path survives.
- **CON-6 (rebase awareness, C-8/SR-12):** prior work (bugfix-891 / bugfix-824) touched `distill_handler.rs`.
  **Delivery leader MUST confirm no live rebase conflict before delivery** (a pre-flight, not a test).
- **CON-7 (hygiene, C-7):** 500-line file limit, `fmt`, `clippy`; extend existing `distill_handler.rs` fixtures — no
  isolated scaffolding.
- **Test-construction constraints (whole matrix):** every negative assertion ("no purge"/"no candidates"/"buffer
  intact") keys on **synchronous observable state** (buffer still present; spy at a synchronization point), never the
  absence of an async audit (R-10, #4879); every path-specific row proves which of the four returns executed (#4452);
  clock/window tests use explicit fixed offsets + on/inside/outside boundaries (#4195/#4236), never `now_ts()`, and
  the join is windowed never exact; AC#10 uses a populated fixture + ratio, guarded against the empty-buffer vacuous
  pass (#3548); the lifecycle + consumer changes are verified end-to-end and **per protocol** (delivery AND bugfix) —
  a server-only or single-protocol green suite does not satisfy the gate (#5383 reachability, not store read-back).

---

## Delivery-Critical Coordination / Blast-Radius Notes

- **ANTI-STUB orphan deletion (R-06 / SR-10, CLAUDE.md rule 2).** `purge_cycle_transcripts` (server.rs:661) +
  `clear_transcripts_for_feature` + `purge_held_for_feature` lose **all non-test callers** once the four review-site
  calls are removed. They MUST be **deleted** (dead-code / clippy) — not `#[allow]`ed. The exhaustive
  `TranscriptRetention` match lived **inside** the deleted purge; delivery MUST **re-home it onto the surviving
  backstop reclaim paths** (`RetainDays` a no-op, no `_` arm) — the C-5 obligation relocates, it does not disappear.
  Backstops are now the SOLE reclamation path; a broken one means unbounded residency (secrets) with no visible failure.
- **Four-site #4750 seam now gates ONLY the content-opaque fold read (R-07).** After the purge leaves the seam the
  fold is the only surviving success side-effect; missing a site (esp. memo-hit, site 3) under-counts durable,
  non-`force`-reproducible integers (#4585 drift). The **×4 purge-count / attach-before-purge source assertions are
  deliberately removed (R-11) — recorded with an in-source rationale, never silent**; the fold-read ×4 assertion stays.
- **Silent false negative is the TOP Critical risk (R-01) — the feature's raison d'être.** Loss propagation must be
  proven **per loss condition**: `search_complete==false` for each of `elided_bytes>0`, `has_holes`, `Reconstructed`,
  `dropped_candidates>0`, and their OR-combination, per session; a clean `Primary` session is a trustworthy negative
  (may be omitted); no `match` path may return a bare `matched` without its `SessionLossInfo`.
- **Clock normalization correctness (R-05).** Plane A epoch-millis ↔ Plane B JSONL `ts` skew: windowed (never exact)
  join, `byte_offset` fallback for `ts:None`, ±120 s/±3-block default. Route through a named boundary conversion helper
  (#3385/#3372); explicit-offset boundary tests (#4195/#4236).
- **Scoped-filter correctness (R-09).** AND-composition narrows (intersection, not union); `transcript:{}` ≡
  `match:".*"` full dump under the cap; omit = summary-only/non-destructive; `window` ignored by self-bounding `phase`;
  empty scope → absent (not null); invalid `match` regex → `ERROR_INVALID_PARAMS` (flag ReDoS surface to delivery).
- **Residency lengthening / no-new-persistence (R-03, Critical).** Every path now leaves the buffer intact longer;
  the reclamation-without-review path is the least-tested, highest-leak-suspect. Content-scan every SQL/file/log sink
  on ALL changed paths INCLUDING reclamation-without-review; the loss carrier (`SessionLossInfo`/`search_complete`) is
  explicitly in scope and must stay response-transient.
- **5-site consumer atomic unit + two-protocol lifecycle (R-02/R-04, all-session blast radius).** The atomic unit is
  server + `uni-retro/SKILL.md` + tool description + `uni-delivery-protocol.md` + `uni-bugfix-protocol.md`. The
  lifecycle restructure rewires the close-of-cycle harvest for **every future delivery and bugfix session** — a
  mis-wire (stop-before-merge, or omit/misorder post-close `/uni-retro`) silently breaks attribution/verbatim harvest
  feature-wide. merge→close→retro composes ONLY because `context_cycle(stop)` is non-purging (R-08/ADR-005) — guard it.
- **Delivery pre-flight checklist:**
  - **SR-12 bugfix-891 rebase check** — confirm no live conflict on `distill_handler.rs` before delivery.
  - **`distill_before_purge` vestigial-name decision (OQ-4)** — keep the name (minimal churn, preserves counted
    strings) or rename with a deliberate source-assertion-test update. Not blocking; decide before Stage 3b.
  - **`"summary"` alias DROP consumer sweep (R-12).** DROP is breaking. Sweep reconciled consumers for a live
    `format:"summary"` caller; if one surfaces, reconsider fold-to-markdown (the non-breaking delegated option).

---

## Dependencies

- **ass-091 (#898)** — the design source. Q1 data-plane map (authoritative; do not re-derive), Q3 scoped-retrieval
  mechanism (the crt-057 blocker, resolved), ★ headline + ★ non-destructive design note.
- **crt-052 (#706)** — transcript_hold / candidates pipeline + `snapshot()` (single content reader); amends its ADR-008
  purge clause (#4857).
- **crt-054/#5030, crt-055/#5042** — content-opaque integer fold at the review seam (the sole surviving side-effect;
  must stay correct/idempotent now the review never purges).
- **vnc-024 / vnc-025** — retention enum + purge-point content-free audit; amends vnc-025 ADR-004 (#4742).
- **vnc-011 (#196)** — origin of `format`/markdown and AC#10 (restored token-reduction target).
- **ass-090 (#896)** — DOWNSTREAM spike, re-sequenced to depend on ass-091; consumes the Q1 map and extends the fold;
  does not touch Plane B raw content (NG-7). NOT in scope.
- **crt-056 `BackgroundJob` registry (#5167)** — documented seam for a future async transcript-summary job (Q4); left
  open, built by no one here (NG-8).
- **Patterns:** #4750 (four success returns, shared-helper gating — now guards only the fold read), #4850 (candidates
  attached at response-assembly level), #4848 (single content reader). **Amend via `context_correct`:** #4742, #4857.
  Unchanged anchors: #4721, #4850. **Superseded:** prior boolean-contract ADRs #5429/#5422/#5423/#5424 (reworked/deprecated).
- Crates touched: `unimatrix-server`, `unimatrix-observe`. No new external crates.

---

## NOT in Scope

- **NG-1:** persisting raw transcript content to disk in any form — absolute (#4721/#4850/#4742). Touches only in-memory
  purge *timing*.
- **NG-2:** changing the 64-cap, 24h TTL sweep, or per-turn session-close — the backstops stay as-is (sole reclamation).
- **NG-3:** changing distilled-knowledge / observation / audit retention.
- **NG-4:** adding a content secret-scanner / redactor — accept-and-drop + in-memory + system-purge IS the guarantee.
- **NG-5:** cross-plane synthesis, attribution, or human-ledger surfacing (agent-owned retro) — no GH-block join, no
  applied-entry attribution, no rework-count↔cause join, no human-intervention ledger (asserted FR-25 / AC-19).
- **NG-6:** a purge verb on `context_cycle_review` — no `purge:true`, no destructive default. Operator-triggered
  reclamation, if ever needed, is a separate admin/ops verb.
- **NG-7:** distilling transcript signal INTO the review summary — deferred to ass-090 (#896). ZERO in-summary
  distillation scenarios; any AC enriching the report body is a variance.
- **NG-8:** local inference over the transcript (Q4) — feasibility-only; leave the crt-056 seam, build nothing.
- Fixing per-turn drain starvation / mid-session amnesia (#4799); changing the human merge gate (unchanged).

---

## Alignment Status

Vision Guardian verdict (ALIGNMENT-REPORT.md, refreshed 2026-07-04 for the ass-091 redesign):
**PASS 5, WARN 1, VARIANCE 0, FAIL 0.** No variance requires new human approval.

- **Vision / Milestone / Scope-Gaps / Architecture-Consistency / Risk-Completeness: PASS.** Principle 8 ("no secrets
  in any database") / NG-1 upheld and *strengthened* — disk posture unchanged, no candidate slot on the memoized
  report, content-free audits now at the backstop. Advances self-learning (#5219): scoped retrieval hands the retro
  agent targeted WHAT-transpired tools; D-6/D-7 wire the harvest into both protocols. Every SCOPE deliverable (D-1..D-8)
  and AC (AC-01..AC-17) is represented; SCOPE AC-01..AC-17 map 1:1 to SPEC AC-01..AC-17.
- **WARN — Scope Additions.** All architect-delegated resolutions of SCOPE open questions: window default ±120 s/±3
  blocks (OQ-2), `Window`/`r#match` shape (OQ-3), SPEC AC-18/AC-19 grounding OQ-2/NG-5. **The `"summary"` alias DROP is
  the one breaking choice** — delivery MUST run the consumer sweep (R-12) before ship; if a live caller surfaces,
  reconsider fold-to-markdown.
- **Awareness-only (already ratified, no approval needed):** the memory-residency lengthening — now on EVERY path — is
  memory-only, bounded by the unchanged cap/TTL/session-close (behaviorally identical to "no review has run"), and
  human-ratified in SCOPE "Accepted Residency Trade-off" + ADR-001 (b) via `context_correct` on #4742/#4857.
