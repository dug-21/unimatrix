# Gate 3a Report: crt-052

> Gate: 3a (Component Design Review)
> Date: 2026-06-08
> Result: PASS (with 4 WARNs requiring architect/leader sign-off on contract additions)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment (C1..C10 vs ARCH §2/§4) | PASS | All 10 components map to ARCH §2 rows; signatures match ARCH §4 verbatim except 4 flagged, ADR-grounded additions |
| Specification coverage (FR-1..15, NFR-1..9) | PASS | Every FR has corresponding pseudocode; no scope additions |
| Risk coverage (R-01..R-20 mapped) | PASS | All 20 risks mapped to test scenarios in OVERVIEW + per-component plans |
| AC coverage (AC-01..13, AC-V-SEAM, AC-V-FUZZ) | PASS | All 15 ACs mapped in test-plan/OVERVIEW AC→Test table and per-component plans |
| Interface consistency (shared types) | WARN | One contradiction: C4 pseudocode vs C4 test plan on `TranscriptCandidate` Debug |
| Wave A/B dependency separation (R-11) | PASS | Severable hold handle; explicit per-file `transcript_hold.rs` reference status; dependency-direction tests planned |
| Lock discipline (Constraint 1, AC-01) | PASS | Two-phase encoded; byte-copy is only under-lock content work; no parse/match under any lock |
| Six merge gates each have a concrete test plan | PASS | AC-11, AC-06, AC-05, AC-V-FUZZ, AC-V-SEAM, AC-01 all covered |
| Knowledge stewardship compliance (design agents) | PASS | pseudocode (read-only) Queried+no-store; spec Queried+reason; risk Queried+reason |

## Verdicts on the Five Flagged Contract Ambiguities

| # | Ambiguity | Verdict | Basis |
|---|-----------|---------|-------|
| 1 | `SessionLossInfo.dropped_candidates` content-free count added (not in ARCH §4) | **SOUND — AC-grounded; architect must confirm** | AC-08 mandates cap-forced truncation be surfaced ("no silent aggregate-cap drop"). ARCH §4 / ADR-007 `SessionLossInfo` omit any drop field, so the contract as written cannot satisfy AC-08. The added field is content-free (a count), rides the same response-transient never-persisted path, and is the minimal surfacing. Correctly flagged as an additive deviation from the binding §4 table. **WARN** (needs architect ratification, not a defect). |
| 2 | `select_candidates` returns only `Vec<TranscriptCandidate>`; C6 re-derives per-session drop count | **SOUND** | Preserves the binding ARCH §4 / brief signature `-> Vec<TranscriptCandidate>` exactly. Re-derivation is deterministic (C6 holds the same `bytes` + `session_cap`; pre-cap vs post-cap count). No defect. |
| 3 | `hold_on_drain`/`readopt` use the 3-arg `feature_cycle` form vs ARCH §4 short form | **SOUND for `hold_on_drain`; `readopt` 2-arg exceeds both §4 and ADR-008 literal — architect must confirm** | SR-02 (loud re-adopt on cycle MATCH), AC-11(b), R-01 are impossible without `feature_cycle` available at both hold and re-adopt. ADR-008 §Decision lists `hold_on_drain(session_id, arc, feature_cycle)` (3-arg) — the pseudocode's `hold_on_drain` matches the binding ADR exactly; ARCH §4's 2-arg short form is the variance and the ADR governs. However ADR-008 §Decision lists `readopt(session_id) -> Option<...>` (1-arg) — the pseudocode's `readopt(session_id, registering_feature_cycle)` exceeds BOTH ARCH §4 AND ADR-008's literal signature. The reasoning (caller must pass the re-registering cycle for the match) is correct and unavoidable for SR-02, but it is a genuine signature deviation. Correctly flagged. **WARN** (architect should ratify the `readopt` arity in ADR-008). |
| 4 | Fallback hole-fraction threshold proposed as config knob `transcript_fallback_hole_fraction` (not in brief 4-knob table) | **SOUND — ADR-grounded; architect must confirm knob-vs-constant** | ADR-006 §Decision explicitly names "holes covering more than a **configured fraction** of the span" and §Consequences states "the threshold fraction is a tuning parameter that must be boundary-tested." The brief's 4-knob table omits it, but the brief states values are "starting defaults, config-tunable." The pseudocode correctly flags it as additive and asks whether it is a knob or a compile-time constant. **WARN** (architect picks knob vs const; either satisfies ADR-006). |
| 5 | Snapshot field-set follows ARCH §4 (`holes: Vec<HoleInfo>`, `session_id` as seam tuple key) over SPEC's `session_id`/`hole_info` | **SOUND — correct resolution** | The brief's "where ARCH §4 and this brief agree, ARCH §4 is the source of truth" and the Naming Pin make ARCH §4 binding over SPEC §Domain Models. ADR-002 confirms the field set (`bytes, base_offset, high_water, elided_bytes, holes: Vec<HoleInfo>`) and that `session_id` is the tuple key in the seam return, not a snapshot field. No defect; the divergence is correctly resolved in favor of the binding doc. |

