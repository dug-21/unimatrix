//! On-disk fixture-corpus parse types + path-safety helpers (nan-018, ADR-004).
//!
//! This module defines the *authored* shape of a fixture entry-graph file
//! (`RawFixture`, `RawEntry`, `RawScenario`) and the path-traversal guard
//! applied to every author-supplied file reference. The property-assertion
//! *evaluation* lives in `eval/runner/trust.rs`; the on-disk `ExpectedAssertions`
//! shape is owned by `eval/scenarios/types.rs` (re-exported through the loader)
//! so the trust-metric component imports it rather than redefining it.

use std::path::{Component, Path};

use serde::Deserialize;

use crate::eval::scenarios::ExpectedAssertions;

// ---------------------------------------------------------------------------
// On-disk fixture format
// ---------------------------------------------------------------------------

/// One fixture entry-graph file (`*.toml`) under the corpus directory.
///
/// Mirrors the authored TOML layout (see `corpus-fixtures.md`):
///
/// ```toml
/// [[entries]]
/// alias = "chainA.head"
/// title = "..."; content = "..."
/// status = "Active"           # Active | Deprecated
/// superseded_by = []          # alias references, resolved at load — NOT ids
/// category = "..."
///
/// [[scenarios]]
/// query = "..."
/// [scenarios.assertions]
/// redirect_to_head = ["chainA.head"]
/// forbidden_absent = ["chainA.stale"]
/// rank_below       = [["chainA.b", "chainA.head"]]
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RawFixture {
    /// Entry-graph nodes authored in this file.
    #[serde(default)]
    pub entries: Vec<RawEntry>,
    /// Property-assertion scenarios authored in this file.
    #[serde(default)]
    pub scenarios: Vec<RawScenario>,
}

/// A single authored fixture entry. `alias` is the stable handle; ids are
/// assigned at load and are never authored.
#[derive(Debug, Clone, Deserialize)]
pub struct RawEntry {
    /// Stable handle (e.g. `"chainA.head"`); globally unique across the corpus.
    pub alias: String,
    /// Entry title (searchable text).
    pub title: String,
    /// Entry body content (searchable text).
    pub content: String,
    /// Lifecycle status: `"Active"` or `"Deprecated"`.
    pub status: String,
    /// Aliases of entries that supersede THIS entry (resolved at load).
    ///
    /// `entries[X].superseded_by = ["Y"]` means alias `Y` supersedes alias `X`,
    /// producing a Supersedes edge `X -> Y` (old -> new).
    #[serde(default)]
    pub superseded_by: Vec<String>,
    /// Entry category.
    #[serde(default)]
    pub category: String,
}

/// A single authored property-assertion scenario.
///
/// The primary corpus carries `assertions` (property ground truth) and NEVER a
/// literal-id `expected` list (C-04). The `expected` field exists only so the
/// loader can *detect and reject* a literal-id authoring mistake (R-09).
#[derive(Debug, Clone, Deserialize)]
pub struct RawScenario {
    /// Query text replayed through the search path.
    pub query: String,
    /// Property-based ground truth. Required in the primary corpus.
    #[serde(default)]
    pub assertions: Option<ExpectedAssertions>,
    /// Literal-id labels — BANNED in the primary corpus (R-09). Present only so
    /// the loader can reject it with a clear error rather than silently ignore.
    #[serde(default)]
    pub expected: Option<Vec<u64>>,
    /// Optional explicit scenario id; defaults to `"corpus-{index}"` at load.
    #[serde(default)]
    pub id: Option<String>,
}

impl RawScenario {
    /// True when this scenario carries a literal-id `expected` list (banned).
    pub fn has_literal_expected(&self) -> bool {
        self.expected.is_some()
    }

    /// True when this scenario carries neither assertions nor `expected`.
    ///
    /// A null/empty `assertions` (no property of any kind) counts as null —
    /// the primary corpus must carry at least one property assertion (R-09).
    pub fn is_null_ground_truth(&self) -> bool {
        let assertions_empty = self
            .assertions
            .as_ref()
            .map(ExpectedAssertions::is_empty)
            .unwrap_or(true);
        assertions_empty && self.expected.is_none()
    }
}

// ---------------------------------------------------------------------------
// Path-traversal guard
// ---------------------------------------------------------------------------

/// Reject an author-supplied relative file reference that would escape `root`.
///
/// A safe reference is relative and contains no `..` / root / prefix
/// components — it must resolve strictly *under* the controlled corpus
/// directory. Absolute paths and any `..` component are rejected
/// lexically (before any filesystem access), defeating both
/// `/etc/passwd`-style absolute escapes and `../../secret` traversals.
///
/// Returns the joined path under `root` when safe.
pub fn safe_join(root: &Path, reference: &str) -> Result<std::path::PathBuf, PathTraversal> {
    let rel = Path::new(reference);
    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PathTraversal {
                    reference: reference.to_string(),
                });
            }
        }
    }
    Ok(root.join(rel))
}

/// A rejected path reference (absolute or `..`-traversing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTraversal {
    /// The offending author-supplied reference.
    pub reference: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_safe_join_allows_relative_under_root() {
        let root = PathBuf::from("/tmp/corpus");
        let joined = safe_join(&root, "chainA.toml").expect("relative ref is safe");
        assert_eq!(joined, PathBuf::from("/tmp/corpus/chainA.toml"));
    }

    #[test]
    fn test_safe_join_allows_nested_relative() {
        let root = PathBuf::from("/tmp/corpus");
        let joined = safe_join(&root, "sub/chainA.toml").expect("nested relative ref is safe");
        assert_eq!(joined, PathBuf::from("/tmp/corpus/sub/chainA.toml"));
    }

    #[test]
    fn test_safe_join_rejects_parent_traversal() {
        let root = PathBuf::from("/tmp/corpus");
        let err = safe_join(&root, "../secret.toml").expect_err("`..` must be rejected");
        assert_eq!(err.reference, "../secret.toml");
    }

    #[test]
    fn test_safe_join_rejects_nested_parent_traversal() {
        let root = PathBuf::from("/tmp/corpus");
        assert!(safe_join(&root, "sub/../../etc/passwd").is_err());
    }

    #[test]
    fn test_safe_join_rejects_absolute() {
        let root = PathBuf::from("/tmp/corpus");
        let err = safe_join(&root, "/etc/passwd").expect_err("absolute path must be rejected");
        assert_eq!(err.reference, "/etc/passwd");
    }

    #[test]
    fn test_raw_scenario_literal_expected_detected() {
        let s = RawScenario {
            query: "q".into(),
            assertions: None,
            expected: Some(vec![1, 2]),
            id: None,
        };
        assert!(s.has_literal_expected());
    }

    #[test]
    fn test_raw_scenario_null_ground_truth_detected() {
        let s = RawScenario {
            query: "q".into(),
            assertions: None,
            expected: None,
            id: None,
        };
        assert!(s.is_null_ground_truth());

        let empty_assertions = RawScenario {
            query: "q".into(),
            assertions: Some(ExpectedAssertions::default()),
            expected: None,
            id: None,
        };
        assert!(
            empty_assertions.is_null_ground_truth(),
            "empty assertions set is null ground truth"
        );
    }

    #[test]
    fn test_raw_scenario_with_assertions_is_not_null() {
        let s = RawScenario {
            query: "q".into(),
            assertions: Some(ExpectedAssertions {
                forbidden_absent: vec!["chainA.stale".into()],
                ..Default::default()
            }),
            expected: None,
            id: None,
        };
        assert!(!s.is_null_ground_truth());
        assert!(!s.has_literal_expected());
    }
}
