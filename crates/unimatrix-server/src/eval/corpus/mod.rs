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

pub mod assertions;
pub mod loader;

pub use assertions::{PathTraversal, safe_join};
pub use loader::{AliasMap, CorpusError, LoadedCorpus, load_fixture_corpus};

#[cfg(test)]
mod tests;