None of the five constitute a design defect. Four (1, 3, 4) need a one-line architect ratification because they add to / depart from the binding ARCH §4 table; the pseudocode already flagged each in the agent report. These are exactly the kind of contract additions the validator should not silently bless — hence WARN, not PASS-silent. They do not block Stage 3b: each addition is content-free or signature-only, ADR-justified, and reversible.

## Detailed Findings

### Architecture alignment
**Status**: PASS
**Evidence**: OVERVIEW.md Components table maps C1..C10 1:1 to ARCH §2. Signatures match ARCH §4 / brief "Function Signatures" verbatim: `take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>` (snapshot-seam.md L19), `snapshot(&self) -> TranscriptSnapshot` (snapshot-types.md L53), `select_candidates(bytes, session_id, base_offset, session_cap) -> Vec<TranscriptCandidate>` (selection-module.md L106), `reconstruct_from_observations(session_id, obs, session_cap) -> Vec<TranscriptCandidate>` (reconstruct.md L47), `distill_before_purge(registry, feature_cycle, &observations, cfg) -> Option<TranscriptCandidatesSection>` (distill-handler.md L21). Naming pin honored throughout: `TranscriptSnapshot`, never `SessionTranscriptSnapshot`. Technology choices consistent with ADRs (pure observe module ADR-003; exhaustive retention match ADR-005; held store ADR-008).
**Deviations**: the 4 ADR-grounded additions above, all flagged in the pseudocode agent report.

### Specification coverage
**Status**: PASS
**Evidence**: FR-1 lock discipline → C1/C2; FR-2 take-shaped single reader → C1/C2; FR-3 selection → C3; FR-4 dual cap → C3 (session) + C6 (cycle) + C9 (knobs); FR-5 additive section → C4; FR-6 four-return → C6; FR-7 transient/no-persist → C4/C6; FR-8 reconstruction → C5; FR-9 topic_source soft → C5; FR-10 loss visibility → C4/C6; FR-11 two-pipe → C6 (AC-09 test); FR-12 retention seam → C7; FR-13 held buffer → C8; FR-14 untrusted parser → C3; FR-15 consumer guidance → C10. NFRs addressed: NFR-1 (lock holds, C1/C2), NFR-2/AC-12 (4 MiB <50ms, C3 test), NFR-4 (memory bound, C8), NFR-6 (regex-class only, C3/C10), NFR-8 (secrets, C4). No pseudocode implements an unrequested feature; "NOT in Scope" list respected (no server-side classification, no windowing, no wire change).

### Risk coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW Risk→Test table maps all R-01..R-20 to named tests and owning component plans. Per-component plans carry the scenarios: R-01 (held-buffer-store match/mismatch/null), R-02 (cap+TTL independent), R-03 (audit exactly-once across review/sweep/evict/multi-readopt), R-04/R-19 (content-leak gate + metadata-only Debug), R-05 (≥3-drain faithfulness + single-turn negative), R-06 (single-reader + #700 reuse), R-07 (four returns + fifth-return exhaustiveness), R-08 (no-parse-under-lock + concurrency), R-09 (cap-edge/overflow boundary), R-10 (fuzz corpus module+handler), R-11 (dependency-direction), R-12 (logical byte_offset), R-13 (registered∪held congruence), R-14 (topic_source reorders-not-filters), R-15 (deterministic cycle cap), R-16 (eviction/poison surfaced), R-17 (O(1) keyed delta route), R-18 (RetainDays inert), R-20 (independent-corpus provenance header). Integration risks (seam↔hold↔purge, four-return↔memoization, snapshot↔delta-merge, metadata↔fallback↔loss, Wave A↔B) each owned in the Cross-Component Test Dependencies section. Priority emphasis correct: the 6 merge gates carry the Critical risks.

