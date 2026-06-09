//! Token-weighted cost metric for the eval harness (nan-018, Wave-1, ADR-003 #4896).
//!
//! Cost is the noise an agent pays to *read* a result set, measured in tokens —
//! NOT result count. The same `k` carries different cost when token loads differ
//! (a 50-token snippet vs a 500-token one). `cost_tokens = Σ token_proxy(result)`;
//! `k` is a secondary axis derivable from `entries.len()`.
//!
//! `token_proxy` is two-tier and deterministic (R-08):
//! - **Faithful tier (default):** a real subword token count via the `tokenizers`
//!   crate, using the same tokenizer family the embedding model uses (all-MiniLM).
//!   Loaded once into a process-global `OnceLock` and reused.
//! - **Documented fallback:** whitespace-and-punctuation word count × 1.3, used only
//!   when the tokenizer cannot be loaded in the eval context. Its known error bars
//!   are documented in ADR-003 / the Band-2 cost reference (~±20% on KB prose).
//!
//! `char/4` is REJECTED (ADR-003): it ignores vocabulary and mis-ranks sets.
//!
//! The tier that produced the number is logged once per process (NFR-08) so
//! downstream consumers read cost figures with the right confidence.
//!
//! Run-loop wiring (populating `ProfileResult.cost_tokens`) lives in the report /
//! metrics layer (report-extensions); the functions here stand alone and are unit-tested.
//!
//! **Payload = `title + content` (ADR-003).** The agent-read payload is the entry's
//! `title` plus its snippet `content`. As of nan-018 report-extensions, `ScoredEntry`
//! carries both fields (the cost-metric carry-flag), and [`payload_text`] is the single
//! place that assembles them — so the metric counts the full payload an agent reads.

use std::path::PathBuf;
use std::sync::OnceLock;

use tokenizers::Tokenizer;
use tracing::{debug, warn};

use super::output::ScoredEntry;

/// Multiplier applied to the whitespace/punctuation word count in the fallback
/// tier. Documented in ADR-003: word counts systematically under-count
/// subword-split rare tokens, so a fixed ×1.3 brings the estimate within roughly
/// ±20% of the true subword count on knowledge-base prose.
const FALLBACK_WORD_MULTIPLIER: f64 = 1.3;

/// Process-global tokenizer for the faithful tier.
///
/// `None` means the tokenizer could not be loaded in this eval context; every
/// `token_proxy` call then uses the documented fallback. Loaded once and reused so
/// the proxy is deterministic across results and runs (R-08), and so the tier is
/// logged exactly once per process.
static TOKENIZER: OnceLock<Option<Tokenizer>> = OnceLock::new();

/// Resolve and load the faithful-tier tokenizer, if available.
///
/// The tokenizer is the same `tokenizer.json` the embedding model uses. It is
/// loaded from the resolved model cache directory **only if already present** — we
/// never trigger a download from inside the cost metric. When the file is absent
/// (or fails to parse), the documented fallback tier engages.
fn load_tokenizer() -> Option<Tokenizer> {
    let config = unimatrix_embed::EmbedConfig::default();
    let cache_dir = config.resolve_cache_dir();
    let tokenizer_path: PathBuf = cache_dir
        .join(config.model.cache_subdir())
        .join("tokenizer.json");

    if !tokenizer_path.exists() {
        warn!(
            target: "eval::cost",
            path = %tokenizer_path.display(),
            "cost metric: tokenizer.json not found; using documented word×1.3 fallback tier (ADR-003)"
        );
        return None;
    }

    match Tokenizer::from_file(&tokenizer_path) {
        Ok(mut tok) => {
            // The model's `tokenizer.json` may bake in padding/truncation (the embed
            // pipeline configures both). For a faithful *count* we must measure the
            // true token length, not a padded/truncated fixed width — otherwise every
            // text reports the pad length and cost becomes constant. Clear both.
            tok.with_padding(None);
            if let Err(e) = tok.with_truncation(None) {
                warn!(
                    target: "eval::cost",
                    error = %e,
                    "cost metric: failed to clear tokenizer truncation; counts may be capped"
                );
            }
            debug!(
                target: "eval::cost",
                path = %tokenizer_path.display(),
                "cost metric: loaded faithful subword tokenizer tier (ADR-003)"
            );
            Some(tok)
        }
        Err(e) => {
            warn!(
                target: "eval::cost",
                path = %tokenizer_path.display(),
                error = %e,
                "cost metric: failed to load tokenizer; using documented word×1.3 fallback tier (ADR-003)"
            );
            None
        }
    }
}

