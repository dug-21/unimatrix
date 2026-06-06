# Gate 3a Report: vnc-025

> Gate: 3a (Design Review)
> Date: 2026-06-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components match ARCHITECTURE.md decomposition; ADR-001..008 faithfully carried into pseudocode; all line refs spot-verified against main |
| Specification coverage | PASS | FR-01..FR-21, NFR-01..NFR-09 all traced to pseudocode; no scope additions (retention_config field is required FR-16 wiring, transparently flagged) |
| Risk coverage | PASS | R-01..R-15 all mapped in test-plan/OVERVIEW.md with hard gates named; every edge case and integration risk from RISK-TEST-STRATEGY.md has a planned test |
| Interface consistency | PASS (1 WARN) | Shared types/signatures identical across OVERVIEW.md and all 7 component files; one server.rs:335 contradiction between config-knob pseudocode and test plan (resolved below) |
| Knowledge stewardship | PASS | architect (Queried + Stored #4739–#4746), risk-strategist (Queried + declined with reason), pseudocode (Queried), testplan (Queried + declined with reason) |
| Open questions (6) | RESOLVED | All six resolvable within the validated design — dispositions below; none require rework or scope change |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**: Component table in pseudocode/OVERVIEW.md matches ARCHITECTURE.md Component Breakdown one-to-one. Spot-verified against main:
- `listener.rs:1009` filter line exists exactly as ADR-003 describes; dispatch-wiring Edit 2 tees before it without touching it.
- ADR-008 two-layer policy reproduced exactly in transcript-buffer.md (`checked_add` drop-whole, no `high_water` update on overflow, `into_inner()` + `clear()` at every lock site) and in registry-wiring.md's `lock_buffer` helper.
- ADR-001 lock discipline (registry lock: lookup + Arc clone + scalar bump only; memcpy under buffer lock) is verbatim in `apply_transcript_delta` pseudocode.
- ADR-004 collect-under-lock/emit-after-release and the pinned audit event shape are identical in purge-audit.md, cycle-review-purge.md, and OVERVIEW.md.
- ADR-005 move inventory (constants at `hook.rs:39/:50`, functions at `:1113/:1205/:1361/:1383/:1442`) verified — constants and named tests exist at the cited locations.
- ADR-006/ADR-007 carried correctly (ctor injection; `session_key` degenerate-but-load-bearing with the required doc comment).
- Architecture line refs verified: `tools.rs:1918` (context_cycle_review), `server.rs:508` (audit_fire_and_forget — exact tokio::spawn + log_event_async shape claimed), `listener.rs:1796/:1814` (sweep/drain call sites), `session.rs:475/:501` (drain/sweep signatures).

### 2. Specification coverage
**Status**: PASS
**Evidence**: FR-01..05 → transcript-buffer.md; FR-06..09 → dispatch-wiring.md Edits 1–2 (early-return shape preserved, no re-sanitize, HTTP unchanged); FR-10/11 → config-knob.md + buffer ring-tail; FR-12..14 → registry-wiring + purge-audit (silently-evicted case explicitly handled in `sweep_stale_sessions` pseudocode); FR-15/16 → cycle-review-purge.md (exhaustive match, no `_` arm, RetainDays arm present and non-purging); FR-17..19 → dispatch-wiring Edit 3 + transcript-block.md (prepend before token_count, tail-contiguity, no elision marker); FR-20 → `session_key`; FR-21 satisfied by absence (no pseudocode touches `transcript_excerpt` — the "remains ignored" requirement is met by not adding a reader; implementer must not add one).
NFR-09's full contract (drop-whole, no partial clip, I5 conversion invariant comments, fuzz verification) is reproduced with the ADR-008 "do NOT improve into partial-clip" warning intact. No unrequested features found; the one new wiring surface (`UnimatrixServer.retention_config`) is required by FR-16 and follows the #561 precedent.

### 3. Risk coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW.md maps all 15 risks to plan files with hard gates: R-01 permutation harness + 4 hole-surgery classes; R-02 fuzz no-panic (named NFR-09 verification); R-03 tail-window equivalence (correctly NOT strengthened to full-content); R-04 vnc-024 zero-rows suite unmodified (5 test names pinned) + mixed-batch row-content; R-05 sentinel + static grep (both, per strategy); R-06 poisoned-mutex mandatory test; R-07 #4379 emission context + burst + failure independence; R-08 silently-evicted mandatory named case; R-09 golden parity + empty-buffer byte-identity (both hard gates); R-10 attribution matrix + post-clear pinning; R-11 scenario-5 cap chain; R-12 #4725 transform tests; R-13 budget bound + document-and-accept; R-14 22-name pre-move inventory captured + constant pins; R-15 collapse-at-65. All strategy Edge Cases (zero-length delta, invalid UTF-8, cap off-by-one, window 0/hole-boundary, re-registration, sweep×cycle-review race) have named tests. Integration harness plan correctly scopes to smoke/tools/protocol/lifecycle with the Rust-test-only coverage gap explicitly documented for RISK-COVERAGE-REPORT.

### 4. Interface consistency
**Status**: PASS (1 WARN)
**Evidence**: `TranscriptBuffer` field set, `TranscriptPurgeRecord`, `session_key`, `apply_transcript_delta(&self, &str, u64, &[u8])`, `contiguous_tail(usize) -> Option<Vec<u8>>`, `clear() -> u64`, drain `Option<(SignalOutput, Option<TranscriptPurgeRecord>)>`, sweep `(Vec<SweepResult>, Vec<TranscriptPurgeRecord>)`, `clear_transcripts_for_feature -> Vec<TranscriptPurgeRecord>`, and the pinned audit event shape are character-identical across OVERVIEW.md, all component files, and the ARCHITECTURE Integration Surface table.
**WARN (W1)**: config-knob **pseudocode** says keep `SessionRegistry::new()` at `server.rs:335` (test ctor), while config-knob **test plan** §3 grep gate says "all three production construction sites (server.rs:335, main.rs:645/:1068) call with_transcript_cap". These contradict. Resolution with code evidence below (OQ-3) — the test-plan grep gate must be corrected at Stage 3b/3c to expect `with_transcript_cap` at `main.rs:645/:1068` only.

### 5. Knowledge stewardship
**Status**: PASS
**Evidence**: architect report — Queried (context_briefing, context_lookup) + Stored #4739–#4746 (8 ADRs). risk-strategist report — Queried (4 searches, all cited) + "Stored: nothing novel to store" with reason (recurring patterns already captured; poisoned-mutex concern single-feature). pseudocode report (read-only agent) — Queried (context_briefing; #4365 applied to FR-16 pin). testplan report — Queried + "Stored: nothing novel" with reason (all techniques are existing patterns, applied not evolved). All blocks present, all reasons given.

## Open Question Dispositions (requested by spawn prompt)

| # | Question | Disposition |
|---|----------|-------------|
| OQ-1 | `register_session` overwrite purges live transcript (pseudocode OVERVIEW #1) | **Resolvable — no rework.** The premise is partially stale: the `cycle_start` path (`listener.rs:2425-2443`, GH #519) registers **only when the session is absent** — the guard comment explicitly states "register_session overwrites and would reset … accumulated state for live sessions". A live session's transcript is therefore NOT wiped at cycle_start. The only overwrite path is an explicit `SessionRegister` request (`listener.rs:569`) from a reconnecting client — which is exactly the architecture's pinned "re-registration after drain: fresh empty buffer" case. Keep the simple overwrite. Record one line in the F3 contract notes: a client that re-sends SessionRegister mid-stream resets its own transcript (self-inflicted, unaudited — acceptable, ships dark). Implementer should fix the stale premise in pseudocode/OVERVIEW.md at Stage 3b. |
| OQ-2 | Purge only on successful review (pseudocode OVERVIEW #2) | **Resolvable — purge-on-success-only is correct and approved.** Spec W4 sequences purge as a step of a review that "runs" to completion; purging on error paths would destroy transcripts on a failed review for zero benefit and contradicts cycle-review-purge.md's own pin ("review failed ⇒ transcripts stay for the retry"). Consistent with SR-09's degradation posture and FR-15's "review output otherwise unchanged". Already pinned by test scenario 7 in test-plan/cycle-review-purge.md. |
| OQ-3 | `server.rs:335` as a `with_transcript_cap` switch site (pseudocode OVERVIEW #3) | **Resolved with code evidence.** `server.rs:335` is the test-server ctor (`session_registry: Arc::new(SessionRegistry::new())`); production daemon/stdio paths construct their own registry at `main.rs:645/:1068` and **overwrite** the server field at `main.rs:752/:1174` ("Share … session_registry with the MCP server (col-009)"). Sibling fields at the same ctor carry the identical "default for test server; overwritten in main.rs daemon/stdio paths" comment. Therefore: switch `main.rs:645/:1068` to `with_transcript_cap(...)`; keep `new()` at `server.rs:335`. The IMPLEMENTATION-BRIEF's three-site list is inaccurate on this point; the config-knob **test-plan grep gate must be reworded** (W1) to: "both main.rs production sites use with_transcript_cap; server.rs:335 test ctor keeps new(); no production-constructed registry uses new()". |
| OQ-4 | `UnimatrixServer.retention_config` wiring (pseudocode OVERVIEW #4) | **Resolvable — approved as required wiring, not scope creep.** FR-16 requires the handler to read `transcript_retention` at runtime; `UnimatrixServer` does not hold retention config today. The #561 `store_config` precedent is real (verified at the server.rs ctor) and the pseudocode follows it exactly. Architecture file-list omission is documentation-only; no design conflict. |
| OQ-5 | Snapshot-baseline sequencing (testplan OQ #1/#3) | **Resolvable — sequencing instruction to Stage 3b.** Three pre-change baselines are needed: (a) empty-buffer `CompactPayload` response (R-09.4 hard gate), (b) `context_cycle_review` output snapshot (AC-09), (c) `SignalOutput` serialization fixture. The Stage 3b developer MUST capture these fixtures from pre-change code (first commit, or generated from main) before touching `listener.rs`/`tools.rs`/`session.rs`. The SM should order Stage 3b work accordingly; failure to do so loses the baseline and turns two hard gates into hand-written expectations (#2984 anti-pattern). |
| OQ-6 | FR-16 compile-gate verification (testplan OQ #2) | **Resolvable — compile gate acceptable, with one strengthening.** FR-16's own requirement IS the exhaustive match — compile-level enforcement is exactly what the spec demands; falling back to it is compliant. Strengthening: `RetentionConfig` fields are `pub` and `RetainDays(u32)` is a constructible variant, so a unit test CAN inject `transcript_retention: RetainDays(30)` directly into a registry/handler fixture without weakening `validate()` — the test plan's "if constructible" branch is available and should be attempted first; the review gate remains the floor. |

## Rework Required

None blocking. Three WARN-level corrections to fold into Stage 3b (no re-validation needed):

| Item | Owner | Fix |
|------|-------|-----|
| W1 | rust-dev / tester (Stage 3b/3c) | Correct config-knob test-plan §3 grep gate per OQ-3 disposition: `with_transcript_cap` at `main.rs:645/:1068` only; `server.rs:335` keeps `new()` |
| W2 | rust-dev (Stage 3b) | Update pseudocode/OVERVIEW.md open question 1: cycle_start path is already guarded (GH #519); only explicit SessionRegister overwrites — add the F3 contract note |
| W3 | SM (Stage 3b sequencing) | Land pre-change snapshot fixtures (CompactPayload empty-buffer, cycle-review output, SignalOutput serialization) BEFORE implementation edits per OQ-5 |

## Scope Concerns

None. No FAIL indicates wrong scope, unworkable technology, or an architecture unable to support a requirement.
