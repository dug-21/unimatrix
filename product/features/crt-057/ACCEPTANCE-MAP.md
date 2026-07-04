# crt-057 Acceptance Criteria Map

**AC authority:** SPEC AC IDs (AC-01..AC-19) in `specification/SPECIFICATION.md` are **authoritative**. This map
carries each SPEC AC to its verification method, the RISK-TEST-STRATEGY scenario(s) that verify it, and the source
SCOPE AC it traces to. SPEC AC-01..AC-17 align 1:1 with SCOPE AC-01..AC-17; **AC-18** (window default) and **AC-19**
(ownership boundary — negative) are net-new SPEC ACs grounding ADR-006/OQ-2 and NG-5.

**Feature:** crt-057 · **Tracking:** GH #894 · **Total ACs:** 19
**Coverage flag:** AC-19 has **no dedicated risk scenario** — see "AC Lacking Risk Coverage" below.

Verification types: `test` (cargo test / specific fn), `manual` (human), `file-check` (path exists), `grep`
(content match), `shell` (run + check output).

| AC-ID | Description | Verification Method | Verification Detail | Risk Scenario(s) | ↔ SCOPE AC | Status |
|-------|-------------|--------------------|--------------------|------------------|-----------|--------|
| AC-01 | Default response (no `transcript`, `markdown`) contains NO candidate block; buffer intact. | test | Integration test: call with defaults; assert no candidate section; assert buffer unchanged (synchronous read). | R-09 sc.1, R-10 sc.1 | AC-01 | PENDING |
| AC-02 | With `transcript` present, response has a candidate section scoped by `phase`/`anchor`/`match`/`window`; **absent (not null)** when scope yields nothing. | test | Populated buffer + scope (section present, narrowed); scope yielding nothing (section absent, no crash). | R-09 sc.3, R-09 sc.5 | AC-02 | PENDING |
| AC-03 | `context_cycle_review` NEVER purges on any path/param; no purge verb; a 2nd identical review returns the same candidates; the eager purge removed from all four success returns. | test + grep | Spy/trace across default, `json`, `force`, every `transcript` shape: `purge_cycle_transcripts` never invoked; buffer intact after each; repeat `transcript:{}` returns identical candidates. Source-grep: the four `purge_cycle_transcripts(` calls removed. | R-06 sc.4, R-10 sc.1-2, R-09 sc.2, R-08 sc.1 | AC-03 | PENDING |
| AC-04 | Content-opaque fold read still runs, gated at all four success returns per #4750, as the sole remaining review-seam side-effect; no candidate/buffer content reaches any SQL/file/log write. | test | Assert fold lands durable integers at each of the four returns (incl. memo-hit); audit assertion that persisted rows are content-free. | R-07 sc.1, R-03 sc.2 | AC-04 | PENDING |
| AC-05 | `transcript:{}` (present, all-None) returns the full candidate set under the existing per-cycle cap, equivalent to `match:".*"`, non-destructively. | test | `transcript:{}` and `transcript:{match:".*"}` return the same candidate set bounded by the cap; buffer intact after both. | R-09 sc.2 | AC-05 | PENDING |
| AC-06 | For `match`, each returned session reports `matched`, `search_complete` (false iff `elided_bytes>0 ∥ has_holes ∥ Reconstructed`), `elided_bytes`, `provenance`; a no-match over `search_complete==false` is INDETERMINATE, never a bare false. | test | No-match over clean `Primary` → trustworthy negative; no-match over elided/holed/`Reconstructed` → INDETERMINATE with loss surfaced; per-loss-condition matrix; no bare boolean. | R-01 sc.1-5 | AC-06 | PENDING |
| AC-07 | `anchor`/`phase` return the evidence-ts span / phase bounds that defined the window and include `ts:None` candidates via `byte_offset` proximity fallback; no candidate silently drops out. | test | `anchor:<id>, window:±N` and `phase:<id>` assert returned bounds and that a `ts:None` candidate inside the byte-proximity window is included and flagged. | R-05 sc.1-2 | AC-07 | PENDING |
| AC-08 | An agent query in its own units (finding/anchor id, phase id, regex, event/time window) resolves against skewed Plane-B `ts` without knowing Plane B's clock; candidate `ts` normalized to a canonical epoch; `ts:None` uses `byte_offset` fallback. | test | Fixture candidates whose JSONL `ts` is skewed from Plane-A `EvidenceRecord.ts`: anchor query resolves the correct candidate via windowed join; `ts:None` resolves via `byte_offset`; no test path supplies a Plane-B timestamp; explicit fixed offsets. | R-05 sc.1, sc.4-5 | AC-08 | PENDING |
| AC-09 | `force:true` always accepted, report-only recompute from durable observations, NEVER retrieves candidates and NEVER purges; report reproducible before and after buffer reclamation. | test | `force:true` (no `transcript`) before/after reclamation → identical report body, no candidate section, buffer untouched; `force:true` + `transcript` → report recomputed AND scoped slice returned (orthogonal, no precedence). | R-18 sc.2 | AC-09 | PENDING |
| AC-10 | Default response achieves ≥80% token reduction versus the full JSON candidate-bearing response for a typical review. | test | Measured: `tokens(default_markdown) ≤ 0.20 × tokens(transcript_full_json)`; populated fixture; ratio not absolute; empty-buffer vacuity guard. | R-13 sc.1-3 | AC-10 | PENDING |
| AC-11 | `format:"json"` renders identical report content to `markdown` — no candidates, no purge; differ only in serialization. `format:"summary"` (and any unknown) → `ERROR_INVALID_PARAMS` at all four render loci. | test | Same cycle `markdown` vs `json` — semantic content equality, buffer intact after both; assert `"summary"` → `ERROR_INVALID_PARAMS` with exact message; no surviving third path. | R-12 sc.1-2 | AC-11 | PENDING |
| AC-12 | `transcript` scope threading and the fold-read gate apply identically at all four success returns; `distill_handler.rs:651-726` source-assertion tests pass (purge-count assertion removed with rationale, fold-read four-site assertion preserved); memo-hit (site 3) honors `transcript` identically to full-pipeline; no per-site forking. | test | Source-assertion tests; behavioral matrix row per site: memo-hit + `transcript` present → scoped candidates present, buffer intact; memo-hit + no `transcript` → no candidates; path-proven (prove which site executed). | R-07 sc.1-2, R-11 sc.1-2 | AC-12 | PENDING |
| AC-13 | The 64-cap, 24h TTL sweep, and per-turn session-close purge are unchanged and the sole reclamation path; no new cycle-close purge trigger; orphaned `purge_cycle_transcripts` + helpers deleted (dead-code clean). | test + grep | Never-`transcript` cycle: buffer reclaimed by a backstop with content-free audit; no cycle-close purge path; assert `purge_cycle_transcripts` / `clear_transcripts_for_feature` / `purge_held_for_feature` deleted (clippy dead-code); exhaustive `TranscriptRetention` re-homed. | R-06 sc.1-3, R-06 sc.4 | AC-13 | PENDING |
| AC-14 | Candidates and loss-propagation fields stay response-transient, outside the memoized report; persisted `RetrospectiveReport` gains no candidate slot; scoped-retrieval path creates no new persistence. | test | Persisted report struct has no candidate/loss field (compile-time + serialized-form); sink content-scan on every changed path incl. reclamation-without-review; loss carrier response-transient only. | R-03 sc.1-4 | AC-14 | PENDING |
| AC-15 | An amending ADR amends #4742 and #4857 recording the purge removal, fully-non-destructive review (no purge verb), residency posture change (bounded by unchanged cap/TTL/session-close), and disk-posture-unchanged. | manual + test | Verify stored ADR content covers all four statements; verify amendment used `context_correct`, not deprecate+store. | R-17 sc.1 | AC-15 | PENDING |
| AC-16 | `uni-retro/SKILL.md` and the `context_cycle_review` tool description use the `transcript{}` block, imply no purge-on-review / any-review-carries-candidates behavior, and the tool description states no purge verb; `uni-agent-routing.md` excluded. | grep + test | Doc grep: no residual `include_transcript_candidates` / "any review carries candidates" / "review purges" in the four-doc atomic unit; `transcript{}` present in both; `uni-agent-routing.md` NOT grepped; end-to-end harvest-fires test proves consumer+server agree. | R-02 sc.1-4 | AC-16 | PENDING |
| AC-17 | BOTH `uni-delivery-protocol.md` and `uni-bugfix-protocol.md` keep the review phase open through merge, close after merge (`phase-end` then `stop`), then invoke `/uni-retro`, ordering merge → close → retro; retro retrieves non-destructively; human merge gate unchanged. | test + grep | Per protocol: review phase not stopped pre-merge; human merge → `phase-end`+`stop` → `/uni-retro` in that order; trace test — stop the cycle, then `transcript` retrieval, assert candidates present (close touched no buffer); protocol-parity grep (both files, neither retains pre-merge `stop`). | R-04 sc.1-3, R-08 sc.1-3 | AC-17 | PENDING |
| AC-18 | With `anchor`/`match` supplied and `window` omitted, default window is ±120 000 ms for ts-bearing candidates and ±3 candidate blocks for `ts:None`; caller override honored and cap-bounded. | test | Omit `window` → assert the ±120 000 ms / ±3-block selection bounds; supply override → honored under the cap; self-bounding `phase` ignores `window`. | R-05 sc.3 | AC-06/AC-07 (mechanism, OQ-2) | PENDING |
| AC-19 | `context_cycle_review` returns only the Plane-A summary + the honest scoped Plane-B slice; NO synthesized GH stewardship joins, applied-entry attribution, rework-count↔cause join, or human-intervention ledger. | test + manual | Response schema contains no attribution/join/ledger field; no code path synthesizes across GH `## Knowledge Stewardship` blocks. **Adjacent only:** R-18 sc.1 (report-body invariance) proves no transcript signal enters the summary — it does NOT assert absence of an attribution/ledger field. | R-18 sc.1 (adjacent — see flag) | NG-5 | PENDING |

