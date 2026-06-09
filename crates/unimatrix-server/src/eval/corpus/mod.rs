//! Fixture-corpus loader + property-assertion parsing (nan-018, ADR-004).
//!
//! Materializes hand-authored fixture entry-graphs into a snapshot DB the
//! existing `EvalServiceLayer::from_profile` consumes unchanged, and produces an
//! [`AliasMap`] for property-based trust evaluation. The on-disk
//! `ExpectedAssertions` shape is owned by `eval/scenarios/types.rs` and shared
//! with the trust-metric component (nan-018 Integration Surface).
//!
//! Submodules:
//! - [`assertions`] — authored fixture parse types + path-traversal guard.
//! - [`loader`] — the load pipeline, `AliasMap`, `LoadedCorpus`, `CorpusError`.
//! - [`embed`] — embed-at-load (ADR-002 branch (b)): makes the loaded snapshot
//!   end-to-end searchable so AC-14 trust assertions are non-vacuous (R-15).

pub mod assertions;
pub mod embed;
pub mod loader;

pub use assertions::{PathTraversal, safe_join};
pub use embed::{embed_and_write_vectors, load_fixture_corpus_with_embeddings};
pub use loader::{AliasMap, CorpusError, LoadedCorpus, load_fixture_corpus};

#[cfg(test)]
mod fixtures_tests;
#[cfg(test)]
mod tests;
