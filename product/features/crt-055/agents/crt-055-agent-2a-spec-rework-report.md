# Agent Report — crt-055-agent-2a-spec-rework

> Gate 3a REWORK (iteration 1)
> Defect: AC-22 worked example self-contradictory with its own binding gate semantics
> Result: RESOLVED — worked example restated; gate semantics UNCHANGED

## Defect (as reported)

AC-22's worked example asserted a read +500ms after `compacted_at = T` COUNTS and expected `compaction_reread_count == 2`. Under the binding gate (normalize read `ts` millis→seconds via integer floor `ts_millis ÷ 1000`, count iff `read_ts_secs > compacted_at`, STRICT), a +500ms read floors back to second `T`, and `T > T` is false → it does NOT count. The asserted `== 2` is arithmetically impossible under the correct floor + strict-`>` gate.

## Resolution

Binding gate is correct and was left UNCHANGED (floor `÷1000` + strict `>`; earliest `compacted_at` per session per ADR-006 / #5048). Only the worked example's numbers were wrong and were restated so the positive (counting) case actually clears the gate:

- `compacted_at = T` (Unix seconds).
- Read at **+1s** (`ts_millis = (T+1)·1000`) → floor `T+1` → `T+1 > T` true → **COUNTS**.
- Read at **−500ms** (`ts_millis = (T−1)·1000 + 500`) → floor `T−1` → `T−1 > T` false → does **NOT** count. This is the floor-catching guard: an unnormalized gate comparing raw millis `ts` (`(T−1)·1000 ≫ T`) against seconds `compacted_at = T` would wrongly count it; correct seconds-normalization rejects it (catches the ~1000× mis-compare).
- Read **exactly at the boundary** (`ts_millis = T·1000`) → floor `T` → `T > T` false → **NOT** counted (strict `>`).
- Expected `compaction_reread_count` = **1** (was the impossible `2`).

The seconds-normalization intent (catch a ~1000× millis-vs-seconds mis-compare) is preserved entirely by the −500ms case.

## Files edited

- `product/features/crt-055/specification/SPECIFICATION.md` — AC-22 row (Criterion column worked example + Verification Method column).
- `product/features/crt-055/ACCEPTANCE-MAP.md` — AC-22 row (Description + Verification Detail columns).

Both documents now state the SAME self-consistent worked example.

## Confirmation — no other AC changed

Only the AC-22 rows were edited in each file. No gate semantics changed (floor + strict `>` intact). No other AC (AC-01..AC-21, AC-19) touched. No FR/NFR/Constraint/Domain text changed. No git operations run (Delivery Leader owns git).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-006 (#5048, gate on earliest `compacted_at` per session, strict `>`) and ADR-005 (#5047, dual reload two gates), confirming the binding gate semantics are correct and must remain unchanged; applied them to restate only the worked-example numbers. Read-only tier — no storage (rework is feature-specific; retro may promote any interpretation that generalizes).