### AC coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW AC→Test table covers AC-01..AC-13 + AC-V-SEAM + AC-V-FUZZ with verification methods matching ACCEPTANCE-MAP.md. Spot-checked against per-component plans: AC-04 (response-types serde-omit + distill-handler golden diff), AC-08 (distill-handler loss assembly incl. aggregate-cap drop surfacing), AC-09 (distill-handler detection_isolation extension), AC-10 (retention-gate no-wildcard + validate-reject + helper-None), AC-13 (consumer-guidance checklist + cargo audit). The infra-001 integration harness gaps and 5 new MCP tests are identified (no harness reshaping — deferred internal-lifecycle proof correctly kept in the Rust AC-11 test).

### Six merge gates — concrete test plans
**Status**: PASS
**Evidence**:
- **AC-11 ≥3-drain continuity**: held-buffer-store.md `continuity_simulated_lifecycle` — register→deltas→drain→deltas→drain→deltas→drain→re-register→review, asserts (a) cross-turn content not just last, (b) loud re-adopt/fail-loud mismatch, (c) held-count bound+eviction, (d) TTL reclaim w/o review, (e) audit exactly-once; explicit single-turn-rejected negative guard (L113-116).
- **AC-06 content-leak**: response-types.md `test_retrospective_report_has_no_candidate_field` (compile-level structural) + distill-handler.md re-review/grep/SQL/log gate + audit-detail content-free + snapshot-types.md metadata-only Debug.
- **AC-05 four-return exhaustiveness**: distill-handler.md per-return tests at 2110/2236/2925/3027 + `test_exhaustiveness_fifth_return_fails`.
- **AC-V-FUZZ no-panic**: selection-module.md malformed corpus (truncated/non-UTF-8/oversized/unknown-type/embedded-NUL/nested) at module level + distill-handler.md `test_handler_fully_corrupt_snapshot_normal_response` at handler level.
- **AC-V-SEAM single-reader**: snapshot-types.md `test_only_two_production_buffer_content_readers` + `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail` + all-four-metadata-fields.
- **AC-01 snapshot-and-release**: snapshot-seam.md `test_seam_no_parse_under_lock` source assertion + `test_concurrent_deltas_during_seam_consistent` stress/loom.

### Wave A/B dependency-direction separation (R-11)
**Status**: PASS
**Evidence**: OVERVIEW.md Wave A/B Dependency-Direction Map states the hard invariant — no Wave A module (C2, C3, C4, C5, C6, C7, and C1 except its held-scan branch) may have a compile-time `use`/path reference to `transcript_hold.rs`. C1 touches the hold ONLY through an optional/injected handle (`self.transcript_hold: Option<…>`, snapshot-seam.md L37, L94-98), making the held-scan branch severable. Every Wave A pseudocode file carries an explicit "NO reference to `transcript_hold.rs`" header. Dependency-direction tests planned in selection-module.md (`test_distill_module_no_transcript_hold_reference`) and distill-handler.md (`test_wave_a_handler_no_transcript_hold_dependency`) + Wave-A-only empty-buffer degrade run. C8 is the sole Wave B file. Separation is explicit and correct.

### Lock discipline (Constraint 1 / AC-01)
**Status**: PASS
**Evidence**: snapshot-seam.md encodes two phases — Phase 1 registry lock does Arc-clone only then releases (L26-44); Phase 2 per-buffer lock does `snapshot()` byte-copy + metadata only then releases (L46-54). "registry lock RELEASED here — before any buffer lock, before any parse" (L44). snapshot-types.md confirms `snapshot()` does byte-copy + metadata read, "NO parse, NO I/O" (L64). All JSONL parse / marker match happens in C6→C3/C5 strictly after the owned Vec returns (OVERVIEW Lock-Discipline Summary L77-93). Pattern #3753 honored: no downstream re-acquisition of a buffer lock. Poison recovery per #4764 surfaces loss (not silent). C8 delta routing merges under the buffer lock only with O(1) keyed lookup (R-17). No parse/marker-match under any lock anywhere.

