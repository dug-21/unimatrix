# Component: Cost metric — `cost-metric.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/runner/cost.rs` (new) + `metrics.rs` (call site).
**ADR**: ADR-003 (#4896). **Risks**: R-07 (infidelity), R-08 (non-determinism).

## Purpose

Compute a per-profile **token-weighted** cost `cost_tokens = Σ token_proxy(result)` over the
returned set. Token-weighted (NOT result-count): the same `k` carries different cost when token
loads differ. `k` is a SECONDARY axis (derivable from `entries.len()`); `cost_tokens` is primary.

## `token_proxy` — two-tier, deterministic

Counts tokens over the **payload an agent reads** for a result: the entry's `title` + `content`
(the snippet text), NOT score metadata.

```
pub fn token_proxy(entry: &ScoredEntry) -> f64 {
    let text = format!("{} {}", entry.title, entry.content);   // payload the agent reads

    match TOKENIZER.get() {                                     // tokenizers crate (embed dep tree)
        Some(tok) => {
            // FAITHFUL TIER (default): real deterministic subword count, same tokenizer
            // family the embedding model uses (all-MiniLM). Deterministic given the text.
            tok.encode(&text, /*add_special_tokens=*/ false).len() as f64
        }
        None => {
            // DOCUMENTED FALLBACK: whitespace+punctuation word count * 1.3.
            // Known error (Band-2): under-counts subword-split rare tokens, over-counts on
            // punctuation-heavy text; empirically within ~±20% of the subword count on KB prose.
            let words = text.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
                            .filter(|w| !w.is_empty())
                            .count();
            (words as f64) * 1.3
        }
    }
    // char/4 is REJECTED (ADR-003) — it ignores vocabulary and mis-ranks sets.
}
```

- The tier that produced the number is LOGGED (faithful vs fallback) so downstream reads cost
  with the right confidence (NFR-08).
- Tokenizer is loaded once (e.g. `OnceCell`) and reused; the proxy is deterministic given corpus
  text — a property the drift guard and cross-run comparison rely on (R-08).

## Per-profile cost sum

```
pub fn profile_cost_tokens(entries: &[ScoredEntry]) -> f64 {
    entries.iter().map(token_proxy).sum()
    // empty result set ⇒ 0.0
}
```

## Call site (in `eval/runner/metrics.rs` / `run_single_profile`)

```
// same pass as P@5/MRR and trust (C-03, AC-14):
profile_result.cost_tokens = profile_cost_tokens(&entries);
// k is already implied by entries.len(); report surfaces both (cost primary, k secondary).
```

## Data flow

- **Input**: `&[ScoredEntry]` (the returned set, carrying `title` + `content`).
- **Output**: `f64` on `ProfileResult.cost_tokens`; surfaced in `report` next to P@K/MRR/latency,
  with a cost-delta column vs baseline.
- **Transformation**: text → token count (faithful) or word×1.3 (fallback) → sum.

## Regression surfacing (advisory, ε = 0.0 — LOCKED §7.1)

`find_regressions` (see `report-extensions.md`) lists any candidate whose `cost_tokens` exceeds
baseline by more than **ε = 0.0** (any growth reported) but BLOCKS NOTHING — `eval report` exit
code unchanged (R-17). ε is report-only in Wave-1; a non-zero ε is premature tuning (ASS-037
authority; no downstream cost distribution exists yet).

## Error handling

- Missing `title`/`content` ⇒ empty string contributes 0 tokens (no panic).
- Tokenizer load failure ⇒ silent, defined fallback to word×1.3 (logged), never an abort.

## Key test scenarios

- **Differential-token (R-07.1, AC-09)**: two sets with the SAME k but different per-result token
  loads (50-token vs 500-token snippets) ⇒ DIFFERENT cost (proves token-weighted, not k-weighted).
- **Monotonicity (R-07.2)**: a strictly longer-text set costs strictly more.
- **Determinism (R-08)**: `token_proxy(result)` repeated on the same entry ⇒ equal; cost over a
  fixed set identical across runs.
- **Empty set**: cost = 0.0.
- **Fallback tier**: with tokenizer unavailable, word×1.3 is used and the tier is logged; numbers
  stay deterministic.
- **Surfacing**: cost AND k both appear in the report; cost-delta column present.