/// Faithful subword token count over `text` (real tokenizer, deterministic).
fn faithful_tokens(tok: &Tokenizer, text: &str) -> f64 {
    match tok.encode(text, false) {
        Ok(encoding) => encoding.len() as f64,
        Err(e) => {
            // An encode failure on a single string is non-fatal: fall back for this
            // result rather than aborting the whole eval run.
            warn!(
                target: "eval::cost",
                error = %e,
                "cost metric: tokenizer.encode failed for a result; using word×1.3 fallback for it"
            );
            fallback_tokens(text)
        }
    }
}

/// Documented fallback: whitespace + punctuation word count × 1.3.
///
/// Deterministic and panic-free over any UTF-8 input, including multi-byte text.
/// Empty / whitespace-only text yields `0.0`.
fn fallback_tokens(text: &str) -> f64 {
    let words = text
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| !w.is_empty())
        .count();
    (words as f64) * FALLBACK_WORD_MULTIPLIER
}

/// Assemble the agent-read payload text for a result: the snippet text the search
/// surfaces, NOT the score metadata.
///
/// ADR-003 / the pseudocode define the payload as `title` + `content`. As of nan-018
/// report-extensions (the cost-metric carry-flag), `ScoredEntry` carries both: the
/// payload is the full `title + content` an agent reads. This helper is the single
/// place that assembles it, so the rest of the metric is unchanged.
fn payload_text(entry: &ScoredEntry) -> String {
    format!("{} {}", entry.title, entry.content)
}

/// Token-weighted cost of a single result — the noise an agent pays to read it.
///
/// Counts tokens over the **payload an agent reads** (see [`payload_text`]), NOT the
/// score metadata. Uses the faithful subword tier when the tokenizer is available,
/// otherwise the documented word×1.3 fallback (ADR-003). Deterministic given the
/// text (R-08): the tokenizer is loaded once and reused.
///
/// Missing / empty payload ⇒ 0 tokens (never a panic).
pub fn token_proxy(entry: &ScoredEntry) -> f64 {
    let text = payload_text(entry);

    match TOKENIZER.get_or_init(load_tokenizer) {
        Some(tok) => faithful_tokens(tok, &text),
        None => fallback_tokens(&text),
    }
}

