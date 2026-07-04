# crt-057 Architecture — Fully Non-Destructive `context_cycle_review` with Scoped, Honest Transcript Retrieval

Feature: crt-057 · GH #894 · Phase: Cortical · Session: design
Contract status: REWORKED (2026-07-04) after research spike **ass-091 (#898)** and human re-scope
approval. The prior locked boolean contract (`include_transcript_candidates` fused emit+purge) is
**superseded**. The transcript axis is now a **read-only scoped retrieval**; the review loses its
purge verb entirely. This document designs to the re-scoped `SCOPE.md`; it does not relitigate it.

Source docs: `SCOPE.md`, `product/research/ass-091/FINDINGS.md` (headline deliverable + ★ design
note), `FINDINGS-Q1.md` (data-plane map), `FINDINGS-Q3.md` (scoped-retrieval mechanism). Companion
ADRs: `ADR-001-purge-trigger-and-residency.md` (purge REMOVED + residency),
`ADR-002-three-axis-api-surface.md` (scoped-retrieval API surface),
`ADR-003-warn-and-proceed-on-incomplete-extraction.md` (loss-propagation contract),
`ADR-004-flag-gating-lockstep.md` (fold-read-only seam gating),
`ADR-005-retro-lifecycle-and-cycle-close.md` (simplified lifecycle),
`ADR-006-scoped-retrieval-and-clock-normalization.md` (retrieval mechanism + clock + window default).

---

## 1. System Overview

`context_cycle_review` (MCP tool, `unimatrix-server/src/mcp/tools.rs`) produces the retrospective
report that feeds `/uni-retro` and the self-learning harvest (#5219). Two problems, both born in
crt-052 (`ae9dbb53`) and carried by the prior boolean fix, are dissolved here:

1. **Bloat / lost lean default (#871).** An ungated candidate append pushed the `markdown` default to
   ~75 KB (~88% raw candidate bytes), breaking vnc-011 AC#10 (≥80% token reduction vs JSON).
2. **A destructive-by-default review.** `purge_cycle_transcripts` fired at the four success returns on
   `result.is_ok()` (#4750), so a routine review distilled candidates, discarded them, and **purged
   the only source** (buffers are memory-only, never persisted — crt-052 AC-06). A later review could
   produce only degraded `Reconstructed` candidates or none.

ass-091 (Q3, ★ design note) showed the prior boolean fused two separable jobs — *return the
transcript* and *reclaim the buffer*. Separating them removes the destructive job from the review
entirely: once retrieval is a read-only `snapshot()`, the purge has no reason to ride on the review.
crt-057 therefore exposes **three independent axes, none of them destructive**:

| Axis | Parameter | Effect | Touches transcript buffer? |
|------|-----------|--------|-----------------------------|
| **Render** | `format: "markdown" \| "json"` (default `markdown`) | Serialization of the report. Content identical either way. | No — never candidates, never purge |
| **Recompute** | `force: bool` (default `false`) | Rebuild the report from the durable observation table (bypass memo). | No — never candidates, never purge |
| **Scoped retrieval** | `transcript: { phase?, anchor?, match?, window? }` (optional; omit = none) | **READ-ONLY** scoped `snapshot()` over the existing candidate pipeline. Returns candidates + per-session `SessionLossInfo`. | Read-only snapshot; **purges NOTHING** |

The enabling fact (§5, A-1, re-confirmed by ass-091): the report is **100% observation-derived and
buffer-content-independent** (`build_report()` takes no transcript argument), so render and recompute
are cleanly separable from retrieval, and the report is `force`-reproducible byte-for-byte regardless
of buffer state. crt-057 changes *whether and how the buffer is read*; it never changes *what is
distilled into the report* (that is ass-090 / NG-7, out of scope).

**crt-057 provides exactly two things:** (1) the non-destructive Plane-A observation summary, and
(2) honest scoped retrieval of Plane B (candidates + loss). Nothing that interprets, joins, or
attributes across planes — see the Ownership Boundary (§10).

---

## 2. The Two Data Planes and the Summary ⟂ Plane-B Invariant (ass-091 Q1)

The design rests on the authoritative data-plane map from ass-091 Q1. Do not re-derive it.

- **Plane A — durable observations.** SQL-persisted hook tool-events are the source of record. The
  review summary (`RetrospectiveReport`) is **100% Plane-A-derived and buffer-independent**:
  `build_report()` takes no transcript argument, so the summary is a pure function of durable SQL and
  is `force`-reproducible byte-for-byte (A-1, §5).
- **Plane B — in-memory `transcript_candidates`.** A per-session byte ring buffer, **never persisted
  to disk** (NG-1; #4721, #4850, #5030). Bounded: 4 MiB per-session ring-tail elision, 1 MiB
  per-frame clip, 64-session hold cap, 24h TTL, `Primary` vs `Reconstructed` (~0.81 fidelity floor)
  provenance. Consumed at exactly one seam.
- **The one durable transcript-derived survivor** is a **content-opaque integer fold**
  (`bytes_total`, `*_delta_count`, `class_counts`) on the separate `CycleReviewRecord`
  (crt-054/#5030, crt-055/#5042) — integers only, never prose, not `force`-reproducible once the
  buffer is gone. Never conflate this fold with Plane B raw content.

**Design invariant (encode it):** *summary prose ⟂ Plane B content.* The scoped `transcript` block
owns Plane B **only** and never touches summary derivation. Every summary field is Plane-A-derived;
no narrative or metric path reads transcript-buffer content (§5).

---

## 3. Component Breakdown

| Component | File | Responsibility | Change in crt-057 |
|-----------|------|----------------|-------------------|
| `RetrospectiveParams` | `mcp/tools.rs:~431` | Deserialize tool params | **Remove** `include_transcript_candidates`; **add** `transcript: Option<TranscriptScope>` (`#[serde(default)]`); keep `format`, `force` |
| `TranscriptScope` | `unimatrix-observe` (new) or `mcp/tools.rs` | The scoped-retrieval filter block | **NEW**: `phase?`, `anchor?`, `r#match?` (regex), `window?` (see §7 Integration Surface) |
| `context_cycle_review` handler | `mcp/tools.rs:~2125` | Dispatch across four success returns | Thread the `transcript` scope; **delete the four `purge_cycle_transcripts` calls**; drop `"summary"` alias |
| `distill_before_purge` (shared helper) | `mcp/distill_handler.rs:48` | Candidate snapshot + selection (the ONE reader of buffer *content*) | Gains scope + clock-normalization; returns `None` when `transcript` absent; **name is now vestigial** ("before_purge" — no purge follows). Keep the name to preserve counted strings, or rename with a deliberate source-assertion-test update (delivery detail, §12) |
| `attach_to_response_assembly` (shared helper) | `mcp/distill_handler.rs:281` | Append candidates to the `CallToolResult` at assembly level | Signature unchanged — already no-ops on `None` |
| `purge_cycle_transcripts` | `server.rs:661` | Review-seam purge of registered ∪ held buffers | **All four review-site calls REMOVED.** Its only non-test callers are those four (verified) → **orphaned**; delivery deletes it + its now-unused helpers `clear_transcripts_for_feature` / `purge_held_for_feature` (§9, anti-stub) |
| Content-opaque fold read (crt-054/055) | `mcp/activity_fold_handler.rs`, `review_aggregates.rs`, `session.rs:566` (`activity_snapshots_for_feature`) | Durable integer fold on `CycleReviewRecord` | **UNCHANGED; STAYS gated at all four seam returns.** Now the SOLE surviving success side-effect (ADR-004). Strictly benefits — buffer survives, nothing lost sooner |
| TTL sweep / cap eviction / session-close | `infra/transcript_hold.rs` (`sweep_expired`, cap eviction), session lifecycle | Backstop reclamation | **UNCHANGED.** Now the SOLE reclamation path; carries the full load previously shared with the review-purge (C-4) |
| Consumers | `uni-retro/SKILL.md`, tool description, `uni-delivery-protocol.md`, `uni-bugfix-protocol.md` | Call the tool / document / drive the retro lifecycle | Reconcile to the scoped `transcript{}` block + simplified lifecycle (D-6/D-7). `uni-agent-routing.md` **excluded** (§8) |

---

## 4. How the Three Axes Compose (Data Flow)

```
                            context_cycle_review(params)
                                       │
        force? ───────────────┐        │        ┌─────────── format (render-only)
   (bypass memo, recompute    │        │        │   markdown | json → identical report content
    report from durable       ▼        ▼        ▼   ("summary" alias DROPPED → ERROR_INVALID_PARAMS)
    ObservationRecords)   ┌──────────────────────────┐
                          │  one of four success      │  build_report(records, …)  ← NO transcript arg
                          │  returns → report built   │  (report is buffer-content-independent, §5)
                          └──────────────┬───────────┘
                                         │  content-opaque fold read (crt-055) — SOLE side-effect,
                                         │  gated at all four returns, read-only, lands durable ints
                                         ▼
              section = distill_before_purge(reg, cycle, obs, cfg, scope, reviewer_session_id)
                          │
        transcript absent (scope None) ──► returns None  (no buffer read, no candidates — lean default)
        transcript present ─────────────► read-only snapshot() → scoped-filter (phase/anchor/match
                                          + window) → clock-normalize → candidates + per-session
                                          SessionLossInfo (§6)
                                         │
                                         ▼
              attach_to_response_assembly(&mut result, section)   (no-op on None)
                                         │
                                         ▼
              (NO purge — the review is fully non-destructive; the buffer survives)
```

Composition falls out with no precedence rule and **no destructive path anywhere**:

- **default** (no `transcript`, markdown): report only, no candidates, buffer intact.
- **json**: identical content to markdown, no candidates, buffer intact.
- **force:true (no `transcript`)**: report recomputed from durable observations, reproducible, no
  candidates, buffer intact. `force` selects *which report path runs*; it never reaches retrieval.
- **`transcript:{}`** (present, all-None): full candidate set under the existing per-cycle cap
  (≡ `match:".*"`), buffer intact. A second identical call returns the same candidates.
- **`transcript:{ phase?/anchor?/match?/window? }`**: scoped candidate subset + loss, buffer intact.
- **force:true + transcript present**: recompute the report from durable data AND return the scoped
  slice — orthogonal, nothing to arbitrate.

`force` × `transcript` are orthogonal because `force` operates on the report (durable observations)
and `transcript` operates on the buffer (read-only) — disjoint state, and neither purges.

---

## 5. A-1 Re-Verification — the report is buffer-content-independent (design-critical; re-confirmed by ass-091)

The render/force-vs-retrieval decoupling rests on assumption A-1, re-confirmed against ass-091 Q1 and
re-verified file:line in the crt-057 worktree:

- **`build_report`** (`crates/unimatrix-observe/src/report.rs:15-53`) has **no transcript argument**.
  `session_count` / `total_records` derive from the durable `ObservationRecord` set; every narrative /
  summary / recommendation field is filled from observations/signals, never from buffers.
- **Only non-test reader of buffer *content*:** `take_transcripts_for_feature`
  (`infra/session.rs:502-541`, reads via `buf.snapshot()`), with exactly one non-test caller —
  `distill_before_purge` (`mcp/distill_handler.rs:65`). It produces the
  `TranscriptCandidatesSection` and nothing else, attached out-of-band, never onto
  `RetrospectiveReport`.
- **The other review-time buffer reader is content-opaque:** `activity_snapshots_for_feature`
  (`session.rs:566`) via the crt-055 fold yields counters only (`bytes_total`, `delta_count`,
  `class_counts`) — no byte-bearing field. Persisted durably to `cycle_review_index`; reads no content.

**Result: A-1 HOLDS (ass-091-confirmed).** The report body is byte-identical whether or not the
buffer is present or complete; `force:true` rebuilds it from durable observations alone and is fully
reproducible; the sole pre-existing buffer read on the common path (the content-opaque fold) does not
depend on any destructive step and keeps working now that the review never purges.

---

## 6. Loss Propagation — no-match is never a silent false negative (see ADR-003)

Every session in a `transcript` response carries its `SessionLossInfo` (the type already exists —
§7). The scoped `match` contract, per returned session, surfaces:

- `matched: bool`
- `search_complete: bool` — derived `false` iff `elided_bytes > 0 || has_holes ||
  provenance == Reconstructed`. A no-match with `search_complete == false` is **INDETERMINATE**, not
  "didn't happen."
- `elided_bytes` and `provenance` alongside — high `elided_bytes` (past the 4 MiB tail) and
  `Reconstructed` each independently flag a negative as untrustworthy.

For `anchor` / `phase`: return the evidence-ts span / phase bounds that defined the window, and fall
back to `byte_offset` proximity for `ts:None` candidates so they never silently drop out (AC-07).

`match` MUST NOT collapse to a bare boolean — a bare no-match over a lossy/`Reconstructed` session is
exactly the silent false negative this redesign exists to prevent. This is the honesty mechanism of
the new contract; it replaces the prior "warn-and-proceed on destructive extraction" framing (which
was motivated by a one-shot purge that no longer exists — ADR-003).

---

## 7. Scoped Retrieval Mechanism + Clock Normalization (see ADR-006)

### 7.1 Retrieval is a read-only `snapshot()` over the EXISTING candidate pipeline

The `transcript` block layers AND-composed, all-optional filters onto the **same**
`TranscriptCandidatesSection` the seam already produces (`distill_before_purge`), narrowed **before**
`attach_to_response_assembly`. It reuses the existing `snapshot()` (already `&self`, non-mutating —
`session_transcript.rs:296`); **no new buffer reader is introduced**, respecting the single-content-
reader invariant (crt-052 ADR-002, #4848). The block is a filter layer, not a new content path.

| Field | Meaning | Resolution path |
|-------|---------|-----------------|
| *(omitted)* | Summary only — Plane A, buffer untouched (lean default). | `build_report` has no transcript input |
| `phase?` | Candidates within a phase window | Phase bounds from `cycle_events` (`CycleEventRecord`, `event_type == "cycle_phase_end"`); join `candidate.ts ∈ [phase_start, phase_end]`. Self-bounding (ignores `window`). |
| `anchor?` | A finding identifier ± window | Resolve to `HotspotFinding.evidence[].ts` span → select candidates in `[min − window, max + window]` |
| `r#match?` | Regex over whole `TranscriptCandidate.text` blocks | Filter candidate blocks; per-session loss propagation (§6). Bounded by the per-cycle cap. |
| `window?` | ±N events (or ±T time) around anchor/match hits | Modifies `anchor`/`match`; ignored by self-bounding `phase`. Default in §7.3. |

**Composition:** `phase`/`anchor`/`match` AND-compose (each narrows); `window` modifies
`anchor`/`match`. `transcript:{}` (present, all-None) = the full candidate set under the existing
per-cycle cap (`distill_handler.rs:222`) ≡ `match:".*"` — the degenerate full dump, non-destructive,
already bounded. There is no separate whole-stream mode.

### 7.2 Clock normalization (server-side; interface requirement, §Goal 5)

The agent expresses its query in **its own units** — a finding/anchor id, a phase id, a regex, a
window in events or time. Unimatrix normalizes **internally** to the stored Plane-B unit; the agent
never needs to know Plane B's clock:

- Plane A `EvidenceRecord.ts` is `u64` epoch-millis; Plane B `TranscriptCandidate.ts` is
  `Option<String>` (JSONL) — **independent clocks for `Primary` sessions**. The handler parses
  candidate `ts` to a canonical epoch **at attach time** and joins over a **WINDOW, never an exact
  match**, absorbing the epoch-millis ↔ JSONL skew server-side. Record which clock each side used.
- `ts:None` candidates cannot be placed on the wall clock → fall back to `byte_offset` proximity
  within the same session so they never escape the join silently.

### 7.3 Window default (architect decision — OQ-2): **±120 s / ±3 candidate blocks**

When `anchor`/`match` is supplied and `window` is omitted, the default is **±120 000 ms (±2 min)** for
ts-bearing candidates, with a **±3-candidate-block** `byte_offset`-proximity fallback for `ts:None`
candidates. Caller-overridable; bounded by the existing per-cycle cap. Rationale in ADR-006 §Decision;
summarized: a candidate is a whole conversational **block/turn**, so the window must exceed one turn
plus the cross-plane skew to reliably land the block containing an anchor event; over-inclusion is the
safe error direction (loss propagation makes every extra block individually inspectable under the cap,
whereas under-inclusion produces the silent false-negative the redesign exists to prevent); precision
is not load-bearing (OQ-1 live `ts:None` fraction / hit-rate is unmeasurable read-only, folds into a
delivery-time experiment, and the correctness contract holds at any magnitude).

---

## 8. Consumer Reconciliation — the Atomic Unit (D-6, C-6)

**Server change + all consumers + both protocols + amended ADRs ship as ONE indivisible unit (C-6).**
Shipping the server change without the consumers leaves `/uni-retro` calling the old boolean and
starves the harvest (#5219) — the dominant scope risk.

The atomic unit:

1. **Server (D-1..D-4):** scoped `transcript{}` retrieval + non-destructive default; remove the four
   review-site purge calls; loss propagation; clock normalization; drop the `"summary"` alias.
2. **`uni-retro/SKILL.md`:** the candidate-bearing call uses the `transcript{}` block (not the old
   boolean). The retro AGENT owns the synthesis (Ownership Boundary, §10). Because retrieval is
   repeatable and non-destructive, the retro may retrieve as often as it needs, in any scope, with no
   one-shot to sequence around.
3. **`context_cycle_review` tool description:** document the three axes — `format` render-only;
   `force` durable recompute (never retrieves, never purges); `transcript{}` read-only scoped
   retrieval returning candidates + `SessionLossInfo`, purging nothing. **State plainly the tool has
   no purge verb.**
4. **`uni-delivery-protocol.md` AND `uni-bugfix-protocol.md` (D-7, §11):** cycle-close-after-merge
   then `/uni-retro`, ordering **merge → close → retro**.

`uni-agent-routing.md` is **excluded** — a passing descriptive mention in an overview doc no live
protocol loads; it sets no flag and depends on no candidate-carrying behavior.

---

## 9. Purge Removal + Fold-Read-Stays (see ADR-001, ADR-004)

- **Purge removed.** The four review-site `purge_cycle_transcripts` calls (`tools.rs:2379, 2558,
  3328, 3451`) are deleted. `context_cycle_review` gains **zero destructive capability** — no verb, no
  flag, no default (NG-6).
- **Orphaned functions → delete.** Verified: `purge_cycle_transcripts` (server.rs:661) and its
  helpers `clear_transcripts_for_feature` (session.rs) / `purge_held_for_feature`
  (transcript_hold.rs:331) have **no non-test callers** once the four review-site calls are removed.
  Delivery MUST delete them (CLAUDE.md rule 2, anti-stub / clippy dead-code) — leaving them dead is a
  violation. If operator-triggered immediate reclamation is ever needed, that is a **separate admin/ops
  verb**, authored then, never a parameter on the review tool.
- **Backstops carry the full load (unchanged).** Reclamation is delegated **entirely** to the
  independent `transcript_hold` paths: `sweep_expired(ttl)` (24h TTL, transcript_hold.rs:308),
  cap-eviction (64-session), and per-turn session-close purge (`transcript_hold.rs:9` — "purge only at
  cycle review (post-distill), stale sweep, or cap-eviction"; the cycle-review trigger is what we
  remove, leaving the other two). These already bound memory when no review runs, so leaning on them
  adds no new memory risk. They still emit the content-free terminal audit (`transcript_session_purged`,
  `trigger=stale_sweep` / cap-eviction, bytes-only) — SR-02 audit trail preserved.
- **Exhaustive-match (C-5) moves with the purge.** The exhaustive `TranscriptRetention` match today
  lives inside `purge_cycle_transcripts` (server.rs:~543/551). When that function is deleted, delivery
  MUST confirm the surviving backstop reclaim paths honor retention exhaustively (`RetainDays` stays a
  no-op, no `_` arm) — the C-5 obligation relocates, it does not disappear.
- **Fold read STAYS.** The content-opaque fold (crt-054/055, #5030/#5042) is read at the seam via
  `activity_snapshots_for_feature`, read-before-any-reclamation, and remains gated at all four success
  returns per #4750 (ADR-004). Because the buffer now survives the review, the fold strictly benefits
  (nothing lost sooner) and a subsequent scoped retrieval can hit the same buffer.

**Residency posture (human-ratified, §Accepted Residency Trade-off in SCOPE):** removing the eager
review-purge lengthens raw-transcript residency in memory from *gone-at-review* to *≤24h (TTL) / until
64-cap eviction / until session-close*. Still **memory-only**, still **bounded**, **NG-1 intact**
(never touches disk). A deliberate, human-ratified lengthening of the raw-content window that also
pays for the Q4 "hold more in memory for later inference" direction at no additional cost.

---

## 10. Ownership Boundary — Retro Synthesis Is Agent-Owned, Not Unimatrix's (NG-5)

The `transcript` retrieval is a targeted tool the retro AGENT optionally uses to capture *what
transpired*. Unimatrix serves honest planes; the agent owns the synthesis. **Unimatrix does NOT, and
crt-057 does not build:**

- Synthesizing or joining GH `## Knowledge Stewardship` comment blocks.
- Manufacturing applied-entry attribution (which served entry an agent "applied").
- The rework-count ↔ cause join.
- Surfacing a human-intervention ledger.

The tool returns the non-destructive Plane-A summary + the honest scoped Plane-B slice (candidates +
`SessionLossInfo`) **only** — never a causal claim, never a cross-plane join. All three-source
serving / attribution / human-ledger richness is OUT OF SCOPE (agent-owned retro, outside this
feature).

---

## 11. Retro Lifecycle — Simpler Under the Redesign (see ADR-005)

Both protocols today **close the cycle before the human merges**, so merge/rework activity is never
attributed to the cycle. The restructure (both `uni-delivery-protocol.md` and
`uni-bugfix-protocol.md`):

- Keep a distinct **pr-review / bug-review phase** open **through the human merge decision**.
- **Close only after merge:** `context_cycle(phase-end)` then `context_cycle(stop)`.
- **Then `/uni-retro` post-merge.** Strict ordering: **merge → close cycle → retro**. Human merge gate
  unchanged.

**This is now trivially safe.** Under the prior boolean, close→retro was a hazard: the retro
extraction was the sole purge trigger, so a close-side purge would have starved a retro-after-close.
That entire concern **dissolves** because BOTH review and close are now fully non-destructive:

- `context_cycle stop` was already verified non-purging (its handler drains only the
  `pending_entries_analysis` queue + writes an audit row — no buffer / hold / registration / sweep).
- The review is now non-destructive too, so there is no reaper anywhere in the review→close→retro
  path. Candidates survive to a post-close retro, which may retrieve **repeatedly and
  non-destructively** in any scope.

The only residual exposure is the same TTL/cap aging every buffer already has (a dev-phase buffer may
age out before a late post-merge retro) — independent of review ordering, surfaced via loss
propagation (§6), and softened relative to the prior contract (no earlier purge can lose it; only
aging can). The report body is buffer-content-independent (§5), so only verbatim candidates degrade,
never the summary.

---

## 12. Integration Surface

Exact interfaces so downstream (pseudocode/spec/dev) agents do not invent names. `[NEW]` = introduced
by crt-057; `[UNCHANGED]` = keep byte-identical; `[REMOVE]` = deleted by crt-057.

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Scoped-retrieval param | `#[serde(default)] pub transcript: Option<TranscriptScope>` `[NEW]` — omit = summary only | `RetrospectiveParams`, `mcp/tools.rs:~431` |
| Scope block | `struct TranscriptScope { phase: Option<String>, anchor: Option<String>, r#match: Option<String>, window: Option<Window> }` `[NEW]` — all optional, AND-composed. `match` is a Rust keyword → field is `r#match` or `#[serde(rename = "match")]` (pseudocode detail) | new type (`unimatrix-observe` or `mcp`) |
| Window type + default | `Window` expressing ±N events or ±T millis; **default ±120_000 ms / ±3 blocks** when omitted (§7.3) `[NEW]` — exact enum/struct shape a pseudocode detail | new type |
| Removed param | `include_transcript_candidates` `[REMOVE]` — deleted from `RetrospectiveParams` | `mcp/tools.rs` |
| Render param | `pub format: Option<String>` — accepts `"markdown"`\|`"json"`; `"summary"` **dropped** `[NEW behavior]` | `RetrospectiveParams`, `mcp/tools.rs:~445` |
| Recompute param | `pub force: Option<bool>` `[UNCHANGED]` (None ≡ false) | `RetrospectiveParams`, `mcp/tools.rs:~448` |
| Candidate helper | `distill_before_purge(registry: &SessionRegistry, feature_cycle: &str, observations: &[ObservationRecord], cfg: &RetentionConfig, scope: Option<&TranscriptScope>, reviewer_session_id: Option<&str>) -> Option<TranscriptCandidatesSection>` `[NEW signature]` — returns `None` when `scope` is `None`; applies filters + clock-normalization; name vestigial (no purge follows) | `mcp/distill_handler.rs:48` |
| Attach helper | `attach_to_response_assembly(result: &mut Result<CallToolResult, ErrorData>, section: Option<TranscriptCandidatesSection>)` `[UNCHANGED]` — no-ops on `None`/`Err` | `mcp/distill_handler.rs:281` |
| Read-only snapshot | `TranscriptBuffer::snapshot(&self) -> TranscriptSnapshot` `[UNCHANGED]` — the SOLE content reader reused; no new reader (crt-052 ADR-002 #4848) | `infra/session_transcript.rs:296` |
| Candidate type | `struct TranscriptCandidate { session_id: String, byte_offset: u64, ts: Option<String>, family_hints: Vec<FamilyHint>, text: String }` `[UNCHANGED]` — `match` runs over whole `text`; `ts` parsed to canonical epoch, `byte_offset` is the `ts:None` fallback key | `observe/src/types.rs:611-624` |
| Per-session loss | `struct SessionLossInfo { session_id: String, elided_bytes: u64, has_holes: bool, provenance: CandidateProvenance, dropped_candidates: u64 }` `[UNCHANGED]` — the discriminator for `search_complete` (§6) | `observe/src/types.rs:633-646` |
| Provenance | `enum CandidateProvenance { Primary, Reconstructed }` `[UNCHANGED]` | `observe/src/types.rs:~596-603` |
| Candidates section | `struct TranscriptCandidatesSection { candidates: Vec<TranscriptCandidate>, loss: Vec<SessionLossInfo> }` `[UNCHANGED]` — `search_complete` derived per-session at render, or add a response-transient field OUTSIDE `RetrospectiveReport` (pseudocode detail; AC-14) | `observe/src/types.rs:657-663` |
| Anchor source | `struct EvidenceRecord { description: String, ts: u64 /* epoch millis, Plane A */, tool: Option<String>, detail: String }`; `HotspotFinding.evidence: Vec<EvidenceRecord>` `[UNCHANGED]` — anchor span = `[min(ts), max(ts)]` | `observe/src/types.rs:35-63` |
| Phase source | `cycle_events` / `CycleEventRecord` (`event_type == "cycle_phase_end"`) `[UNCHANGED]` — phase bounds `[phase_start, phase_end]` | `observe/src/types.rs:~344-350` |
| Per-cycle cap | existing chronological keep-earliest cap `[UNCHANGED]` — bounds `transcript:{}` / `match:".*"` full dump | `mcp/distill_handler.rs:222` |
| Purge (removed at review) | `purge_cycle_transcripts(&self, feature_cycle: &str)` `[REMOVE]` — four review-site calls deleted; function + `clear_transcripts_for_feature` / `purge_held_for_feature` orphaned → delete (§9) | `server.rs:661`; calls at `tools.rs:2379, 2558, 3328, 3451` |
| Backstop reclaim (sole path) | `sweep_expired(ttl: Duration) -> Vec<TranscriptPurgeRecord>` + cap-eviction + session-close `[UNCHANGED]` | `infra/transcript_hold.rs:308` etc. |
| Content-opaque fold (STAYS) | `activity_snapshots_for_feature` fold read at the seam, gated ×4 `[UNCHANGED]` — SOLE surviving success side-effect (ADR-004) | `session.rs:566`, `mcp/activity_fold_handler.rs` |
| Source-assertion tests | count `distill_before_purge(` / `attach_to_response_assembly(` (×4) — **`self.purge_cycle_transcripts(&feature_cycle)` ×4 assertion REMOVED** (purge gone; deliberate test update with rationale, C-1/AC-12) | `distill_handler.rs:651-726` |
| Render dispatch loci ("summary" arm removed) | `tools.rs:2532`, `:3359`, `:4268`, `:4324` | `mcp/tools.rs` |

Error boundary: unknown `format` (now including `"summary"`) → `ERROR_INVALID_PARAMS`
(`"Unknown format '…'. Valid values: \"markdown\", \"json\"."`). Scoped retrieval against a gone
buffer (past 24h TTL / cap-evicted / session-closed) → empty or `Reconstructed`-only candidates with
`search_complete == false`, never a crash (AC-06). An invalid `match` regex → `ERROR_INVALID_PARAMS`.

---

## 13. Open Questions for Downstream Agents / Human

- **OQ-1 (deferred, not blocking):** live regex hit-rate and `ts:None` candidate fraction are
  unmeasurable read-only (Plane B never persisted). Fold into a delivery-time instrumentation
  experiment. The correctness contract (loss propagation, indeterminate no-match, `byte_offset`
  fallback, window default) holds regardless of the rate.
- **Anchor / phase id surface:** the exact caller-facing identifier for `anchor` (finding id — the
  report labels findings e.g. `F-03`; confirm the field the pseudocode agent binds to) and `phase`
  (phase name / id string) — resolve against how the report exposes findings/phases. Resolution path
  is fixed (§7.1); the id *representation* is a pseudocode/spec detail.
- **`Window` type shape and `r#match` serde-rename:** exact enum/struct for `Window` (±events vs
  ±millis) and the `match` keyword handling — pseudocode detail; the default (§7.3) is fixed.
- **`distill_before_purge` rename:** keep the vestigial name (preserves counted strings, minimal
  churn) or rename with a deliberate source-assertion-test update — delivery decision, flag to the
  synthesizer.
- **SCOPE AC drift already reconciled in the re-scope:** the re-scoped SCOPE's AC-01..AC-17 already
  reflect the non-destructive contract; no stale boolean-era AC remains for the architect to surface.
```