---

## Coverage Summary

- **19 SPEC ACs** total (AC-01..AC-19).
- **18 ACs** map to at least one directly-verifying RISK-TEST-STRATEGY scenario.
- Critical-risk coverage: R-01 (silent false negative) → AC-06; R-02 (consumer partial-ship) → AC-16; R-03
  (persistence leak) → AC-14/AC-04; R-04 (two-protocol mis-wiring) → AC-17.

## AC Lacking Risk Coverage (flagged)

- **AC-19 (ownership boundary — negative, NG-5).** No dedicated risk scenario. The risk register has **no SR/R risk
  for NG-5** (the SR list is SR-01..SR-14; SR-06→R-18 covers the *NG-7* line — distilling INTO the summary — a
  *different* negative). R-18 sc.1 (report-body-invariance-under-buffer-state) is **adjacent**: it proves no
  transcript-derived signal enters the summary, but does NOT assert the response schema lacks an attribution / join /
  human-ledger field, nor that no code path synthesizes across GH stewardship blocks. **Recommendation for the
  tester:** add a dedicated negative scenario for AC-19 (schema-shape assertion: no attribution/join/ledger field;
  code-path assertion: no cross-GH-block synthesis) rather than lean on R-18. Low-severity gap — AC-19 fences scope
  *out* (a negative requirement over code that is not being built), so regression risk is low, but the AC is currently
  verified only by inspection, not by a risk-mapped test.

## Notes

- SPEC AC-18 traces to SCOPE AC-06/AC-07 (mechanism) and OQ-2 (the delegated window-default decision), not to a
  distinct SCOPE AC — it is a net-new SPEC AC grounding ADR-006.
- SPEC AC-19 traces to SCOPE NG-5 (not a SCOPE AC) — a net-new SPEC AC formalizing the ownership boundary as a
  testable negative requirement (FR-25).
- All 17 boolean-era SCOPE ACs were superseded; the reworked SCOPE AC-01..AC-17 already reflect the non-destructive
  contract, so no stale boolean-era AC remains to reconcile.
