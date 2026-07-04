# crt-057 Specification: Fully Non-Destructive `context_cycle_review` with Scoped, Honest Transcript Retrieval

**Feature:** crt-057
**Tracking:** GH #894 (promoted from bug #871, closed)
**Phase:** Cortical (crt) — Learning & drift
**Source scope:** `product/features/crt-057/SCOPE.md` (REWORKED by human 2026-07-04 after research spike ass-091 / #898)
**Source architecture:** `architecture/ARCHITECTURE.md` + `architecture/ADR-001..ADR-006`
**Design source:** `product/research/ass-091/FINDINGS.md` (headline design + ★ non-destructive note; Q1 data-plane map, Q3 scoped-retrieval mechanism)

> **MAJOR REWORK (2026-07-04).** This specification is a full replacement of the prior boolean-era spec. The
> superseded contract — a fused `include_transcript_candidates` boolean that both *emitted* candidates and
> *triggered a purge* — is dissolved. The transcript axis is now a **read-only scoped retrieval**
> (`transcript: { phase?, anchor?, match?, window? }`), and `context_cycle_review` **loses its purge verb
> entirely**. All boolean-era FRs/ACs (`include_transcript_candidates`, purge-iff-flag, force-vs-extract
> precedence, extractive-vs-non-extractive review states, the OQ-2 refuse-vs-warn extraction question, the
> capture advisory) are RETIRED. FR/AC renumbering is recorded in the closing block.

This spec is written to the reworked SCOPE and the ADR set; it does not relitigate them. Where a detail is
deferred to pseudocode/delivery (exact `Window` shape, `r#match` serde-rename, `distill_before_purge` rename,
anchor/phase id representation), the spec records the requirement envelope and marks the open decision.

---

## Objective

`context_cycle_review` today does two ungated things on the common path: it appends the full crt-052 raw
candidate payload to every response (pushing the `markdown` default to ~75 KB, ~88% raw bytes, breaking
vnc-011 AC#10's ≥80% token reduction), and it purges the in-memory transcript buffers on the first successful
review (`purge_cycle_transcripts` at the four success returns), permanently destroying the only source of
verbatim candidates. crt-057 restores a lean, non-destructive default and makes the review **fully
non-destructive** — the eager purge is removed with no purge verb of any kind, reclamation delegated entirely
to the unchanged backstops. The transcript payload becomes a **scoped, read-only, AND-composed retrieval**
(`phase`/`anchor`/`match`/`window`) with per-session loss propagation and server-side cross-plane clock
normalization, so a caller queries in its own units and never receives a silent false negative. The tool
exposes three mutually orthogonal axes — render (`format`), recompute (`force`), scoped retrieval
(`transcript`) — none of them destructive.

---

## Domain Models / Ubiquitous Language

Authoritative for all downstream agents (pseudocode, tester, risk strategist). Use these terms exactly.

### The two data planes (ass-091 Q1 — authoritative; do not re-derive)

- **Plane A — durable observations.** SQL-persisted hook tool-events; the source of record. The review summary
  (`RetrospectiveReport`) is **100% Plane-A-derived and buffer-content-independent**: `build_report()` takes no
  transcript argument, so the summary is a pure function of durable SQL and is `force`-reproducible
  byte-for-byte (A-1, re-confirmed by ass-091).
- **Plane B — in-memory `transcript_candidates`.** A per-session byte ring buffer, **never persisted to disk**
  (NG-1). Bounded: 4 MiB per-session ring-tail elision, 1 MiB per-frame clip, 64-session hold cap, 24h TTL,
  `Primary` vs `Reconstructed` (~0.81 fidelity floor) provenance. Read at exactly one seam.
- **The content-opaque fold** — the one durable transcript-derived survivor: integer counters
  (`bytes_total`, `*_delta_count`, `*_error_count`, `*_refusal_count`, `signal_class_counts_json`) on the
  separate `CycleReviewRecord` (crt-054/#5030, crt-055/#5042). Integers only, never prose, not
  `force`-reproducible once the buffer is gone. **Never conflate this fold with Plane B raw content.**

**Design invariant (encode it): summary prose ⟂ Plane B content.** Every summary field is Plane-A-derived; the
scoped `transcript` block owns Plane B only and never touches summary derivation.

### The three axes (mutually orthogonal, no precedence, compose freely — NONE destructive)

| Axis | Parameter | Type / default | Meaning |
|------|-----------|----------------|---------|
| **Render** | `format` | `"markdown" \| "json"`, default `"markdown"` | Serialization of the report ONLY. Content byte-identical across formats. Never retrieves, never purges. `"summary"` alias dropped → `ERROR_INVALID_PARAMS`. |
| **Recompute** | `force` | `bool`, default `false` | Rebuilds the report from the durable observation table (bypass memoization). Never retrieves candidates, never purges. |
| **Scoped retrieval** | `transcript` | `Option<TranscriptScope>`, `#[serde(default)]`; omit = none | **READ-ONLY** scoped `snapshot()` over the existing candidate pipeline. Returns candidates + per-session `SessionLossInfo`. **Purges NOTHING.** |

### The `transcript` scope block

`TranscriptScope { phase?, anchor?, r#match?, window? }` — all optional, AND-composed filters over the EXISTING
candidate pipeline (the same `TranscriptCandidatesSection` the seam already produces), narrowed before attach.

- **`phase`** — candidates within a phase window; bounds from `cycle_events` (`CycleEventRecord`,
  `event_type == "cycle_phase_end"`); select `candidate.ts ∈ [phase_start, phase_end]`. **Self-bounding**
  (ignores `window`).
- **`anchor`** — a finding/anchor id; resolve to the finding's `HotspotFinding.evidence[].ts` span
  `[min, max]`; select candidates in `[min − window, max + window]`.
- **`match`** — regex over whole `TranscriptCandidate.text` blocks (survives truncation — a block is present
  whole or absent). Rust keyword → field is `r#match` / `#[serde(rename = "match")]` (pseudocode detail).
- **`window`** — ±N events or ±T time; modifies `anchor`/`match`; ignored by self-bounding `phase`. Default in
  Domain "Window default" below.
- **Omit `transcript`** = summary only (lean, non-destructive default). **`transcript: {}`** (present, all-None)
  = full candidate set under the existing per-cycle cap ≡ **`match:".*"`** — the degenerate full dump,
  non-destructive, already bounded. There is no separate whole-stream mode.

### Loss propagation vocabulary

- **`SessionLossInfo`** (exists: `observe/src/types.rs:633-646`) — per-session `elided_bytes`, `has_holes`,
  `provenance` (`Primary | Reconstructed`), `dropped_candidates`.
- **`matched: bool`** — whether a `match` hit the session.
- **`search_complete: bool`** — derived per session, `false` **iff** `elided_bytes > 0 || has_holes ||
  provenance == Reconstructed`. A response-transient derivation over existing loss fields; needs no new read.
- **INDETERMINATE** — a `match` no-match over a session with `search_complete == false`. Means "the buffer was
  lossy; the string could be past the 4 MiB tail, inside a hole, or absent from the 0.81-fidelity
  `Reconstructed` rebuild" — **NOT "didn't happen."** A clean `Primary` session with no loss reports a
  trustworthy negative; a clean session with nothing to report may be OMITTED (silence = nothing to report).
- `match` **MUST NOT collapse to a bare boolean** — that bare no-match over a lossy/`Reconstructed` session is
  exactly the silent false negative this redesign exists to prevent.

### Clock normalization

Plane A `EvidenceRecord.ts` is `u64` epoch-millis; Plane B `TranscriptCandidate.ts` is `Option<String>`
(JSONL) — **independent clocks for `Primary` sessions**. The caller queries in **its own units** (finding/anchor
id, phase id, regex, a window in events or time); Unimatrix parses each candidate `ts` to a **canonical epoch at
attach time** and joins over a **WINDOW, never an exact match**, absorbing the skew server-side. `ts:None`
candidates fall back to `byte_offset` proximity within the same session so they never escape the join silently.
**The caller never supplies or sees Plane B's storage clock.**

### Window default (ADR-006 / OQ-2)

When `anchor`/`match` is supplied and `window` is omitted: **±120 000 ms (±2 min)** for ts-bearing candidates;
**±3 candidate blocks** by `byte_offset` proximity for `ts:None`. Caller-overridable; bounded by the existing
per-cycle cap. Over-inclusion is the safe error direction; precision is not load-bearing (correctness holds at
any magnitude).

### Reclamation / residency vocabulary

- **Backstop reclamation** — the SOLE reclamation path: the 64-session hold cap, the independent 24h TTL
  stale-sweep (`sweep_expired`), and per-turn session-close purge. Unchanged by crt-057; they now carry the full
  load previously shared with the removed review-purge. Each still emits a content-free terminal audit
  (`transcript_session_purged`, `trigger=stale_sweep`/cap-eviction, bytes-only).
- **Residency posture (human-ratified).** Removing the review-purge lengthens raw-transcript residency from
  *gone-at-review* to *≤24h (TTL) / until 64-cap eviction / until session-close*. Still memory-only, still
  bounded, NG-1 intact.

### Structural terms

- **Four-site lockstep (#4750)** — `context_cycle_review` has four success returns (purged-signals,
  cached-metrics, memo-hit, full-pipeline). Success-only side effects are factored into shared helpers called
  identically at all four sites; source-assertion tests live at `distill_handler.rs:651-726`.
- **Memo-hit path** — the memoized-report success return (site 3); the easy-to-miss site that must honor the
  `transcript` block identically to the full-pipeline path.

### Ownership boundary (NG-5 — asserted, not built)

The `transcript` retrieval is a targeted tool the retro AGENT optionally uses to capture *what transpired*.
Unimatrix serves honest planes; the agent owns the synthesis. Unimatrix does NOT, and crt-057 does not build:
synthesizing/joining GH `## Knowledge Stewardship` blocks; manufacturing applied-entry attribution; the
rework-count ↔ cause join; or surfacing a human-intervention ledger. The tool returns the Plane-A summary + the
honest scoped Plane-B slice (candidates + `SessionLossInfo`) **only** — never a causal claim, never a
cross-plane join.

---

## Functional Requirements

Each requirement is testable; verification appears in the Acceptance Criteria section.

### Render axis (`format`)

- **FR-1** `format` accepts exactly `"markdown"` and `"json"`. The dead `"summary"` alias is **dropped**: it
  now falls to the `ERROR_INVALID_PARAMS` arm at all four render-dispatch loci
  (`tools.rs:2532, :3359, :4268, :4324`). Any other value likewise returns `ERROR_INVALID_PARAMS`. No third
  divergent render path survives.
- **FR-2** `format` is render-only: `"markdown"` and `"json"` produce identical report content, differing only
  in serialization. `format` never retrieves candidates and never purges.

### Recompute axis (`force`)

- **FR-3** `force:true` recomputes the report from the durable observation table, bypassing the memoized
  summary. `force:false` (default; `None` ≡ false) may return the memoized report.
- **FR-4** `force` is orthogonal to retrieval and to reclamation: it never reads or clears the transcript
  buffer, never retrieves candidates, and never purges — regardless of buffer state.
- **FR-5** Because the report is buffer-content-independent (A-1), a `force:true` recompute is fully
  reproducible after the buffer is reclaimed — the report body is byte-identical to a pre-reclamation recompute
  for the same durable observations.

### Scoped retrieval axis (`transcript`)

- **FR-6** `transcript` is optional (`#[serde(default)]`). When **omitted**, the response contains NO candidate
  block, the buffer is untouched, and the report is the observation-derived summary only (the lean,
  non-destructive default — restores AC#10).
- **FR-7** When `transcript` is **present**, the response contains a candidate section scoped by the supplied
  `phase`/`anchor`/`match`/`window` filters (AND-composed), attached out-of-band at assembly level. When the
  scope yields nothing, the section is **absent (not present-but-null)**.
- **FR-8** `transcript: {}` (present, all-None) returns the full candidate set under the existing per-cycle cap,
  equivalent to `match:".*"` — the degenerate full dump, non-destructive.
- **FR-9** Scoped retrieval is **read-only over the existing candidate pipeline** via the existing `snapshot()`
  (`session_transcript.rs:296`, already `&self`). **No new buffer reader is introduced** — the single-content-
  reader invariant (crt-052 ADR-002, #4848) is preserved. The block is a filter layer, not a new content path.
- **FR-10** Filter semantics: `phase` selects `candidate.ts ∈ [phase_start, phase_end]` from `cycle_events`
  bounds and is self-bounding (ignores `window`); `anchor` resolves to the finding evidence-ts span `[min, max]`
  and selects `[min − window, max + window]`; `match` is a regex over whole `TranscriptCandidate.text` blocks;
  `window` modifies `anchor`/`match` only. An invalid `match` regex returns `ERROR_INVALID_PARAMS`.

### Non-destructive review (no purge verb)

- **FR-11** `context_cycle_review` **NEVER purges** the transcript buffer on any path or parameter combination.
  There is **no purge verb** — not a flag, not a default. The eager review-triggered purge
  (`purge_cycle_transcripts`) is removed from all four success returns (`tools.rs:2379, 2558, 3328, 3451`). A
  second, identical review returns the same candidates (the buffer survived) until a backstop reclaims it.
- **FR-12** The content-opaque fold read (crt-054/055, #5030/#5042 — `activity_snapshots_for_feature`) still
  runs, gated at all four success returns per the #4750 pattern, as the **sole remaining review-seam success
  side-effect**. No candidate/buffer content reaches any SQL/file/log write.
- **FR-13** Reclamation is delegated **entirely** to the unchanged backstops (24h TTL sweep, 64-session
  cap-eviction, per-turn session-close). No new cycle-close purge trigger is added. The orphaned
  `purge_cycle_transcripts` and its now-unused helpers (`clear_transcripts_for_feature` /
  `purge_held_for_feature`) are deleted (anti-stub / dead-code, CLAUDE.md rule 2); the exhaustive
  `TranscriptRetention` match (C-5) relocates to the surviving backstop reclaim paths (`RetainDays` a no-op, no
  `_` arm).

### Loss propagation

- **FR-14** Every session returned in a `transcript` response carries its `SessionLossInfo`. Per returned
  session the response surfaces `matched`, `search_complete`, `elided_bytes`, and `provenance`.
- **FR-15** `search_complete` is derived `false` **iff** `elided_bytes > 0 || has_holes ||
  provenance == Reconstructed`. A `match` no-match with `search_complete == false` is reported as
  **INDETERMINATE**, never a bare false negative. `match` MUST NOT collapse to a bare boolean. All of it is
  response-transient, on the candidates channel, OUTSIDE `RetrospectiveReport` (summary ⟂ Plane-B invariant).
- **FR-16** For `anchor`/`phase`, the response returns the evidence-ts span / phase bounds that defined the
  window, and includes `ts:None` candidates via `byte_offset` proximity fallback so no candidate silently drops
  out of a windowed query.

### Clock normalization (interface + correctness requirement)

- **FR-17** The caller expresses its query in its own units (finding/anchor id, phase id, regex, a window in
  events or time). Unimatrix normalizes internally: it parses each candidate `ts` to a canonical epoch at attach
  time, joins over a window (never an exact match), and falls back to `byte_offset` proximity for `ts:None`
  candidates. The caller never supplies or sees Plane B's storage clock.
- **FR-18** When `anchor`/`match` is supplied and `window` is omitted, the default window is **±120 000 ms** for
  ts-bearing candidates and **±3 candidate blocks** (`byte_offset` proximity) for `ts:None`. It is
  caller-overridable and bounded by the existing per-cycle cap.

### Consumer reconciliation & retro lifecycle (must ship with the server change — see Constraints)

The atomic unit is: server change + `uni-retro/SKILL.md` + the `context_cycle_review` tool description + BOTH
protocol files (`uni-delivery-protocol.md`, `uni-bugfix-protocol.md`). `uni-agent-routing.md` is NOT an active
consumer and is excluded.

- **FR-19** `uni-retro/SKILL.md` is reconciled so the candidate-bearing call uses the `transcript{}` block
  (not the old boolean). The retro's call is the `transcript: {}` full-candidate block (retro captures the whole
  retained set, then owns synthesis per the Ownership Boundary). Because retrieval is repeatable and
  non-destructive, the retro may retrieve as often as it needs, in any scope. No reference implies the old
  any-review-carries-candidates or purge-on-review behavior.
- **FR-20** The `context_cycle_review` tool description documents the three orthogonal axes — `format`
  render-only; `force` durable recompute (never retrieves, never purges); `transcript{}` read-only scoped
  retrieval returning candidates + `SessionLossInfo`, purging nothing — and **states plainly that the tool has
  no purge verb**.
- **FR-21** (pr-review phase — both protocols) Both `uni-delivery-protocol.md` (phase `pr-review`) and
  `uni-bugfix-protocol.md` (phase `bug-review`) keep a distinct review phase **OPEN through the human merge
  decision** — the cycle is NOT stopped at the end of that phase. The human merge gate is unchanged.
- **FR-22** (close after merge — both protocols) In both protocols, `context_cycle(type:"phase-end")` for the
  review phase followed by `context_cycle(type:"stop")` runs **only AFTER the human merges**. No
  `context_cycle(type:"stop")` fires ahead of the merge decision.
- **FR-23** (`/uni-retro` post-close — both protocols) Both protocols invoke `/uni-retro` **post-merge, after
  cycle-close**. Strict ordering: **merge → close cycle → retro**. Because both review AND cycle-close are
  non-destructive (ADR-005: `context_cycle(type:"stop")` drains only the retrospective queue and writes an
  audit row — it touches no buffer), the post-close retro reads an intact buffer and may retrieve repeatedly and
  non-destructively; `take_transcripts_for_feature`/`snapshot()` scan buffers by the `feature_cycle` string,
  independent of cycle open/closed state.
- **FR-24** (ADR amendment) An amending ADR is stored (amends #4742 and #4857 via `context_correct`, not
  deprecate+store) recording: (a) the eager review-purge removed and the review fully non-destructive with no
  purge verb; (b) the memory-residency change bounded by the UNCHANGED 64-cap + 24h TTL + session-close, no new
  cycle-close trigger; (c) disk posture unchanged (NG-1); (d) `force`/`format` orthogonal to retrieval; (e) the
  content-opaque fold read remains the sole review-seam side-effect. (Recorded as ADR-001; retro-lifecycle in
  ADR-005.)

### Ownership boundary (negative requirement, NG-5)

- **FR-25** Unimatrix does NOT synthesize or join GH `## Knowledge Stewardship` blocks, does NOT manufacture
  applied-entry attribution, does NOT perform the rework-count ↔ cause join, and does NOT surface a
  human-intervention ledger. `context_cycle_review` returns the Plane-A observation summary + the honest scoped
  Plane-B slice only — no causal claim, no cross-plane join. crt-057 exposes exactly one retrieval axis and
  nothing that interprets or attributes.

---

## Non-Functional Requirements

- **NFR-1 (restored vnc-011 AC#10 — token reduction).** The default response (no `transcript`, `markdown`)
  achieves ≥80% token reduction versus the full candidate-bearing JSON response for a typical review. Measurable
  target: `tokens(default) ≤ 0.20 × tokens(transcript_full_json)`, asserted by a measured test.
- **NFR-2 (no new persistence path).** No transcript-buffer or candidate content reaches any SQL/file/log write
  on any changed path. The memoized `RetrospectiveReport` gains no candidate slot (candidates stay
  response-transient, attached out-of-band at assembly level — #4850). Loss-propagation fields are equally
  response-transient. Backstop reclamation stays content-free (SR-01/SR-02).
- **NFR-3 (bounded residency envelope, human-ratified).** With no review-purge, raw transcript bytes reside in
  memory longer on every path, bounded by the UNCHANGED backstops: worst-case resident volume = up to 64 held
  buffers × per-buffer cap-bytes, for up to the 24h TTL — behaviorally identical to "no review has run". No
  unbounded deferral; no new cycle-close purge trigger. Stated plainly in the ADR-001 amendment.
- **NFR-4 (hot-path safety).** The common (no-`transcript`) review path adds no new I/O and no new locks. The
  one pre-existing buffer read on the common path — the content-opaque fold (crt-055, reads counters not
  content) — remains correct and idempotent now that the review never purges (no double-count of durable
  `cycle_review_index` integers across repeated non-purging reviews).
- **NFR-5 (secrets posture unchanged, NG-4).** No new content secret-scanner or redactor. Accept-and-drop +
  in-memory + system-purge remains the secrets guarantee. The crt-052 assembly-level attach posture (#4850) is
  preserved.
- **NFR-6 (four-site lockstep integrity).** The `transcript` scope threading and the surviving fold-read gate
  apply identically at all four success returns. The `distill_handler.rs:651-726` source-assertion suite passes
  or is deliberately updated with recorded rationale (the purge-count assertion is removed — no per-site
  forking; the fold-read four-site assertion is preserved).
- **NFR-7 (exhaustive retention match).** Where `TranscriptRetention` is still matched (surviving backstop
  reclaim paths after `purge_cycle_transcripts` deletion), the match stays exhaustive; `RetainDays` a no-op; no
  `_` catch-all arm (C-5).
- **NFR-8 (graceful degradation over a long merge window).** A dev-phase session buffer may age past the 24h TTL
  or be evicted past the 64-cap before the post-merge retro fires, degrading those candidates to
  `Reconstructed`/empty. This is independent of cycle open/closed state and, unlike the prior contract, is NOT
  compounded by any earlier purge (nothing is lost to a review, only to aging). Degradation is visible via loss
  propagation (FR-14/FR-15) — never silent, never a crash; the report body is buffer-content-independent, so
  only verbatim candidates degrade, never the summary; the retro may re-retrieve non-destructively until aging.
- **NFR-9 (file-size / lint hygiene).** Changes respect the 500-line file limit, `fmt`, and `clippy`; test
  changes extend existing `distill_handler.rs` fixtures — no isolated scaffolding.

---

## Acceptance Criteria

Each maps to a SCOPE AC (SCOPE AC-NN in the mapping column) and states a verification method. SPEC AC IDs are
authoritative and internally consistent.

| AC-ID | Criterion | Verification | ↔ SCOPE |
|-------|-----------|--------------|---------|
| **AC-01** (default no candidates) | Default response (no `transcript`, `markdown`) contains NO candidate block; buffer intact. | Integration test: call with defaults; assert no candidate section; assert buffer unchanged. | AC-01 |
| **AC-02** (scoped candidates present) | With `transcript` present, the response contains a candidate section scoped by `phase`/`anchor`/`match`/`window`; the section is **absent (not null)** when the scope yields nothing. | Tests: populated buffer + scope (section present, correctly narrowed); scope yielding nothing (section absent). | AC-02 |
| **AC-03** (fully non-destructive; no purge verb) | `context_cycle_review` NEVER purges on any path/param combination; there is no purge verb; a second identical review returns the same candidates; the eager purge is removed from all four success returns. | Spy/trace test across default, `json`, `force:true`, and every `transcript` shape: assert `purge_cycle_transcripts` never invoked; assert buffer intact after each; assert a repeat `transcript:{}` returns identical candidates. Source: assert the four `purge_cycle_transcripts(` calls are removed. | AC-03 |
| **AC-04** (fold read preserved) | The content-opaque fold read still runs, gated at all four success returns per #4750, as the sole remaining review-seam side-effect; no candidate/buffer content reaches any SQL/file/log write. | Test: assert the fold lands durable integers at each of the four returns (incl. memo-hit); audit assertion that all persisted rows are content-free. | AC-04 |
| **AC-05** (`transcript:{}` full dump) | `transcript:{}` (present, all-None) returns the full candidate set under the existing per-cycle cap, equivalent to `match:".*"`, non-destructively. | Test: `transcript:{}` and `transcript:{match:".*"}` return the same candidate set bounded by the cap; buffer intact after both. | AC-05 |
| **AC-06** (indeterminate no-match) | For `match`, each returned session reports `matched`, `search_complete` (false iff `elided_bytes>0 \|\| has_holes \|\| provenance==Reconstructed`), `elided_bytes`, and `provenance`. A no-match over a session with `search_complete==false` is reported as INDETERMINATE, never a bare false. `match` never collapses to a bare boolean. | Tests: no-match over a clean `Primary` session → trustworthy negative (`search_complete==true`); no-match over an elided/holed/`Reconstructed` session → INDETERMINATE (`search_complete==false`) with `elided_bytes`/`provenance` present. | AC-06 |
| **AC-07** (anchor/phase windowing + `ts:None`) | `anchor`/`phase` return the evidence-ts span / phase bounds that defined the window and include `ts:None` candidates via `byte_offset` proximity fallback; no candidate silently drops out. | Tests: `anchor:<id>, window:±N` and `phase:<id>` assert returned bounds and that a `ts:None` candidate inside the byte-proximity window is included. | AC-07 |
| **AC-08** (clock normalization) | An agent expressing its query in its own units (finding/anchor id, phase id, regex, event/time window) resolves correctly against skewed Plane-B `ts` without supplying or knowing Plane B's clock; candidate `ts` is normalized to a canonical epoch server-side; `ts:None` uses `byte_offset` fallback. | Test with fixture candidates whose JSONL `ts` is skewed from Plane-A `EvidenceRecord.ts`: assert an anchor query resolves the correct candidate via the windowed join; assert a `ts:None` candidate resolves via `byte_offset`. | AC-08 |
| **AC-09** (force orthogonality) | `force:true` is always accepted, performs a report-only recompute from durable observations, and NEVER retrieves candidates and NEVER purges — regardless of buffer state; the report is reproducible before and after buffer reclamation. | Test: `force:true` (no `transcript`) before and after backstop reclamation → identical report body, no candidate section, buffer untouched by `force`; `force:true` + `transcript` present → report recomputed AND scoped slice returned (orthogonal, no precedence). | AC-09 |
| **AC-10** (restored vnc-011 AC#10) | The default response achieves ≥80% token reduction versus the full JSON candidate-bearing response for a typical review. | Measured test: `tokens(default_markdown) ≤ 0.20 × tokens(transcript_full_json)`. | AC-10 |
| **AC-11** (format render-only) | `format:"json"` renders identical report content to `markdown` — no candidates, no purge; the two differ only in serialization. `format:"summary"` (and any unknown value) → `ERROR_INVALID_PARAMS` at all four render loci. | Test: run same cycle `markdown` vs `json`, assert semantic content equality and buffer intact after both; assert `"summary"` → `ERROR_INVALID_PARAMS`. | AC-11 |
| **AC-12** (four-site lockstep) | The `transcript` scope threading and the fold-read gate apply identically at all four success returns; the `distill_handler.rs:651-726` source-assertion tests pass (or are updated with recorded rationale — purge-count assertion removed, fold-read four-site assertion preserved). No per-site forking. Memo-hit (site 3) honors `transcript` identically to full-pipeline. | Run source-assertion tests; behavioral matrix row per site: memo-hit + `transcript` present → scoped candidates present, buffer intact; memo-hit + no `transcript` → no candidates, buffer intact. | AC-12 |
| **AC-13** (backstops unchanged, sole reclamation) | The 64-cap, 24h TTL sweep, and per-turn session-close purge are unchanged and are the sole reclamation path; no new cycle-close purge trigger is added. A never-retrieved cycle has its buffers reclaimed by a backstop with a content-free terminal audit. | Test a never-`transcript` cycle: assert buffer reclaimed by a backstop with content-free audit; assert no cycle-close purge path exists; assert `purge_cycle_transcripts` and orphaned helpers are deleted (dead-code/clippy clean). | AC-13 |
| **AC-14** (secrets posture / no new persistence) | Candidates and loss-propagation fields stay response-transient, outside the memoized report; the persisted `RetrospectiveReport` gains no candidate slot; the scoped-retrieval path creates no new persistence. | Test asserts persisted report struct has no candidate/loss field; audit assertion that reclamation and fold writes are content-free. | AC-14 |
| **AC-15** (ADR amendment) | An amending ADR amends #4742 and #4857 recording the purge removal, the fully-non-destructive review with no purge verb, the residency posture change (bounded by unchanged cap/TTL/session-close), and disk-posture-unchanged. | Verify stored ADR content covers all four statements; verify amendment uses `context_correct`, not deprecate+store. | AC-15 |
| **AC-16** (consumer reconciliation) | `uni-retro/SKILL.md` and the `context_cycle_review` tool description use the `transcript{}` block (the retro's candidate call is the `transcript:{}` full block), imply no purge-on-review / any-review-carries-candidates behavior, and the tool description states the tool has no purge verb. `uni-agent-routing.md` is excluded. | Doc review / grep: no residual `include_transcript_candidates` or "any review carries candidates" language; `transcript{}` present in both; assert `uni-agent-routing.md` absent from the atomic-unit ship. | AC-16 |
| **AC-17** (both protocols lifecycle restructure) | BOTH `uni-delivery-protocol.md` and `uni-bugfix-protocol.md` keep the review phase open through merge, close after merge (`phase-end` then `stop`), then invoke `/uni-retro`, ordering **merge → close cycle → retro**; the retro retrieves non-destructively (no one-shot sequencing). Human merge gate unchanged. | Verify each protocol: review phase not stopped pre-merge; human merge gate → `context_cycle phase-end`+`stop` → `/uni-retro`, in that order; trace test — stop the cycle, then run a `transcript` retrieval, assert candidates present (cycle-close touched no buffer). | AC-17 |
| **AC-18** (window default) | With `anchor`/`match` supplied and `window` omitted, the default window is ±120 000 ms for ts-bearing candidates and ±3 candidate blocks for `ts:None`; caller override is honored and cap-bounded. | Test: omit `window` → assert the ±120 000 ms / ±3-block selection bounds; supply an override → assert it is honored under the cap. | AC-06/AC-07 (mechanism, OQ-2) |
| **AC-19** (ownership boundary — negative) | `context_cycle_review` returns only the Plane-A summary + the honest scoped Plane-B slice. It does NOT emit synthesized GH stewardship joins, applied-entry attribution, a rework-count ↔ cause join, or a human-intervention ledger. | Test/inspection: assert the response schema contains no attribution/join/ledger field; assert no code path synthesizes across GH blocks. | NG-5 |

---

## User / Agent Workflows

1. **Human requests a cycle summary.** Calls `context_cycle_review` with defaults → lean observation-derived
   report, no raw bytes, buffer preserved, no purge. (Was: a ~75 KB dump + silent purge.)
2. **`/uni-retro` harvest (self-learning loop, #5219).** The candidate-bearing call uses `transcript: {}` (the
   full retained set under the cap), issued **post-merge, after cycle-close** → candidates returned
   non-destructively; the retro AGENT owns the synthesis (Ownership Boundary). It may re-retrieve any scope,
   repeatedly, until aging. Each returned session carries `SessionLossInfo` so a lossy/`Reconstructed` slice is
   honestly flagged.
3. **Scoped investigation.** An agent narrows to what it needs: `transcript:{ anchor:<finding id>, window:±N }`,
   `transcript:{ phase:<id> }`, or `transcript:{ match:"<regex>" }`. A `match` no-match over a lossy session
   reads as INDETERMINATE, not "didn't happen." The agent queries in its own units; the server normalizes clocks.
4. **Re-review / audit.** A later `force:true` review recomputes the report from durable observations (fully
   reproducible) with no candidates and no purge. Repeated reviews never destroy the buffer.
5. **Delivery / bugfix close (retro lifecycle).** The pr-review / bug-review phase stays open through the human
   merge decision; the human merges; the protocol closes the cycle (`context_cycle phase-end` → `stop`); then
   invokes `/uni-retro` post-close. Strict ordering: **merge → close → retro**. Because both review and
   cycle-close are non-destructive, the post-close retro reads an intact buffer. Applies to BOTH protocols.

---

## Constraints

- **CON-1 (consumer-reconciliation atomic unit, C-6/SR-04).** Server change (D-1..D-4) + `uni-retro/SKILL.md` +
  the `context_cycle_review` tool description + BOTH protocol files (`uni-delivery-protocol.md`,
  `uni-bugfix-protocol.md`) ship as ONE indivisible deliverable. `uni-agent-routing.md` is excluded (not an
  active consumer). Shipping the server change alone silently starves the candidate consumers (self-learning
  harvest #5219) with no error. AC-16 fails if any consumer still implies old behavior; AC-17 fails if either
  protocol omits merge → close → retro.
- **CON-2 (four-site lockstep, C-1/#4750).** The `transcript` scope threading and the surviving fold-read gate
  are expressed once and applied identically at all four success returns; per-site forking breaks the
  `distill_handler.rs:651-726` source-assertion tests. History (#4585) shows lockstep sites drift silently. The
  purge-count assertion is deliberately removed with rationale; the fold-read four-site assertion is preserved.
- **CON-3 (memo-hit parity, OQ-3).** The memo-hit path (site 3) must honor the `transcript` block identically to
  the full-pipeline path. `scope` threading at the memo-hit site is not source-assertable → explicit behavioral
  matrix rows required (AC-12).
- **CON-4 (single buffer reader, C-2/ADR-002 #4848).** Scoped retrieval reuses the existing `snapshot()`
  (`&self`); no new buffer reader.
- **CON-5 (secrets / retention invariants).** never-persist-raw-transcript-to-disk (#4721/#4850/#4742) is
  absolute; candidates + loss fields stay response-transient outside the memoized struct (#4850);
  reclamation audits stay content-free (#4742); `TranscriptRetention` match stays exhaustive with `RetainDays`
  a no-op (#4857 backstops untouched, C-5).
- **CON-6 (`"summary"` alias, SR-06).** The dead `"summary"` render alias is dropped (not folded) — `format`
  accepts exactly `markdown|json` with identical content; `"summary"` → `ERROR_INVALID_PARAMS` at all four loci.
- **CON-7 (rebase awareness, C-8/SR-12).** Prior work (bugfix-891 / bugfix-824) touched `distill_handler.rs`;
  confirm no live conflict before delivery. (Delivery-leader concern; flagged here.)
- **CON-8 (file/lint hygiene, C-7).** 500-line file limit, `fmt`, `clippy`; extend existing test fixtures — no
  isolated scaffolding.

---

## Dependencies

- **ass-091 (#898)** — the design source. Q1 data-plane map (authoritative; do not re-derive), Q3
  scoped-retrieval mechanism (the crt-057 blocker, resolved), ★ headline + ★ non-destructive design note.
- **crt-052 (#706)** — transcript_hold / candidates pipeline + `snapshot()` (the single content reader); amends
  its ADR-008 purge clause (#4857).
- **crt-054/#5030, crt-055/#5042** — content-opaque integer fold at the review seam (the sole surviving
  side-effect; must remain correct now that the review never purges — NFR-4).
- **vnc-024 / vnc-025** — retention enum + purge-point content-free audit; amends vnc-025 ADR-004 (#4742).
- **vnc-011 (#196 / #952)** — origin of `format`/markdown and AC#10 (the restored token-reduction target).
- **ass-090 (#896)** — DOWNSTREAM spike, re-sequenced to depend on ass-091; consumes the Q1 map and extends the
  fold; does not touch Plane B raw content (NG-7).
- **crt-056 `BackgroundJob` registry (#5167)** — the documented seam for a future async transcript-summary job
  (Q4); left open, built by no one here (NG-8).
- **Pattern #4750** — four success returns, shared-helper gating (the lockstep this feature reuses, now guarding
  only the fold read). **#4850** — candidates attached at response-assembly level (secrets anchor). **#4848** —
  single content reader.
- Stored ADRs to amend via D-5: **#4742**, **#4857**. Unchanged anchors: **#4721**, **#4850**.
- **Superseded:** the prior boolean-contract crt-057 ADRs (#5429/#5422/#5423/#5424) are reworked/deprecated in
  the design phase; the reworked ADR-001..ADR-006 supersede them.

---

## NOT in Scope (explicit exclusions)

- **NG-1: Persisting raw transcript content to disk in any form.** Absolute (#4721/#4850/#4742). This work
  touches only in-memory purge *timing*; no disk/SQL/file/log persistence of buffer bytes.
- **NG-2: Changing the 64-session cap, 24h TTL sweep, or per-turn session-close purge.** The backstops stay
  exactly as-is; they are the sole reclamation path.
- **NG-3: Changing distilled-knowledge / observation / audit retention.**
- **NG-4: Adding a content secret-scanner / redactor.** Accept-and-drop + in-memory + system-purge IS the
  secrets guarantee.
- **NG-5: Cross-plane synthesis, attribution, or human-ledger surfacing (agent-owned retro).** Unimatrix does
  not join GH stewardship blocks, manufacture applied-entry attribution, do the rework-count ↔ cause join, or
  surface a human-intervention ledger (asserted as FR-25 / AC-19).
- **NG-6: A purge verb on `context_cycle_review`.** No `purge:true` flag, no destructive default. Operator-
  triggered immediate reclamation, if ever needed, is a separate admin/ops verb — never a parameter on the
  review tool.
- **NG-7: Distilling transcript signal INTO the review summary.** Deferred to spike ass-090 (#896); must extend
  the content-opaque fold, must not touch Plane B raw content. Any AC that enriches the report body is a
  variance.
- **NG-8: Local inference over the transcript.** Q4 (local GGUF summarization) is feasibility-only and does not
  gate crt-057. Leave the seam (review-time opt-in + crt-056 `BackgroundJob` registry #5167); build nothing.
- **Fixing per-turn drain starvation / mid-session amnesia (#4799).** Consumption-timing correctness is not
  buffer completeness; the underlying drain behavior is unchanged. Loss propagation keeps residual incompleteness
  visible.

---

## Open Questions for Downstream Agents / Human

- **OQ-1 (live regex hit-rate / `ts:None` fraction — deferred, not blocking).** Unmeasurable read-only (Plane B
  never persisted). Folds into a delivery-time instrumentation experiment. The correctness contract (loss
  propagation, indeterminate no-match, `byte_offset` fallback, ±120 s/±3-block window default) holds regardless
  of the rate.
- **OQ-2 (anchor / phase id surface — pseudocode/spec detail).** The exact caller-facing identifier for
  `anchor` (finding id — the report labels findings e.g. `F-03`) and `phase` (phase name/id string) resolves
  against how the report exposes findings/phases. Resolution *path* is fixed (Domain / ADR-006); the id
  *representation* is a pseudocode detail.
- **OQ-3 (`Window` type shape + `r#match` serde-rename — pseudocode detail).** Exact enum/struct for `Window`
  (±events vs ±millis) and the `match` keyword handling. The default (±120 s / ±3 blocks) is fixed (FR-18).
- **OQ-4 (`distill_before_purge` rename — delivery decision).** Keep the vestigial name (preserves counted
  strings, minimal churn) or rename with a deliberate source-assertion-test update. Flag to the synthesizer.

---

## FR/AC Renumbering & Retirement (this rework)

This is a full replacement of the prior boolean-era spec. Recorded for traceability:

**Retired (boolean-era, now dissolved) — removed entirely:**
- The `include_transcript_candidates` boolean and all FRs governing it (old FR-6..FR-9, FR-13..FR-16).
- The purge-iff-flag contract and the "extractive vs non-extractive review" state model (old Domain section +
  old FR-8, AC-04).
- The force-vs-extract precedence / composition FR (old FR-14).
- The one-shot extraction + degraded-post-purge FRs (old FR-13, FR-16; old AC-06).
- The honest-capture advisory (old FR-17, AC-12) and the OQ-2 refuse-vs-warn premature-extraction WARN
  (old FR-18, AC-13, OQ-B) — the motivating one-shot purge no longer exists.
- Old AC-19/AC-20 (close-before-retro-yields-candidates as a purge-ordering concern; residency-trade-off framed
  around extraction) — folded into AC-03/AC-17 and NFR-8 under the non-destructive model.

**New / repurposed FRs:** FR-6..FR-10 now define the scoped `transcript` retrieval (was the boolean);
FR-11..FR-13 define the fully non-destructive review + backstop-only reclamation; FR-14..FR-16 define loss
propagation (per-session `SessionLossInfo`, `search_complete`, INDETERMINATE, anchor/phase `ts:None` fallback);
FR-17..FR-18 define clock normalization + window default (promoted from delivery carry-forward to a first-class
interface requirement); FR-19..FR-24 carry the consumer/lifecycle/ADR set forward under the new contract;
FR-25 asserts the ownership boundary as a negative requirement.

**AC alignment:** SPEC AC-01..AC-17 are aligned 1:1 with the reworked SCOPE AC-01..AC-17 (each row shows the
mapping) and add verification methods. **AC-18** (window default, ±120 s / ±3 blocks) and **AC-19** (ownership
boundary — negative) are net-new SPEC ACs grounding ADR-006 / OQ-2 and NG-5 respectively. KEPT and carried
forward unchanged in intent: AC-10 (≥80% token reduction), A-1 buffer-independence (FR-5 / NFR-4), the D-5/D-7
protocol-lifecycle (now simpler — AC-17), the D-4/D-6 consumer set (AC-16), NG-1 (NG-1 above), no-new-persistence
(NFR-2 / AC-14).

**No conflicts against the architecture.** The three-axis contract, the loss-propagation contract, the clock
normalization + ±120 s/±3-block window default, the four-site fold-only gating, and the both-protocols
merge→close→retro lifecycle are specified exactly as ADR-001..ADR-006 and ARCHITECTURE §1–§12 define them.

---

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_briefing` (crt-057) — surfaced #5434 (ADR-002 three-axis, read-only
  scoped `transcript{}`, no destructive axis), #5438 (ADR-006 scoped retrieval + clock normalization + ±120 s/
  ±3-block window default), #5435 (ADR-003 loss-propagation / INDETERMINATE no-match), #5436 (ADR-004 four-site
  seam now gates only the content-opaque fold read), #4848 (crt-052 single content reader / `snapshot()`),
  #4850 (candidates attached response-assembly level, outside memoized struct), #5031 (crt-054 survival-to-
  review obligation), #5037/#5042/#5051 (crt-055 fold writer), #5063/#5066 (cycle_events phase bounds; multi-
  source aggregate wiring). Read: reworked SCOPE.md, ARCHITECTURE.md, ADR-001..ADR-006, ass-091 FINDINGS.
- **Stored:** nothing — specification is a read-only tier; spec decisions are feature-specific. The amending
  ADRs belong to the architect (already authored as ADR-001..ADR-006).