/// Per-profile token-weighted cost: `cost_tokens = Σ token_proxy(result)`.
///
/// An empty result set ⇒ `0.0`. Primary cost axis for a profile/scenario; `k` is
/// the secondary axis, derivable from `entries.len()`.
pub fn profile_cost_tokens(entries: &[ScoredEntry]) -> f64 {
    entries.iter().map(token_proxy).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `ScoredEntry` whose agent-read payload carries `payload`.
    ///
    /// The payload lives in `content` (title left empty) so the assembled
    /// `payload_text` (`"{title} {content}"`) carries exactly the `payload` words.
    /// All non-text fields are inert: `token_proxy` reads only the payload.
    fn entry_with(payload: &str) -> ScoredEntry {
        ScoredEntry {
            id: 1,
            title: String::new(),
            content: payload.to_string(),
            category: String::new(),
            final_score: 0.0,
            similarity: 0.0,
            confidence: 0.0,
            status: String::new(),
            nli_rerank_delta: None,
        }
    }

    /// The cost payload is `title + content` (nan-018 carry-flag): both fields
    /// contribute tokens. A result with content costs strictly more than the
    /// same title alone.
    #[test]
    fn test_payload_includes_title_and_content() {
        let title_only = ScoredEntry {
            id: 1,
            title: "alpha beta gamma".to_string(),
            content: String::new(),
            category: String::new(),
            final_score: 0.0,
            similarity: 0.0,
            confidence: 0.0,
            status: String::new(),
            nli_rerank_delta: None,
        };
        let title_and_content = ScoredEntry {
            content: "delta epsilon zeta eta theta".to_string(),
            ..title_only.clone()
        };
        assert!(
            token_proxy(&title_and_content) > token_proxy(&title_only),
            "title+content payload must cost more than title alone"
        );
    }

    fn short_text() -> &'static str {
        "alpha beta gamma delta epsilon"
    }

    fn long_text() -> String {
        // Same lexical shape, far more tokens than `short_text` — used for the
        // same-k / different-token-load assertions.
        "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod \
         tempor incididunt ut labore et dolore magna aliqua ut enim ad minim \
         veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea \
         commodo consequat duis aute irure dolor in reprehenderit in voluptate"
            .to_string()
    }

    // --- R-07: token-weighting, not k-weighting (AC-09) ---

    /// Load-bearing: same k, different per-result token load ⇒ different cost.
    #[test]
    fn test_cost_same_k_different_token_load_differs() {
        let short_set: Vec<ScoredEntry> = (0..5).map(|_| entry_with(short_text())).collect();
        let long_set: Vec<ScoredEntry> = (0..5).map(|_| entry_with(&long_text())).collect();

        assert_eq!(short_set.len(), long_set.len(), "same k precondition");

        let cost_short = profile_cost_tokens(&short_set);
        let cost_long = profile_cost_tokens(&long_set);

        assert_ne!(
            cost_short, cost_long,
            "same k but different token loads must yield different cost (token-weighted, not k-weighted)"
        );
        assert!(
            cost_long > cost_short,
            "the heavier token load must cost more: long={cost_long} short={cost_short}"
        );
    }

    /// A strictly longer text yields a strictly larger token_proxy.
    #[test]
    fn test_token_proxy_monotonic_on_length() {
        let short = entry_with(short_text());
        let long = entry_with(&long_text());

        let p_short = token_proxy(&short);
        let p_long = token_proxy(&long);

        assert!(
            p_long > p_short,
            "longer text must score strictly more: long={p_long} short={p_short}"
        );
    }

    /// `cost == Σ token_proxy(r)` exactly over the returned set.
    #[test]
    fn test_cost_is_sum_of_token_proxy() {
        let entries = vec![
            entry_with("alpha one two three"),
            entry_with("beta four five"),
            entry_with("gamma six"),
        ];

        let expected: f64 = entries.iter().map(token_proxy).sum();
        let actual = profile_cost_tokens(&entries);

        assert_eq!(
            actual, expected,
            "profile_cost_tokens must equal the sum of per-result token_proxy"
        );
    }

    // --- R-08: determinism (AC-09) ---

    /// token_proxy is identical across repeated calls on the same entry.
    #[test]
    fn test_token_proxy_deterministic() {
        let entry = entry_with("title here some snippet content for the agent to read");
        let first = token_proxy(&entry);
        let second = token_proxy(&entry);
        let third = token_proxy(&entry);
        assert_eq!(first, second, "token_proxy must be deterministic");
        assert_eq!(second, third, "token_proxy must be deterministic");
    }

    /// Summed cost over a fixed set is identical across repeated computation.
    #[test]
    fn test_cost_deterministic_across_runs() {
        let entries: Vec<ScoredEntry> = (0..7)
            .map(|i| entry_with(&format!("t{i} content block number {i} with words")))
            .collect();

        let run_a = profile_cost_tokens(&entries);
        let run_b = profile_cost_tokens(&entries);

        assert_eq!(
            run_a, run_b,
            "summed cost over a fixed set must be identical across runs (no map-order / float drift)"
        );
    }

    // --- empty set ---

    /// Empty result set ⇒ cost == 0.0 exactly.
    #[test]
    fn test_cost_empty_set_is_zero() {
        let empty: Vec<ScoredEntry> = Vec::new();
        assert_eq!(profile_cost_tokens(&empty), 0.0);
    }

    /// Empty payload ⇒ 0 tokens (no words/tokens).
    #[test]
    fn test_token_proxy_empty_payload_is_zero() {
        let entry = entry_with("");
        assert_eq!(
            token_proxy(&entry),
            0.0,
            "empty payload must contribute 0 tokens, not panic"
        );
    }

    // --- fallback tier: deterministic + correct multiplier ---

    /// The documented fallback is whitespace+punctuation word count × 1.3, and is
    /// deterministic. Tested directly so the assertion holds regardless of whether a
    /// faithful tokenizer happens to be present in the test environment.
    #[test]
    fn test_fallback_tier_word_times_1_3_deterministic() {
        // 5 words; punctuation is a separator, not a word.
        let text = "hello, world. foo bar baz!";
        let expected = 5.0 * FALLBACK_WORD_MULTIPLIER;

        let a = fallback_tokens(text);
        let b = fallback_tokens(text);

        assert_eq!(a, expected, "fallback must be word_count × 1.3");
        assert_eq!(a, b, "fallback must be deterministic");
    }

    /// Fallback word count ignores empty splits from runs of whitespace/punctuation.
    #[test]
    fn test_fallback_tier_ignores_empty_splits() {
        let text = "  spaced   out --- words  ";
        // words: "spaced", "out", "words" ⇒ 3
        assert_eq!(fallback_tokens(text), 3.0 * FALLBACK_WORD_MULTIPLIER);
    }

    /// Fallback monotonic on length (independent of faithful tier availability).
    #[test]
    fn test_fallback_tier_monotonic() {
        assert!(fallback_tokens(&long_text()) > fallback_tokens(short_text()));
    }

    // --- unicode / multi-byte ---

    /// Non-ASCII / multi-byte text is handled without panic and counted consistently.
    #[test]
    fn test_token_proxy_unicode_no_panic_consistent() {
        let entry = entry_with("héllo wörld café naïve 日本語 emoji 😀 test");
        let first = token_proxy(&entry);
        let second = token_proxy(&entry);
        assert!(first >= 0.0, "unicode payload must produce a finite count");
        assert_eq!(first, second, "unicode counting must be deterministic");
    }
}
