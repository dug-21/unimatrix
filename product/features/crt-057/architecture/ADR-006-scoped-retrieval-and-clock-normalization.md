## ADR-006: Scoped Transcript Retrieval Over the Existing Pipeline (No New Reader) + Cross-Plane Clock Normalization + a Conservative ±120 s / ±3-Block Window Default

Feature: crt-057 · GH #894 · New ADR, 2026-07-04 (design source: ass-091 Q3, FINDINGS-Q3.md)
Owns the retrieval *mechanism*; ADR-002 owns the API surface; ADR-003 owns loss propagation.

### Context

The `transcript` axis (ADR-002) is a scoped, read-only retrieval. Its mechanism must satisfy three
hard constraints simultaneously, established by ass-091 Q3:

1. **No new buffer reader** — the single-content-reader invariant (crt-052 ADR-002, #4848) forbids a
   second reader of buffer content; the existing `snapshot()` (`session_transcript.rs:296`, already
   `&self`) is the one reader, and the scoped filters must layer on the existing candidate pipeline.
2. **Cross-plane clock skew** — Plane A `EvidenceRecord.ts` is `u64` epoch-millis; Plane B
   `TranscriptCandidate.ts` is `Option<String>` (JSONL). These are **independent clocks for `Primary`
   sessions** (they coincide only for `Reconstructed` sessions, derived from the same observations).
   Any timestamp join across the planes inherits the skew.
3. **`ts:None` candidates** — a candidate whose JSONL block lacked a timestamp cannot be placed on the
   wall clock and would silently escape any timestamp-scoped selection.

The agent must express its query in its own units (finding/anchor id, phase id, regex, a window in
events or time) and never be required to know Plane B's storage clock (Goal 5). A window magnitude for
`anchor`/`match` needs an architect default (OQ-2).

### Decision

**Retrieval is a read-only `snapshot()` narrowing the existing candidate pipeline.** The `transcript`
block's `phase`/`anchor`/`match`/`window` filters run over the same `TranscriptCandidatesSection` that
`distill_before_purge` already produces, narrowed **before** `attach_to_response_assembly`. It reuses
the existing `snapshot()`; **no new buffer reader is introduced** (#4848 preserved). The block is a
filter layer, not a new content path.

- `phase` → phase bounds from `cycle_events` (`CycleEventRecord`, `event_type == "cycle_phase_end"`);
  select `candidate.ts ∈ [phase_start, phase_end]`. Self-bounding (ignores `window`).
- `anchor` → resolve to the finding's `HotspotFinding.evidence[].ts` span `[min, max]`; select
  candidates in `[min − window, max + window]`.
- `match` → regex over whole `TranscriptCandidate.text` blocks (survives truncation — truncation bites
  at selection, not the regex). Per-session loss propagation is ADR-003.
- `window` → modifies `anchor`/`match`; ignored by self-bounding `phase`.
- `phase`/`anchor`/`match` AND-compose. `transcript:{}` ≡ `match:".*"` under the existing per-cycle cap
  (`distill_handler.rs:222`) = the degenerate full dump.

**Clock normalization is server-side and internal.** The handler parses each candidate `ts` to a
**canonical epoch at attach time** and joins over a **WINDOW, never an exact match**, absorbing the
epoch-millis ↔ JSONL skew for `Primary` sessions. It records which clock each side used. `ts:None`
candidates fall back to `byte_offset` proximity within the same session so they never escape the join
silently. The agent supplies query units it knows; it never sees or supplies Plane B's clock. This
also serves any future timestamp join (ass-090's distill-into-summary work inherits the same
normalization).

**Window default (OQ-2): ±120 000 ms (±2 min) for ts-bearing candidates; ±3 candidate blocks by
`byte_offset` proximity for `ts:None`.** Applied when `anchor`/`match` is supplied and `window` is
omitted. Caller-overridable; bounded by the existing per-cycle cap. Rationale:

1. **Candidate granularity is a whole conversational block/turn** (`TranscriptCandidate.text` is "the
   whole matched user/assistant block, unwindowed"), not a raw byte. The window must exceed one turn
   plus the cross-plane skew so an anchor evidence event (Plane A, fires mid-turn at a tool call)
   reliably lands the Plane-B block that *contains* it, whose block `ts` may be stamped at turn start
   or end. ±2 min comfortably exceeds a single generously-long agent turn plus skew.
2. **The window's primary job is to absorb the Plane A ↔ Plane B skew** (Q3 Caveat 1 — "the window
   absorbs the skew"); ±2 min exceeds any plausible within-turn skew between the two independent
   clocks.
3. **Over-inclusion is the safe error direction.** Loss propagation (ADR-003) makes every returned
   candidate individually inspectable and the retro agent owns synthesis, so a slightly wider slice
   costs a few extra blocks under the existing cap — never a silent miss. Under-inclusion produces
   exactly the silent false-negative the redesign exists to prevent. A conservative default biases
   wide.
4. **`ts:None` fallback ±3 blocks** mirrors the "adjacent turns" spirit of the time window and keeps
   timestamp-less candidates from silently dropping out (AC-07).
5. **Precision is not load-bearing.** OQ-1 (live `ts:None` fraction, regex hit-rate) is unmeasurable
   read-only and folds into a delivery-time experiment; the correctness contract holds at ANY window
   magnitude, so ±120 s / ±3 blocks is a conservative-safe starting point, caller-overridable and
   delivery-tunable — not a precision-critical constant.

### Consequences

Easier: scoped retrieval reuses the existing pipeline and the one existing reader (no #4848
violation); the clock skew and `ts:None` traps are handled once, server-side, so callers query in
natural units; the window default errs toward completeness, aligning with the loss-propagation honesty
guarantee; the same normalization serves ass-090's later timestamp joins.

Harder: the ±120 s / ±3-block default is a judgment grounded in turn granularity and skew envelope,
not an empirically measured optimum (OQ-1) — a delivery-time experiment may retune it; `anchor` needs
a caller-facing finding identifier and `phase` a phase identifier (exact id representation resolved
against how the report labels findings/phases — a pseudocode/spec detail); `match` is a Rust keyword,
so the field is `r#match` / `#[serde(rename = "match")]`.

Cross-refs: ADR-002 (API surface), ADR-003 (loss propagation), #4848 (single content reader —
Prerequisite in spirit; reuse `snapshot()`), #4750 (four-site seam), FINDINGS-Q3.md (mechanism
citations), FINDINGS-Q1.md (data-plane bounds).
