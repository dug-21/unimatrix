# Agent Report — nan-018 Wave-1 Cost Metric

**Agent**: nan-018-agent-3-cost-metric
**Component**: cost-metric (`eval/runner/cost.rs`)
**Commit**: `8f13cb03` on `feature/nan-018`

## Files Modified
- `crates/unimatrix-server/src/eval/runner/cost.rs` (new, 363 lines)
- `crates/unimatrix-server/src/eval/runner/mod.rs` (+1 line: `pub mod cost;`)
- `crates/unimatrix-server/Cargo.toml` (+`tokenizers = "0.21"` dep, mirrors embed)
- `Cargo.lock` (tokenizers added to unimatrix-server dep list only)

## What Was Built (per ADR-003 #4896)
- `pub fn token_proxy(entry: &ScoredEntry) -> f64` — two-tier, deterministic.
  - Faithful tier: real subword count via the `tokenizers` crate, loaded once into a
    process-global `OnceLock` from the resolved embed model cache dir (only if
    `tokenizer.json` already present — never triggers a download).
  - Documented fallback: whitespace+punctuation word count × 1.3 when the tokenizer
    cannot load; logged. char/4 rejected.
  - Tier is logged once per process at load time (NFR-08).
- `pub fn profile_cost_tokens(entries: &[ScoredEntry]) -> f64` = Σ token_proxy; empty ⇒ 0.0.

## Tests — 11/11 pass
Differential-token (same k, different load ⇒ different cost, R-07), monotonicity,
sum-equals-proxy, determinism ×2 (R-08), empty set = 0.0, empty payload = 0.0,
fallback word×1.3 deterministic + ignores empty splits + monotonic, unicode no-panic.
Verified via `cargo test -p unimatrix-server --lib eval::` → 249 passed, 0 failed.

## Scope Boundary Honored
Did NOT touch output.rs, metrics.rs, replay.rs. `ProfileResult.cost_tokens` field and
the run-loop call site are left for Wave-4 report-extensions. Functions stand alone.

## Issues / Blockers
1. **Flagged spec discrepancy (not a blocker).** ADR-003/pseudocode define the agent-read
   payload as `title + content`, but the current `ScoredEntry` (nan-007) carries only
   `title` — no `content` field. Adding one touches output.rs/replay.rs, which are out of
   this component's scope. I assembled the payload in a single `payload_text()` helper over
   the available `title`; appending `content` is a documented one-line change once Wave-4
   adds the field. The metric is correct and token-weighted over today's payload.
   **Wave-4 must add `content` to `ScoredEntry` and update `payload_text`.**
2. **Padding/truncation gotcha (resolved).** A freshly loaded embed `tokenizer.json` bakes
   in padding+truncation, so `encode().len()` returned a constant 128 for every input. Fixed
   by clearing both (`with_padding(None)` / `with_truncation(None)`) after load. Stored as
   pattern #4905.
3. **Full-workspace `cargo test` OOM (environmental, not my change).** `cargo test --workspace`
   was SIGKILLed (signal 9) while compiling the large `unimatrix-server` test binary — a
   container memory-pressure issue, not a test failure. The crate builds clean and all eval
   tests pass when build jobs are capped (`CARGO_BUILD_JOBS=2`).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) and the briefing flow —
  ADR-003 #4896 (token-weighted, subword tier + word×1.3 fallback, char/4 rejected),
  ADR-001 #4897, ADR-006 #4894. Patterns surfaced were generic service-layer; none on
  tokenizer counting.
- Stored: entry #4905 "Clear padding/truncation before using an embed tokenizer.json for
  token COUNTING" via /uni-store-pattern (topic unimatrix-server) — a runtime trap invisible
  in source (compiles, runs, silently constant counts).
