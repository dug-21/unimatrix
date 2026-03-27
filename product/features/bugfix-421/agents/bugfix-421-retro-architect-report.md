# bugfix-421 Retro — Architect Report

Agent: bugfix-421-retro-architect (uni-architect)
Date: 2026-03-27

---

## 1. Patterns

### Updated: #3655 -> #3675

**Title:** Tick passes over full entry set: bound, shuffle, and embedding-filter source candidates independently of NLI pair cap

Pattern #3655 described two structural bounds (source-candidate cap before get_embedding; contradiction threshold floor). The bugfix revealed two additional correctness properties that belong in the same pattern since they apply to any tick-based fan-out:

- **Shuffle requirement** — each candidate tier must be independently shuffled with `rand::rng()` before `take(N)`. Stable ordering (insertion order, `created_at DESC`) causes subsequent ticks to re-select the same N entries, exhaust the existing-pairs pre-filter, and write zero new edges indefinitely with no error signal.
- **Embedded-filter requirement** — no-embedding entries must be excluded from both tiers before selection. They consume source slots and are silently skipped every tick, permanently starving embeddable candidates.

Correction stored as #3675. #3655 deprecated.

### No new patterns stored

#3671 (rand::rng() API change) — already stored during the bugfix cycle. No further update needed; scope is correct.

---

## 2. Procedures

**Compile-cycle reduction ("resolve type errors in-memory before building")** — not stored.

The retrospective flagged 20 compile cycles in the discovery phase. This is generic Rust development practice, not a Unimatrix-specific reusable technique. No existing procedure was found (search returned unrelated entries). The behavior is already implied by standard iterative Rust development workflow and does not justify a procedure entry.

---

## 3. ADR Status

All four crt-029 ADRs reviewed against the bugfix changes.

| ID | ADR | Status | Notes |
|----|-----|--------|-------|
| #3656 | ADR-001: nli_detection_tick.rs module split | Validated | Unaffected. Module boundary, file structure unchanged. |
| #3657 | ADR-002: write_inferred_edges_with_cap as named variant | Validated | Unaffected. The cap function's interface and isolation are confirmed correct. |
| #3658 | ADR-003: Source-candidate bound = max_graph_inference_per_tick | Validated | The bound decision is correct. The bugfix added shuffle and embedded-filter *within* the selection function — both complement the bound rather than replace it. No supersession needed. |
| #3659 | ADR-004: query_existing_supports_pairs() for pre-filter | Validated | Unaffected. Pre-filter mechanism is correct; the bug was in candidate ordering/filtering before pre-filter, not in the pre-filter itself. |

No ADR supersessions required.

---

## 4. Lessons

### Corrected: #3672 -> #3676

**Title:** embedded_ids HashSet and rand::rng() must be built in async context, not inside rayon closure

Entry #3672 had no tags, making it undiscoverable by semantic search on relevant terms. Correction applied:
- Added tags: `async-context`, `background-tick`, `bugfix-421`, `embedded-ids`, `nli-detection`, `rand`, `rayon`
- Corrected title to say `rand::rng()` (consistent with body and pattern #3671; original said `rand::thread_rng()`)
- Content unchanged

Stored as #3676. #3672 deprecated.

### #3668 — No changes needed

Entry is high quality with full tags and accurate content. No correction warranted.

### #3669 — Already deprecated

Pre-curated as duplicate/stale before this review. No action.

---

## 5. Retrospective Findings

### Hotspot: tool_failure_hotspot — context_get failed 36 times

Root cause confirmed in this session: `context_get` (and `context_search`) require integer parameters passed as JSON integers, not quoted strings. The MCP schema specifies `format: int64` for `id` and `k`. Any tool invocation framework that serializes these as strings causes systematic failure. This was reproduced at the start of this retro session.

**Action taken:** None beyond observation — this is an agent invocation pattern issue, not a Unimatrix server issue. The fix is on the calling side: pass integer literals without quotes in JSON parameter blocks.

### Hotspot: compile_cycles (20 cycles) — no action

See Procedures section above. Not worth storing as a procedure.

### Positive baselines — no action needed

Zero permission friction, zero coordinator respawn, zero post-completion work, below-threshold grep-via-bash usage. These confirm the bugfix session was well-executed. No lessons to extract from clean signals.

---

## Summary of Unimatrix Changes

| Entry | Action | Result |
|-------|--------|--------|
| #3655 | Corrected (pattern, added shuffle + embedded-filter requirements) | -> #3675 |
| #3672 | Corrected (lesson, added missing tags + title fix) | -> #3676 |
| crt-029 ADRs #3656–#3659 | Validated, no changes | — |
| Compile-cycle procedure | Not stored (too generic) | — |
