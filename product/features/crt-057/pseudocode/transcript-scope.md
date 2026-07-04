# Component: `TranscriptScope` [NEW]

File: `unimatrix-observe/src/types.rs` (next to `TranscriptCandidatesSection`, `:657`)

## Purpose

The all-optional, AND-composed filter block for the read-only `transcript` axis (ADR-002/§7, ADR-006).
Deserialized from tool params (retrospective-params.md); consumed by `retrieve_scoped_candidates`
(distill-before-purge.md).

## Type + serde

```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct TranscriptScope {
    phase:  Option<String>,   // phase id → cycle_events bounds; self-bounding (ignores window)
    anchor: Option<String>,   // finding id → HotspotFinding.evidence[].ts span [min,max]
    #[serde(rename = "match")]
    r#match: Option<String>,  // regex over whole TranscriptCandidate.text; `match` is a Rust keyword
    window: Option<Window>,   // see window.md
}
```

`r#match` handling: use the raw identifier `r#match` with `#[serde(rename = "match")]` so the wire key
is `match` while Rust sees a legal field. (ADR-006 §Consequences fixes this as the resolution.)

## Composition semantics (FR-10, ADR-006, R-09)

AND-composition — each present filter NARROWS (intersection, never union). Evaluated by
`retrieve_scoped_candidates` as a chain of retained-candidate predicates:

```
fn scope_predicate(scope, candidate, ctx) -> bool:      # ctx carries resolved bounds + clock helper
    if scope.phase  present and NOT phase_contains(ctx.phase_bounds, candidate):  return false
    if scope.anchor present and NOT window_contains(ctx.anchor_bounds, scope.window, candidate): return false
    if scope.r#match present and NOT regex_is_match(ctx.compiled_regex, candidate.text): return false
    return true
```

- **phase**: `candidate.ts ∈ [phase_start, phase_end]`, bounds from `cycle_events`
  (`CycleEventRecord`, `event_type == "cycle_phase_end"`). Self-bounding → `window` IGNORED even if
  supplied. `ts:None` candidate resolved via byte_offset proximity fallback within the phase's
  candidate-block span (window.md).
- **anchor**: resolve id → `HotspotFinding.evidence[].ts` span `[min,max]`; select
  `[min − window, max + window]` (window default when omitted — window.md).
- **match**: regex over the WHOLE `TranscriptCandidate.text` block (truncation bites at selection, not
  the regex — a block is present whole or absent, ADR-003).
- **empty scope** `TranscriptScope{None,None,None,None}` ≡ `match:".*"` = full candidate set under the
  existing per-cycle cap (`distill_handler.rs` keep-earliest, FR-8/AC-05). No separate whole-stream mode.

## Anchor / phase id resolution (OQ-2 — pseudocode detail resolved here)

- `anchor` binds to the finding label the report emits for `HotspotFinding` (e.g. `F-03`). Resolution:
  look up the finding by that label in the report's `hotspots` set, take its `evidence: Vec<EvidenceRecord>`,
  compute `[min(ts), max(ts)]` over `EvidenceRecord.ts` (u64 epoch-millis, Plane A). If the id resolves to
  no finding → treated as an empty span → section absent (not an error) UNLESS you prefer a hard error;
  **FLAG (open question)**: unknown `anchor`/`phase` id → empty-section vs `ERROR_INVALID_PARAMS`. Default
  chosen here: empty section (absent), consistent with FR-7 "scope yields nothing → section absent",
  reserving `ERROR_INVALID_PARAMS` for malformed input (bad regex), not merely non-matching ids.
- `phase` binds to the phase name/id string as stored in `cycle_events`.

## Error handling

- Invalid `match` regex → compile once, up front; on failure return `ERROR_INVALID_PARAMS`
  (`Invalid 'match' regex: <err>`). Do NOT panic. ReDoS surface: the regex runs over potentially large
  candidate blocks — **FLAG to delivery** (CON / security): bound with `regex::RegexBuilder`
  `.size_limit(...)` / `.dfa_size_limit(...)` or a match-time guard; the `regex` crate has no catastrophic
  backtracking but a pathological pattern can still be memory-heavy.
- Malformed `window` → serde error at param parse (retrospective-params.md).

## Key test scenarios

- Serde: `{"match":"x"}` populates `r#match`; `{"phase":"design"}` populates `phase`.
- `phase ∧ match` returns a strict subset of either alone (intersection, R-09 sc.3).
- Self-bounding `phase` with a supplied `window` → `window` ignored (R-09 sc.4).
- Empty scope ≡ `match:".*"` → same full candidate set under cap (AC-05).
- Invalid regex `"("` → `ERROR_INVALID_PARAMS`, no panic (R-09 sc.6).
- Unknown `anchor` id → section absent (per chosen default), no crash.