### Interface consistency
**Status**: WARN
**Evidence**: Shared types in OVERVIEW.md "Shared Types" match per-component usage; `byte_offset` logical semantics consistent across C2/C3/C5 (`base_offset + in_snapshot_offset`; reconstructed = 0); the fallback predicate is defined once in C5/reconstruct.md and called by C6 for BOTH the path choice AND the provenance label (ADR-007 no-recomputation honored).
**Issue (minor, non-blocking)**: One contradiction between a pseudocode file and its own test plan on `TranscriptCandidate` Debug:
- response-types.md L60-63 (pseudocode) says: "Provide a metadata-only `Debug` for `TranscriptCandidate` (print `session_id`, `byte_offset`, `ts`, `family_hints`, `text.len()` — never `text`)."
- response-types.md test plan L28-31 says: `test_candidate_debug_present_text_is_intentional` — "`TranscriptCandidate.text` IS the candidate content the agent consumes; its Debug may show text ... the no-content-Debug rule applies to `TranscriptSnapshot`/`HeldBuffer`, NOT to the candidate."
These two directly conflict (metadata-only Debug vs Debug-may-show-text). The test-plan position is the defensible one: `TranscriptCandidate.text` is intentional response data the agent must receive, and the secrets-posture constraint (R-04/R-19) targets the persisted/log surfaces and the snapshot/held types, not the response value itself. The leak gate (AC-06) tests against SQL/log/audit/persisted surfaces, where candidates structurally cannot land (ADR-004). **This is a WARN, not a FAIL**: it is an authoring inconsistency to reconcile before/during 3b (pick one — recommend the test-plan position, candidate Debug may show text since it is response data), with no impact on the binding contract or any AC. Flag to the leader for a one-line pseudocode correction.

### Knowledge stewardship compliance (design-phase agents)
**Status**: PASS
**Evidence**:
- **pseudocode agent** (read-only tier): `## Knowledge Stewardship` block present in agent-1-pseudocode-report.md L33-39 with `Queried:` entries (context_briefing/search/get — #3753, #4799, #4750, ADRs) and an explicit "Deviations ... none" rationale. Read-only agent correctly has Queried entries, no Stored required.
- **risk-strategist** (active-storage tier): RISK-TEST-STRATEGY.md L460-469 has the block with `Queried:` (uni-knowledge-search — #4750, #3753, #4764, #3793, #3800, #3479) and `Stored: nothing novel to store -- {reason}` with a concrete reason (recurring patterns already captured as #4750/#3753/#4764/#3793; crt-052 risks are feature-specific). Reason present → no WARN.
- **architect** (active-storage tier): ADRs are stored in-repo as ADR-001..009 files (this feature uses ADR files per the brief's ADR Index). The spec agent's block (SPECIFICATION.md L480-485) has `Queried:` and a read-only no-store rationale. Stewardship obligations met across the design phase.

## Rework Required

None blocking. Two follow-ups for the leader/architect before or during Stage 3b (neither blocks 3b start):

| Item | Which Agent | What to Fix |
|------|-------------|-------------|
| Ratify the 4 ADR-grounded contract additions (ambiguities 1, 3, 4) | uni-architect (one-line each) | Confirm `SessionLossInfo.dropped_candidates`, the `readopt(session_id, feature_cycle)` arity (update ADR-008 literal signature), and `transcript_fallback_hole_fraction` (knob vs const). All are ADR-justified; this is ratification, not redesign. |
| Reconcile `TranscriptCandidate` Debug contradiction | uni-pseudocode (or leader edit) | response-types.md pseudocode (metadata-only Debug) contradicts its own test plan (Debug may show text). Recommend adopting the test-plan position (candidate text is response data). One-line correction. |

## Result Rationale

All five flagged ambiguities are sound, ADR/AC-grounded resolutions — none is a design defect. Every component aligns with the binding ARCH §4 surface (deviations are documented, ADR-justified, content-free or signature-only). Spec coverage is complete, all 20 risks and all 15 ACs map to concrete per-component test plans, the six merge gates each have a named test, Wave A/B separation is explicit and correct, and lock discipline is correctly encoded. The single interface contradiction and the four contract additions are WARN-level (need a one-line ratification / reconciliation), acceptable under the gate rule "All checks PASS (WARNs acceptable)". **PASS.**

## Knowledge Stewardship
- Queried: reviewed in-repo ADR-001..009, SPECIFICATION, RISK-TEST-STRATEGY, IMPLEMENTATION-BRIEF, ACCEPTANCE-MAP as the binding sources for this gate (no Unimatrix query needed — all source-of-truth is feature-local).
- Stored: nothing novel to store -- the gate findings are feature-specific (crt-052 contract additions live in this report and the pseudocode agent's flags). The cross-feature lesson worth noting ("binding integration-surface table omits AC-mandated fields/signatures, surfaced as pseudocode contract flags") is a single occurrence here; not yet a 2+-feature pattern, so no `/uni-store-lesson` per stewardship rules.
