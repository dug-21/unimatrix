# Risk-Based Test Strategy: crt-057

Fully non-destructive `context_cycle_review` with scoped, honest transcript retrieval
(`transcript{ phase?, anchor?, match?, window? }`).

**Feature:** crt-057 · **Tracking:** GH #894 · **Phase:** Cortical
**Source docs:** SCOPE.md (re-scoped 2026-07-04), SCOPE-RISK-ASSESSMENT.md, ARCHITECTURE.md,
ADR-001..ADR-006, `product/research/ass-091/FINDINGS.md`.
**MAJOR REWORK (2026-07-04):** rewritten for the ass-091 redesign. The transcript axis is now a
**read-only scoped retrieval** (not the fused `include_transcript_candidates` emit+purge boolean);
`context_cycle_review` has **NO purge verb** (eager review-purge removed; reclamation is backstops-only:
24h TTL / 64-cap / session-close); the content-opaque fold read (#5030) is the **only** surviving
review-seam side-effect, still gated at all four #4750 sites; loss propagation makes a no-match over a
lossy/`Reconstructed` session **INDETERMINATE**. Boolean-era rows are retired below.

This strategy identifies what could fail in the *designed* system and the scenarios that would detect
each failure. It does not prescribe implementation — `uni-tester` translates scenarios into concrete
tests, extending the existing `distill_handler.rs` fixtures (C-7, no isolated scaffolding).

**Four design facts dominate the risk profile:**

1. **The redesign exists to kill ONE failure: the silent false negative.** A bare no-match, or a
   `search_complete` not derived from *every* loss condition (`elided_bytes>0 ∥ has_holes ∥
   provenance==Reconstructed ∥ dropped_candidates>0`), over a lossy session reads as "didn't happen."
   This is R-01 — the top risk — and its coverage must prove INDETERMINATE is surfaced for **every**
   loss condition, per session.
2. **Clock normalization is now a first-class interface correctness requirement**, not a carry-forward.
   Plane A epoch-millis ↔ Plane B JSONL `ts` skew, `ts:None` `byte_offset` fallback, and the ±120 s /
   ±3-block window default are where candidates silently drop from anchor/phase joins (R-05). Historical
   evidence: named-conversion-helper-at-boundary (#3385/#3372), time-window tests flap under parallel
   runs — use explicit offsets (#4195), epoch-boundary three-tier suites (#4236).
3. **The dominant load-bearing assertions are negative** ("NO purge on any path", "buffer intact",
   "no candidates on default", "no bytes to disk"). Lesson **#4879**: a `==0` check against an
   asynchronous effect cannot be stabilized by polling. Every "did not happen" assertion here must key
   on **synchronous, observable state** (buffer still present; spy captured at a synchronization point),
   never on the absence of an async audit event (R-10, cross-cutting).
4. **Two blast radii are feature-wide, not crt-057-scoped.** The two-protocol lifecycle restructure
   (D-7, both `uni-delivery-protocol.md` and `uni-bugfix-protocol.md`) and the consumer-reconciliation
   atomic unit rewire the harvest for **every future delivery and bugfix session**. A mis-wire silently
   breaks attribution/verbatim harvest for all features (R-02, R-04, R-08), verified end-to-end and
   per protocol — not at the crt-057 server surface (evidence #5383: verify call-site reachability;
   store-layer read-backs are false positives).

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Silent false negative** — a bare no-match, or `search_complete` not derived from all loss conditions (`elided_bytes>0 ∥ has_holes ∥ Reconstructed ∥ dropped_candidates>0`), over a lossy session presents as "didn't happen." THE risk the redesign exists to prevent. | High | Med | **Critical** |
| R-02 | **Consumer-reconciliation partial ship** — server (D-1..D-4) ships without one of the atomic-unit sites (`uni-retro/SKILL.md`, tool description, `uni-delivery-protocol.md`, `uni-bugfix-protocol.md`); retro calls the removed boolean or the non-retrieving default → harvest (#5219) starves silently, no error. | High | High | **Critical** |
| R-03 | **No-new-persistence leak on a changed / longer-residency path** — buffer/candidate bytes reach SQL/file/log, or `RetrospectiveReport` gains a candidate slot; the one absolute invariant (#4721/#4850, NG-1). | High | Med | **Critical** |
| R-04 | **Two-protocol lifecycle mis-wiring** — merge→close→retro (D-7) wired wrong in delivery and/or bugfix; every future session's attribution or verbatim harvest silently breaks, feature-wide. | High | Med | **Critical** |
| R-05 | **Clock/skew normalization wrong** — epoch-millis↔JSONL windowed join miscomputed, `ts:None` candidates silently drop (no `byte_offset` fallback), or the ±120 s/±3-block window default mis-applied → false omission from anchor/phase joins. | High | Med | **High** |
| R-06 | **Orphan-deletion / backstop-reclamation regression** — `purge_cycle_transcripts` + `clear_transcripts_for_feature` + `purge_held_for_feature` left as dead code (anti-stub), or the exhaustive `TranscriptRetention` match not re-homed onto surviving backstops, so TTL/cap/session-close fail to reclaim or change retention behavior. | High | Med | **High** |
| R-07 | **Fold-read four-site lockstep drift** — the content-opaque fold (#5030) not gated at all four success returns (esp. memo-hit, site 3) after the purge left the seam → durable integers silently under-counted, not `force`-reproducible (#4585/#4750). | High | Low | **High** |
| R-08 | **Cycle-close-non-purging regression** — a future/misread `context_cycle(stop)` purges/reclaims buffers, so post-close retro extracts empty; merge→close→retro composes ONLY because close is inert (ADR-005). | High | Low | **High** |
| R-09 | **Scoped-filter correctness** — AND-composition of `phase`/`anchor`/`match`/`window` wrong; `transcript:{}` ≢ `match:".*"` full dump under the cap; omit ≠ summary-only/non-destructive; `window` not ignored by self-bounding `phase`. | Med | Med | **High** |
| R-10 | **Negative-assertion unreliability** — "no purge"/"buffer intact"/"no candidates" tests key on absence of an async audit and pass vacuously or flake (#4879). Cross-cutting. | High | Med | **High** |
| R-11 | **Source-assertion-removal side-effects** — deleting the ×4 purge-count / attach-before-purge assertions leaves another check silently depending on them, or the fold-read ×4 assertion is dropped by mistake. | Med | Med | **Medium** |
| R-12 | **Render divergence / `"summary"` drop** — markdown≠json content, or dropping `"summary"` breaks a live caller / leaves a third render path. | Med | Med | **Medium** |
| R-13 | **AC#10 measurement vacuous or brittle** — token-reduction asserted against an empty/tiny buffer (ratio meaningless), or a brittle absolute byte threshold. | Med | Med | **Medium** |
| R-14 | **crt-055 fold double-count** — non-purging common path re-read repeatedly; a non-idempotent fold double-counts durable `cycle_review_index` metrics. | Med | Low | **Medium** |
| R-15 | **Residency-over-long-merge-window fidelity loss** — dev-phase buffers TTL-age/cap-evict before the post-merge retro; accepted+graceful ONLY if surfaced (`Reconstructed`+loss); a defect if SILENT. | Med | Med | **Medium** |
| R-16 | **Degraded/second-retrieval path crash or stale verbatim** — repeat retrieval, or retrieval over aged/evicted/partial buffer, panics or returns stale bytes instead of empty/`Reconstructed`. | High | Low | **Medium** |
| R-17 | **ADR amended via deprecate+store instead of `context_correct`** — breaks #4742/#4857 provenance. | Low | Med | **Low** |
| R-18 | **NG-7 scope creep / force non-orthogonality** — transcript signal distilled into the report body (ass-090 scope), or `force`/`transcript` not orthogonal (a `force` early-return that also skips retrieval). | Low | Low | **Low** |

---

## Risk-to-Scenario Mapping

### R-01: Silent false negative (the top risk)
**Severity:** High · **Likelihood:** Med
**Impact:** A `match` no-match over a session that lost content (past the 4 MiB tail, in a hole, or a
0.81-fidelity `Reconstructed` rebuild) is read by the retro agent as "this didn't happen." Attribution
and self-learning are then built on a false absence — the exact failure the redesign exists to prevent.

**Test Scenarios:**
1. **Every loss condition independently flips `search_complete` false.** Four sub-cases, each a
   no-match over a session carrying exactly ONE loss signal: `elided_bytes>0`; `has_holes==true`;
   `provenance==Reconstructed`; `dropped_candidates>0`. Assert each returns `matched:false` **with**
   `search_complete:false` and the triggering loss field surfaced → INDETERMINATE, not a bare false.
2. **Clean-session negative is a trustworthy negative.** No-match over a `Primary` session with zero
   loss → `matched:false`, `search_complete:true` (a real "didn't happen"), and the clean session may
   be OMITTED (silence = nothing to report, ADR-003).
3. **No bare boolean anywhere.** Assert the `match` response NEVER collapses to a bare `matched` without
   its per-session `SessionLossInfo` — inspect the response shape, not just a happy-path value.
4. **Derivation-correctness of `search_complete`:** a session with two simultaneous loss signals is
   still `search_complete:false` (OR, not AND); a session with none is `true`. Guards against an
   inverted or partial predicate.
5. **Loss row present on a MATCH too**, not only on a miss — a positive match over a lossy session still
   surfaces loss (the match may be incomplete), so the consumer sees fidelity.

**Coverage Requirement:** A per-loss-condition matrix proving `search_complete==false` for EACH of
`elided_bytes>0`, `has_holes`, `Reconstructed`, `dropped_candidates>0` (and their OR-combination), plus a
clean-session trustworthy-negative row. Any no-match path that can return without a `SessionLossInfo`
fails the gate. This is the feature's raison d'être — the richest coverage in the suite.

### R-02: Consumer-reconciliation partial ship (dominant scope risk, SR-04)
**Severity:** High · **Likelihood:** High
**Impact:** Shipping the server without any one of the atomic-unit sites flips `/uni-retro` to the
non-retrieving default (or a removed-boolean call): it stops receiving candidates **with no error**, and
the harvest (#5219) starves silently. Lives across code + skill + tool-description + **both protocols** —
the failure most likely to escape code-level tests. Atomic unit (§8): server (D-1..D-4) +
`uni-retro/SKILL.md` + tool description + `uni-delivery-protocol.md` + `uni-bugfix-protocol.md`.
`uni-agent-routing.md` is excluded (passive mention, no live protocol loads it).

**Test Scenarios:**
1. **End-to-end harvest-fires check (load-bearing).** Exercise the reconciled `/uni-retro` path: assert
   the candidate-bearing call issues a `transcript{}` block (NOT the removed boolean) and the response
   actually contains a candidates section with per-session `SessionLossInfo`. Proves consumer + server
   agree post-change, not just that the server works in isolation (#5383: reachability, not read-back).
2. **Doc/grep guard (AC-16), corrected set:** assert NO residual `include_transcript_candidates` /
   "any review carries candidates" / "review purges" language survives in `uni-retro/SKILL.md`, the tool
   description, or either protocol; assert the retrieval call references the `transcript{}` block. Do NOT
   grep `uni-agent-routing.md` (excluded — a guard on it would fail spuriously).
3. **No-purge-verb in the tool description:** assert the description states plainly the tool has no purge
   verb and lists the three axes (`format` render-only, `force` recompute-only, `transcript{}` read-only).
4. **Retro is repeatable, not one-shot:** assert the reconciled SKILL.md no longer sequences around a
   one-shot extraction (retrieval is non-destructive and re-runnable in any scope).

**Coverage Requirement:** One test driving the reconciled retro call end-to-end and asserting candidates
+ loss were delivered, PLUS a grep-style guard over the corrected four-doc atomic unit that FAILS if any
site still implies boolean/purge semantics and does NOT reference `uni-agent-routing.md`. A green
server-only suite with an unreconciled consumer must not pass the gate.

### R-03: No-new-persistence leak on a changed path (SR-01/SR-02, AC-14)
**Severity:** High · **Likelihood:** Med
**Impact:** Residency now keeps raw (possibly secret-bearing) bytes in memory longer on every path, and
the scoped-retrieval path is a new response surface. Any of it reaching SQL/file/log — or the memoized
`RetrospectiveReport` gaining a candidate/loss slot — silently persists secrets, the one absolute
invariant (#4721/#4850). The reclamation-without-review path is the least-tested, highest-leak-suspect.

**Test Scenarios:**
1. **Struct-shape guard:** assert the persisted `RetrospectiveReport` has NO candidate field and NO
   transcript-content field; the candidates + `SessionLossInfo` live on `TranscriptCandidatesSection`,
   attached at assembly OUTSIDE the memoized report (compile-time + serialized-form assertion).
2. **Sink-scan on every changed path:** for default, json, force, `transcript:{}`, scoped `match`,
   scoped `anchor/phase`, and **cap/TTL/session-close reclamation-without-review**, scan every SQL row /
   file / log line written and assert none contains buffer or candidate byte-content. Reuse the #5089
   shape (`#[traced_test]`, assert no 64+ hex run / no verbatim delta text).
3. **Audit content-free at the backstop:** assert the `trigger=stale_sweep` / cap-eviction /
   session-close reclamation audit carries counters + session id only, zero content (the audit now fires
   ONLY at the backstop — SR-02).
4. **Loss-carrier is response-transient:** assert `SessionLossInfo` / `search_complete` appear only in
   the response, never in any persisted row or the memoized report.

**Coverage Requirement:** AC-14 verified on ALL changed paths INCLUDING reclamation-without-review, with
a content-scan (not just a field-name check) on every write sink. The loss carrier is explicitly in scope.

### R-04: Two-protocol lifecycle mis-wiring (feature-wide blast radius, SR-11)
**Severity:** High · **Likelihood:** Med
**Impact:** The restructure (cycle stays open through the human merge decision; `context_cycle
phase-end`+`stop` only after merge; `/uni-retro` post-close; order merge→close→retro) is edited into
`uni-delivery-protocol.md:516-521` and `uni-bugfix-protocol.md:418-435`. If either stops the cycle
before merge (old behavior) or misorders/omits the post-close `/uni-retro`, then for **every future
session on that protocol** merge/rework is unattributed and/or the verbatim harvest never fires —
silently. Blast radius is the whole delivery/bugfix population.

**Test Scenarios:**
1. **End-to-end full-cycle simulation, PER protocol.** Drive: cycle open → review phase → **simulated
   human merge** → `context_cycle(phase-end)` → `context_cycle(stop)` → `/uni-retro`. Assert (a) the
   cycle is still OPEN at merge; (b) close fires only after merge; (c) the post-close `/uni-retro`
   retrieval returns a non-empty candidates section with loss. Run for delivery AND bugfix — separate
   wirings.
2. **Ordering assertion:** the executed sequence is exactly merge→close→retro; a retro before close, or
   a close before merge, fails.
3. **Protocol-parity grep:** both files contain the post-close `/uni-retro` step and neither retains a
   pre-merge `context_cycle(stop)`; a fix in only one protocol fails (#4915: code-derived cross-check,
   not a single-file human gate).

**Coverage Requirement:** A per-protocol end-to-end scenario proving open-through-merge → close-after →
post-close retrieval delivers candidates, for BOTH protocols. A server-only or single-protocol green
suite must not pass.

### R-05: Clock/skew normalization wrong (SR-08)
**Severity:** High · **Likelihood:** Med
**Impact:** Plane A `EvidenceRecord.ts` (u64 epoch-millis) and Plane B `TranscriptCandidate.ts`
(`Option<String>` JSONL) are independent clocks for `Primary` sessions. An exact-match join, a wrong
epoch parse, or a missing `ts:None` `byte_offset` fallback silently drops candidates from anchor/phase
queries — a false omission indistinguishable from "not there." The agent must never need Plane B's clock.

**Test Scenarios:**
1. **Skewed-clock windowed join:** an anchor evidence event at Plane A `ts=T` resolves candidates whose
   Plane B JSONL `ts` is offset by a realistic skew, over the ±120 s window — the offset block is still
   selected (window absorbs skew, ADR-006). An exact-match join would miss it; assert it is found.
2. **`ts:None` never silently drops:** a candidate with `ts:None` inside an anchor/phase window is
   included via `byte_offset` proximity (±3-block fallback), not dropped. Assert its presence AND that
   it is flagged as ts-less-included, so a consumer sees the fallback fired.
3. **Window default applied when omitted; overridable when supplied:** `anchor` with no `window` uses
   ±120 000 ms / ±3 blocks; a caller `window` overrides it; self-bounding `phase` IGNORES `window`.
4. **Agent-unit-only interface:** the query is expressed in finding/anchor id, phase id, regex,
   event/time window — assert no test path requires the caller to supply a Plane-B storage timestamp.
5. **Deterministic time in tests:** use explicit fixed offsets, never `now_ts()` for window boundaries
   (#4195 — time-window boundary tests flap under parallel runs); include an epoch-boundary triple
   (just-inside / on / just-outside the window) per #4236.

**Coverage Requirement:** A skewed-clock join test proving the window absorbs Plane A↔B skew; a
`ts:None`-included-via-`byte_offset` test; window-default + override + phase-ignores-window; all using
explicit offsets with on/inside/outside boundary cases. No exact-timestamp-match join anywhere.

### R-06: Orphan-deletion / backstop-reclamation regression (SR-10, C-5, anti-stub)
**Severity:** High · **Likelihood:** Med
**Impact:** `purge_cycle_transcripts` (server.rs:661) + `clear_transcripts_for_feature` +
`purge_held_for_feature` lose all non-test callers once the four review-site calls are removed. Left in
place they are dead code (CLAUDE.md rule 2 / clippy). Worse, the exhaustive `TranscriptRetention` match
lived INSIDE the deleted purge; if it is not re-homed onto the surviving backstop reclaim paths, either
retention behavior changes silently or a `_` arm swallows a future variant. Backstops are now the SOLE
reclamation path — a broken one means unbounded residency (secrets) with no visible failure.

**Test Scenarios:**
1. **Orphans deleted (dead-code guard):** assert `purge_cycle_transcripts`, `clear_transcripts_for_feature`,
   `purge_held_for_feature` no longer exist (or a clippy `dead_code`/deny gate covers it) — removal, not
   `#[allow]`. Verify no non-test caller remains (#5383: reachability check).
2. **Backstops still reclaim after the eager purge is gone:** three sub-cases — 24h TTL sweep, 64-cap
   eviction, per-turn session-close — each reclaims a never-reviewed cycle's buffer and emits a
   content-free terminal audit. Assert the buffer is gone AND the audit is byte-free.
3. **Exhaustive-match re-homed, retention behavior unchanged:** assert the surviving reclaim path matches
   `TranscriptRetention` exhaustively (`RetainDays` stays a no-op, no `_` arm) — reuse the enum-exhaustive
   discipline (#4831). A test that supplies each variant to the reclaim path proves no arm was dropped.
4. **No new cycle-close/review purge trigger introduced:** assert no purge fires at review or at
   `context_cycle(stop)` — reclamation only via TTL/cap/session-close (ties R-08).

**Coverage Requirement:** Dead-code removal proven; each backstop reclaims a never-reviewed cycle with a
content-free audit; the exhaustive match is re-homed and retention behavior is byte-unchanged; no new
purge trigger exists.

### R-07: Fold-read four-site lockstep drift (SR-09, #4750/#4585)
**Severity:** High · **Likelihood:** Low
**Impact:** After the purge leaves the seam, the content-opaque fold read (#5030,
`activity_snapshots_for_feature`) is the ONLY surviving success side-effect and must stay gated at all
four `result.is_ok()` returns. Missing a site — especially the easy-to-miss memo-hit return (site 3) —
under-counts non-`force`-reproducible durable integers on a cached re-review: the precise #4585 drift.
Source-assertion counting sees the helper is *called* 4×; it cannot see the fold landed correctly.

**Test Scenarios:**
1. **Path-proven per-site fold rows:** for each of the four success returns (purged-signals,
   cached-metrics, **memo-hit**, full-pipeline), run a review and assert the durable `cycle_review_index`
   fold integers are written. The fixture must PROVE which site executed (assert a memo-hit indicator /
   no-recompute), not assume it (#4452 — a vacuous pass routed through the full-pipeline path).
2. **Memo-hit parity:** the memo-hit row's fold outcome equals the full-pipeline row's for the same
   buffer state. Divergence between sites is the defect.
3. **Fold survives the non-purging review:** because the buffer now survives, a subsequent review re-reads
   the same buffer — assert the fold still reads it (nothing lost sooner), coupled with R-14 idempotency.

**Coverage Requirement:** A behavioral fold-landed row per success return with a path-proof, memo-hit
non-optional. Source-assertion counting alone does not satisfy this (ADR-004).

### R-08: Cycle-close-non-purging regression (ordering correctness, ADR-005)
**Severity:** High · **Likelihood:** Low
**Impact:** merge→close→retro composes ONLY because `context_cycle(stop)` is non-purging (ADR-005 traced
it drains only the retrospective queue + writes an audit row; no buffer/hold/registration touch). If a
future change (or a misread of `PurgeOnCycleClose`) makes close purge/reclaim buffers, the post-close
retro retrieves from **empty buffers** and silently zeroes candidates for **every** feature — the report
survives (buffer-independent), so the harvest starves with no visible failure.

**Test Scenarios:**
1. **Close-then-retrieve still delivers (regression guard):** register buffers, run
   `context_cycle(stop)`, THEN run a `transcript:{}` retrieval. Assert the buffers survived `stop`
   (synchronous read: still present) and the post-close retrieval returns non-empty candidates.
2. **`context_cycle(stop)` is buffer-inert:** a `stop` on a cycle with registered ∪ held buffers leaves
   buffer count/content unchanged (synchronous before/after) and writes only its queue-drain audit — no
   purge/reclamation audit, no registration change.
3. **Order-insensitivity of close vs retrieve:** retrieve-before-close yields the same candidate set as
   retrieve-after-close (close is inert w.r.t. buffers) — the locked merge→close→retro order loses
   nothing.

**Coverage Requirement:** A post-`stop` retrieval-still-returns-candidates guard + a
`context_cycle(stop)`-is-buffer-inert assertion, both keyed on synchronous buffer observation (R-10),
never on the absence of an async purge event.

### R-09: Scoped-filter correctness
**Severity:** Med · **Likelihood:** Med
**Impact:** The `transcript{}` block's `phase`/`anchor`/`match`/`window` AND-composition, the
`transcript:{}` ≡ `match:".*"` full-dump-under-cap degeneracy, and the omit=summary-only default must all
hold; a wrong composition over- or under-selects, and under-selection is a silent miss.

**Test Scenarios:**
1. **Omit = summary only, non-destructive:** no `transcript` field → NO candidates section, buffer intact.
2. **`transcript:{}` = full dump under cap ≡ `match:".*"`:** both return the same full candidate set under
   the existing per-cycle cap, non-destructively; a second identical call returns the same set (buffer
   survived) until a backstop reclaims (AC-03/AC-05).
3. **AND-composition narrows:** `phase` ∧ `match` returns candidates in the phase window AND matching the
   regex — a strict subset of either alone; assert the intersection, not a union.
4. **`window` modifies anchor/match, ignored by phase:** `phase` is self-bounding (a supplied `window` has
   no effect); `anchor`/`match` honor `window`.
5. **Empty scope result is absent, not null:** a scope yielding nothing returns an absent candidates
   section (AC-02), never a crash.
6. **Invalid `match` regex → `ERROR_INVALID_PARAMS`**, not a panic.

**Coverage Requirement:** Each axis and the AND-composition exercised; the `transcript:{}`≡`match:".*"`
full-dump equivalence and repeatability asserted; omit=summary-only; window-ignored-by-phase; empty=absent;
bad-regex error path.

### R-10: Negative-assertion unreliability (cross-cutting, #4879)
**Severity:** High · **Likelihood:** Med
**Impact:** The feature's core guarantees are negative ("no path purges", "buffer intact", "no candidates
on default"). Keyed on the absence of an async audit, they pass vacuously and cannot catch a regression
where a purge later fires (#4879: a `==0` check on a fire-and-forget effect cannot be poll-stabilized).

**Test Scenarios:**
1. **Positive-state negative proof:** assert "no purge" by reading the buffer *after* the review and
   asserting it is **still present with the same content** — a synchronous fact — not by asserting zero
   purge-audit rows.
2. **Spy at a synchronization point:** if a purge/reclaim spy is used, capture it at handler return so
   `purge_calls == 0` is deterministic, not a race.
3. **Backstop reclamation (a positive event) MAY poll** for the audit's appearance (#4879: positive side
   may poll; negative side may not) — the asymmetry is explicit.

**Coverage Requirement:** Every "no purge"/"no candidates"/"buffer intact" assertion keys on synchronous
buffer/state or a synchronously-captured spy. A test-construction constraint on the whole matrix.

### R-11: Source-assertion-removal side-effects
**Severity:** Med · **Likelihood:** Med
**Impact:** The ×4 `purge_cycle_transcripts(` source-count and the attach-before-purge ordering assertions
in `distill_handler.rs:651-726` are deliberately deleted (no purge to count/order). Risk: some other test
or invariant silently depended on them, or the still-required fold-read/attach ×4 counts are dropped by
mistake in the same edit.

**Test Scenarios:**
1. **Deliberate-removal rationale recorded:** assert the removed purge-count/ordering assertions are gone
   with an in-source rationale comment (C-1/AC-12), not silently.
2. **Surviving counts intact:** assert the `distill_before_purge(` / `attach_to_response_assembly(` (and
   the fold-read) ×4 counts still stand — the fold-read lockstep (R-07) is still enforced.
3. **Grep every hidden dependent site (#4044):** confirm no other test file, doc comment, or assertion
   references the removed purge count.

**Coverage Requirement:** The purge assertions are removed with rationale; the surviving four-site counts
and the fold lockstep remain; a grep confirms no orphaned dependency on the removed strings.

### R-12: Render divergence / `"summary"` drop
**Severity:** Med · **Likelihood:** Med
**Impact:** markdown and json must be content-identical (serialization only). Dropping `"summary"` turns
any live `format:"summary"` caller into `ERROR_INVALID_PARAMS`; an incompletely-removed arm leaves a
third divergent render path (four dispatch loci: tools.rs:2532, :3359, :4268, :4324).

**Test Scenarios:**
1. **Render-equivalence:** same cycle rendered markdown vs json — semantic content equality, buffer
   intact after both, no candidates on either (both are non-`transcript` paths).
2. **`"summary"` → `ERROR_INVALID_PARAMS`** with the exact message (`Valid values: "markdown", "json"`);
   no `"summary"` arm survives at any of the four loci.
3. **Consumer sweep for `format:"summary"`:** grep reconciled consumers; flag any live `"summary"` caller
   to the delivery leader.

**Coverage Requirement:** Render-equivalence + buffer-intact; dropped-alias error path with exact message;
no surviving third path; consumer sweep.

### R-13: AC#10 token-reduction measurement vacuous or brittle
**Severity:** Med · **Likelihood:** Med
**Impact:** `tokens(default) ≤ 0.20 × tokens(transcript-json)` proves nothing against an empty/tiny buffer
(both sides ~equal — a vacuous pass, cf. #3548); a hardcoded absolute byte threshold flakes as fields
evolve.

**Test Scenarios:**
1. **Representative populated fixture:** a buffer with realistic candidate volume (~62 KB order) so the
   `transcript`-bearing json response is genuinely candidate-heavy; assert the ≥80% reduction ratio.
2. **Ratio, not absolute:** assert the ratio between the two same-cycle responses, not an absolute count.
3. **Vacuity guard:** assert the `transcript`-json response is materially larger (candidates non-empty)
   before asserting the ratio, so an empty-buffer run fails loudly.

**Coverage Requirement:** Populated fixture, ratio assertion, empty-buffer vacuity guard.

### R-14: crt-055 fold double-count
**Severity:** Med · **Likelihood:** Low
**Impact:** The content-opaque fold is a buffer read on the common path that now NEVER purges, so repeated
non-destructive reviews re-read the same buffer. A non-idempotent fold double-counts durable
`cycle_review_index` metrics (SR-12).

**Test Scenarios:**
1. **Repeated reviews idempotent:** run a default (or `transcript:{}`) review 3× on the same cycle; assert
   `cycle_review_index` fold metrics are stable across reviews (no accumulation).
2. **Content-opacity holds:** the fold output stays counter-only (`bytes_total`, `delta_count`,
   `class_counts`), no byte-bearing field.

**Coverage Requirement:** Idempotency across repeated non-purging reviews; fold output content-free.

### R-15: Residency-over-long-merge-window fidelity loss
**Severity:** Med (High if silent) · **Likelihood:** Med
**Impact:** Holding the cycle open across a multi-day merge lets older dev-phase buffers hit the 24h TTL
or 64-cap and reclaim to `Reconstructed`/empty **before** the post-merge retro. Accepted + graceful
(ADR-005): the report is buffer-independent, so only verbatim candidates degrade, bounded by unchanged
cap+TTL — keeping the cycle open does NOT extend buffer life. The real risk is the loss being **silent**,
indistinguishable from a bug.

**Test Scenarios:**
1. **Aged-buffer degradation is visible:** age/evict a subset of dev-phase buffers, then run the
   post-merge retrieval. Assert aged sessions are `Reconstructed`/empty with per-session loss surfaced
   (`elided_bytes`/`has_holes`/`dropped_candidates`), NOT missing without trace, NOT a crash (ties R-01).
2. **Fresh vs aged partition:** one cycle with fresh + aged buffers → fresh yield `Primary`, aged yield
   `Reconstructed`/empty-with-loss; the response distinguishes them.
3. **Open cycle does not extend buffer life:** buffers governed solely by cap+TTL, independent of cycle
   open/closed (ties R-06 backstop reclamation).

**Coverage Requirement:** A long-merge-window scenario asserting aged degradation is surfaced
(`Reconstructed`+loss), never silent, never a crash; plus the residency-envelope-bounded assertion.
Silence (candidates absent with no loss signal) is a failing outcome.

### R-16: Degraded / second-retrieval path crash or stale verbatim
**Severity:** High · **Likelihood:** Low
**Impact:** A repeat retrieval, or retrieval after TTL/eviction/partial-drain, must yield empty or
`Reconstructed`-only candidates — never a panic, never stale verbatim from a reused buffer slot (which
would re-emit already-reclaimed, possibly secret-bearing content).

**Test Scenarios:**
1. **Second retrieval same cycle:** buffer survives the first (non-destructive), second returns the same
   candidates; after a backstop reclaims, retrieval returns empty/`Reconstructed`, no panic.
2. **Post-reclamation retrieval:** simulate cap-eviction / TTL sweep, then retrieve → empty/`Reconstructed`,
   no crash, no verbatim.
3. **Partial buffer:** `elided_bytes`/`holes` → `Reconstructed` provenance + per-session loss; assert no
   verbatim text in a hole region.
4. **No stale bytes:** a post-reclamation retrieval's candidates do not equal the pre-reclamation verbatim.

**Coverage Requirement:** Degradation exercised for each source-gone cause (TTL, cap-evict, partial drain);
each asserts no-panic AND (empty | `Reconstructed`), plus the no-stale-verbatim assertion.

### R-17: ADR amended via deprecate+store instead of `context_correct`
**Severity:** Low · **Likelihood:** Med
**Impact:** Deprecate+store breaks the #4742/#4857 provenance chain (project rule "context_correct for
updates"). Verification-time, not runtime.

**Test Scenarios:**
1. Verify the amendment used `context_correct` on #4742 and #4857 (chain intact) and states: purge removed
   / fully non-destructive; residency bounded by unchanged cap+TTL; disk posture unchanged; `force`/`format`
   orthogonal; fold read the sole surviving side-effect (AC-15).

**Coverage Requirement:** ADR content covers all points; amendment mechanism is `context_correct`.

### R-18: NG-7 scope creep / force non-orthogonality
**Severity:** Low · **Likelihood:** Low
**Impact:** Distilling transcript signal INTO the report body is ass-090 scope (NG-7); leaking it here
couples crt-057 to deferred work. Separately, a `force` early-return that also short-circuits retrieval
would break `force`+`transcript` orthogonality.

**Test Scenarios:**
1. **Report-body invariance under buffer state:** the report body is byte-identical whether the buffer is
   present, partial, or gone (the A-1 invariant) — the strongest guard that no transcript signal entered
   the summary.
2. **force × transcript orthogonal:** `force:true` + `transcript:{}` recomputes the report from durable
   observations AND returns the scoped slice; `force:true` alone recomputes with NO candidates and buffer
   intact. `force` never reaches or purges the buffer.

**Coverage Requirement:** Report-body-invariance-under-buffer-state; force+transcript co-occurrence and
force-alone-no-retrieval; zero in-summary-distillation scenarios.

---

## Integration Risks

- **Cross-plane clock seam (R-05, #3385/#3372).** The Plane A epoch-millis ↔ Plane B JSONL `ts` join is
  the highest-subtlety seam: independent clocks for `Primary` sessions, `ts:None` candidates, a windowed
  (never exact) join, and a `byte_offset` fallback. Route it through a named boundary conversion helper so
  the unit mismatch cannot silently recur; test with explicit offsets and on/inside/outside boundaries
  (#4195/#4236), never `now_ts()`.
- **Four-site fold-read seam (R-07, #4585/#4750).** The fold is now the ONLY gated success side-effect;
  memo-hit (site 3) is the highest-drift-risk boundary. Behavioral, path-proven rows — not source
  counting — are the enforcement (ADR-004).
- **Server↔consumer semantic seam (R-02, #5383).** The contract lives half in Rust (`transcript{}`) and
  half in skill/tool-description/protocol prose across the four-doc atomic unit. The integration test must
  drive the reconciled retro call end-to-end (reachability), not test the server surface in isolation
  (store-layer read-back is a false positive).
- **Cycle-lifecycle ↔ transcript-hold seam (R-04, R-08).** `context_cycle(stop)` is a lifecycle marker;
  the hold store is keyed by `feature_cycle`, disjoint from open/closed state. merge→close→retro rests on
  close being inert; verified only by an end-to-end simulated-cycle test per protocol, not by a Rust unit
  test on crt-057's handler.
- **Orphan-deletion ↔ retention-exhaustiveness seam (R-06, #4831).** The exhaustive `TranscriptRetention`
  match relocates from the deleted purge onto the surviving backstops; a dropped arm or a `_` catch-all is
  a silent retention-behavior change.
- **Backstop-sole-reclamation seam (R-06, R-15).** With the eager purge gone, TTL/cap/session-close carry
  the full reclamation load; a regression means unbounded residency (secrets) with no visible failure.
- **Non-purging common path ↔ crt-055 fold (R-14).** A repeated reader over a surviving buffer makes fold
  idempotency newly load-bearing.

## Edge Cases

- No-match over a session with each single loss signal (`elided_bytes>0`, `has_holes`, `Reconstructed`,
  `dropped_candidates>0`) → INDETERMINATE, per condition (R-01).
- No-match over a clean `Primary` session → trustworthy negative, session may be omitted (R-01).
- Anchor/phase window containing a `ts:None` candidate → included via `byte_offset` fallback, flagged (R-05).
- Skewed Plane-B `ts` at the window edge (just-inside / on / just-outside) → boundary-correct selection (R-05).
- `transcript:{}` ≡ `match:".*"` full dump under the cap; second identical call returns the same set (R-09).
- Self-bounding `phase` with a supplied `window` → `window` ignored (R-09).
- Empty scope result → candidates section absent (not null), no crash (R-09).
- Invalid `match` regex / unknown `format` (incl. `"summary"`) → `ERROR_INVALID_PARAMS`, exact message (R-09/R-12).
- Second retrieval of the same cycle → same candidates (buffer survived); after backstop reclaim →
  empty/`Reconstructed`, no crash, no stale verbatim (R-16).
- Never-reviewed cycle → backstop (TTL/cap/session-close) reclaims with a content-free audit (R-06).
- `context_cycle(stop)` before retrieval → buffers survive; post-close retrieval still returns candidates (R-08).
- Cycle held open across a multi-day merge → aged dev-phase buffers degrade to `Reconstructed`/empty with
  loss surfaced, never silently absent (R-15).
- Single protocol reconciled but not the other → gate fails (R-04).
- `force:true` + `transcript:{}` → report recomputed from durable data AND scoped slice returned; buffer
  intact (R-18).

## Security Risks

**Untrusted input:** the transcript buffer content itself — raw, possibly secret-bearing agent bytes
(NG-4: no redactor; accept-and-drop + in-memory + system-purge IS the secrets guarantee). Also the changed
params (`transcript{}` block incl. a caller-supplied `match` regex, `format`, `force`) and the
`feature_cycle` lookup key.

- **Persistence leak is the highest concern (R-03).** If buffer/candidate bytes — or the new
  `SessionLossInfo`/`search_complete` carrier — reach SQL/file/log, or a candidate slot lands on the
  memoized `RetrospectiveReport`, secrets persist durably outside the in-memory+purge guarantee
  (#4721/#4850). Content-scan every write sink on every changed path, INCLUDING
  reclamation-without-review; assert no long verbatim/secret-shaped run (#5089).
- **Longer residency raises resident secret volume (SR-01).** No path purges now; bytes reside until a
  backstop reclaims. Bounded by the unchanged 64-cap + 24h TTL, but steady-state resident volume rises.
  Test the residency envelope is bounded (never-reviewed cycle IS reclaimed — R-06), so it cannot grow
  unbounded.
- **Stale-verbatim re-delivery (R-16).** A degraded path returning reused buffer bytes would re-emit
  already-reclaimed secrets. Assert post-reclamation candidates never equal pre-reclamation verbatim.
- **`match` regex as untrusted input.** A caller-supplied regex runs over candidate text; assert an
  invalid regex is rejected (`ERROR_INVALID_PARAMS`), not panicking, and consider catastrophic-backtracking
  exposure — the regex is caller-controlled and runs over potentially large candidate blocks. Flag ReDoS
  surface to the delivery leader (compile-time complexity bound or a size guard).
- **`feature_cycle` as a lookup key:** opaque registry/hold key, not a filesystem path or SQL fragment —
  no path-traversal/injection surface identified. Confirm no changed path interpolates it into a log/audit
  alongside content.

## Failure Modes (expected behavior when a risk materializes)

- **Buffer lossy (elided / holes / Reconstructed / dropped):** every returned session carries
  `SessionLossInfo`; a no-match is `search_complete:false` → INDETERMINATE, never a bare false. (R-01)
- **Cross-plane skew / `ts:None`:** windowed join absorbs skew; `ts:None` included via `byte_offset`
  fallback and flagged; NEVER a silent drop. (R-05)
- **Source gone (TTL / evicted / partial / session-closed):** return empty or `Reconstructed`-only with
  per-session loss visibility. NEVER panic; NEVER stale verbatim. (R-16)
- **Any parameter combination:** NO purge — the buffer survives every path; reclamation only at the
  backstop. A review that reaps the buffer is a regression, not a mode. (R-06, R-08)
- **Unknown / dropped `format`:** `ERROR_INVALID_PARAMS`, exact valid-values message; no partial render. (R-12)
- **Never-reviewed cycle:** backstop reclaims with a content-free terminal audit; the audit must not imply
  a review occurred. (R-06)
- **Empty scope result:** candidates section absent (not null); no crash. (R-09)
- **Cycle closed before retrieval (locked order):** `context_cycle(stop)` is a lifecycle marker only —
  buffers survive; post-close `/uni-retro` retrieval delivers candidates. A close that reaps buffers is a
  regression. (R-08)
- **Dev-phase buffer aged before the post-merge retro:** candidates degrade to `Reconstructed`/empty with
  loss surfaced; NEVER silently absent, NEVER a crash. Report body unchanged. (R-15)

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution / Coverage |
|-----------|------------------|----------------------|
| SR-01 memory-residency lengthening | R-03, R-06, R-15 | Residency-envelope-bounded (never-reviewed cycle reclaimed by backstop, R-06); content-scan on the longer-lived buffer's sinks (R-03); long-merge aging surfaced as `Reconstructed`+loss (R-15). No path purges; bounded by unchanged cap+TTL. |
| SR-02 secrets/audit posture drift | R-03, R-06 | Content-free terminal audit asserted at the backstop (now the sole audit point); sink content-scan on all changed paths incl. reclamation-without-review (R-03 sc.2-3). |
| SR-03 backstop is the sole loss mode | R-06, R-15 | Never-reviewed-cycle reclamation (R-06); long-merge aging made VISIBLE via loss propagation (R-15/R-01). Named as the primary loss mode, not a regression. |
| SR-04 consumer-reconciliation coupling | R-02, R-04, R-08 | Atomic unit: server + `uni-retro/SKILL.md` + tool description + both protocols (`uni-agent-routing.md` DROPPED). End-to-end harvest-fires test + four-doc grep guard (R-02); per-protocol lifecycle end-to-end (R-04); close-non-purging ordering (R-08). Dominant → Critical. |
| SR-05 silent false negative (raison d'être) | R-01 | Per-loss-condition matrix proving `search_complete==false` for each of `elided_bytes>0`/`has_holes`/`Reconstructed`/`dropped_candidates>0` + OR-combination; clean-session trustworthy-negative; no bare-bool no-match (R-01). The richest coverage in the suite. |
| SR-06 ass-090 / NG-7 line | R-18 | Report-body-invariance-under-buffer-state (R-18 sc.1); zero in-summary-distillation scenarios. |
| SR-07 no destructive axis + dead `"summary"` | R-12, R-06 | Render-equivalence + `"summary"`→`ERROR_INVALID_PARAMS` + no-surviving-third-path (R-12); no-purge-verb on any path/param (R-06 sc.4, R-10). |
| SR-08 clock skew (first-class interface) | R-05 | Skewed-clock windowed join; `ts:None` `byte_offset` fallback; window default + override + phase-ignores-window; explicit-offset boundary tests; agent never supplies Plane B's clock (R-05). |
| SR-09 four-site fold-read lockstep | R-07, R-11 | Path-proven per-site fold rows incl. memo-hit (R-07); surviving ×4 counts + rationale-bearing removal of the purge assertions (R-11). |
| SR-10 orphan deletion + exhaustive-match re-home | R-06, R-11 | Dead-code removal proven; backstops reclaim after the eager purge is gone; exhaustive match re-homed, retention byte-unchanged (R-06); grep for hidden dependents on the removed strings (R-11). |
| SR-11 two-protocol lifecycle blast radius | R-04, R-08 | Per-protocol end-to-end (delivery AND bugfix) proving open-through-merge → close-after → post-close retrieval delivers candidates (R-04); close-non-purging ordering guard (R-08). |
| SR-12 crt-055 fold idempotency | R-14, R-07 | Idempotency across repeated non-purging reviews; fold output content-free (R-14). |
| SR-13 ADR-amendment consistency | R-17 | `context_correct` on #4742/#4857; ADR states purge-removed / residency / disk-unchanged / orthogonality / fold-sole-side-effect (R-17, AC-15). |
| SR-14 rebase conflict on `distill_handler.rs` | — | Not an architecture risk. Flagged to the delivery leader (C-8): confirm no live conflict before delivery. |

### Retired / Superseded Boolean-Era Risk Rows

| Prior (boolean-era) risk | Disposition | Rationale |
|--------------------------|-------------|-----------|
| Memo-hit site silently ignores the FLAG (old R-01) | **SUPERSEDED by R-07** | There is no flag to thread. Scope threading is response-decoration (behaviorally caught, ADR-004); the four-site concern is now the content-opaque FOLD read, not a flag. |
| force×flag composition — `force` short-circuits before the distill/**purge** gate (old R-05) | **RETIRED** (residual in R-18) | No purge gate exists. `force`×`transcript` orthogonality is now disjoint read-only state — nothing to arbitrate; the reduced concern (force early-return skipping retrieval) folds into R-18. |
| Purge mis-keyed on `section.is_some()` (old R-12) | **RETIRED** | No purge at all — no keying to get wrong. |
| Purge granularity — "what granularity should the purge have?" | **DISSOLVED** | ass-091 ★ note: once retrieval is a read-only `snapshot()`, the destructive job leaves the review entirely. NG-6: no purge verb. |
| OQ-2 premature-extraction warning / self-exclusion (old R-10) | **DEMOTED** | ADR-003: retrieval is repeatable and non-destructive, so a live sibling is low-stakes; the warning is an optional advisory, NOT a contract. The honesty mechanism is now loss propagation (R-01). |
| Stale verbatim from a post-**purge** reused buffer (old R-06) | **REFRAMED into R-16** | No purge to reuse-after; the residual (stale/degraded bytes) now arises from aged/evicted/partial buffers. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios (min) |
|----------|-----------|--------------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 17 |
| High | 6 (R-05, R-06, R-07, R-08, R-09, R-10) | 24 |
| Medium | 6 (R-11, R-12, R-13, R-14, R-15, R-16) | 17 |
| Low | 2 (R-17, R-18) | 3 |
| **Total** | **18** | **61** |

**Cross-cutting test-construction constraints (apply to the whole matrix):**
- **Loss propagation is structural, not incidental (R-01):** every `match` no-match path is exercised over
  each loss condition; no path may return a bare `matched` without `SessionLossInfo`.
- **Every negative assertion** ("no purge", "no candidates", "buffer intact") keys on synchronous state,
  never on the absence of an async audit (R-10, #4879).
- **Every path-specific row proves which of the four success returns executed** (no vacuous pass through
  the full-pipeline path; #4452) — non-negotiable for the memo-hit fold row.
- **Clock/window tests use explicit fixed offsets and on/inside/outside boundaries** (#4195/#4236), never
  `now_ts()`; the cross-plane join is windowed, never exact.
- **AC#10 uses a populated fixture and a ratio**, guarded against the empty-buffer vacuous pass (#3548).
- **The lifecycle + consumer changes are verified end-to-end and PER protocol** (delivery AND bugfix) —
  a server-only or single-protocol green suite does not satisfy the gate; drive the reconciled retro call
  for reachability (#5383), not a store-layer read-back.
- **No dead code:** the orphaned purge functions are DELETED (not `#[allow]`ed), and the exhaustive
  retention match is re-homed with every variant covered (#4831).
- Extend existing `distill_handler.rs` fixtures; no isolated scaffolding (C-7).

## Knowledge Stewardship

- **Queried:** `context_search` (lesson-learned / pattern / decision, feature crt-057) for silent false
  negative + loss propagation, cross-plane clock skew, and protocol/orphan-deletion blast radius.
  Findings applied:
  - #3385 / #3372 (col-024 named unit-conversion helper at table boundaries) → R-05 recommendation to route
    the Plane A↔B join through a named boundary helper.
  - #4195 (time-window boundary tests flap under parallel runs — explicit offsets) + #4236 (epoch three-tier
    boundary suite) → R-05 clock-test construction constraint.
  - #5383 (vnc-042: verify MCP-handler-change call-site reachability; store-layer read-backs are false
    positives) → R-02/R-04/R-06 end-to-end reachability framing.
  - #4831 (enum-variant exhaustive-match gotcha) → R-06 `TranscriptRetention` re-home coverage.
  - #4044 (grep every hidden site on field add/remove) + #4915 (a checklist can't prove completeness over a
    code-derived set — add a code-derived cross-check) → R-11 source-assertion-removal + R-04 protocol-parity grep.
  - Carried from the prior strategy (still valid): #4879 (negative `==0` can't be poll-stabilized → R-10),
    #4585 (lockstep drift → R-07), #4452 (vacuous-pass path → R-07/R-01), #3548 (coverage vacuity → R-13),
    #5089 (secret-scan regression-guard shape → R-03).
- **Stored:** deferred — see the agent report. Candidate cross-feature pattern (2nd-feature confirmation
  pending, per the "patterns across 2+ features" rule): *"A read-only-retrieval redesign that removes a
  destructive verb relocates the risk from 'did the destructive gate fire correctly' to 'is the negative
  result honest (loss-propagated) AND did the now-sole backstop reclamation stay correct' — re-derive the
  register around silent false-negatives and orphan-deletion, do not port the old destructive-path rows."*
- **Declined:** storing crt-057-specific risks (they live in this document); #4750/#4585/#4879 (reused
  invariants) already exist.
