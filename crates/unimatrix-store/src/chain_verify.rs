//! Transport-agnostic cross-version hash-chain verifier (nxs-014).
//!
//! The single integrity oracle. Given a slice of [`EntryRecord`], it recomputes
//! each entry's content hash and verifies every populated chain link against the
//! authoritative `supersedes` predecessor. It performs **no I/O** and takes **no
//! CLI/MCP types** (C-07) — callers (import, CLI, future MCP) supply the entries.
//!
//! Violations are returned as data ([`ChainReport`]), never as `Err` — the report
//! is always produced and callers decide the failure surface. See ADR-001 (core
//! placement) and ADR-003 (verify semantics).

use std::collections::HashMap;
use std::fmt;

use crate::compute_content_hash;
use crate::schema::EntryRecord;

/// Outcome of a corpus-wide chain verification pass.
///
/// `checked` counts every entry examined (== `entries.len()`). `skipped_legacy`
/// counts the subset whose `previous_hash` is empty (unverifiable-legacy / genesis)
/// — a legacy entry is counted in BOTH `checked` and `skipped_legacy`.
#[derive(Debug)]
pub struct ChainReport {
    /// Number of entries examined (equals the corpus length).
    pub checked: usize,
    /// Entries with an empty `previous_hash`, skipped as unverifiable-legacy.
    pub skipped_legacy: usize,
    /// Every detected violation, naming the offending entry id (fail-loud).
    pub violations: Vec<ChainViolation>,
}

impl ChainReport {
    /// True iff no violations were recorded. False whenever `violations` is
    /// non-empty (NFR-06 / R-12 fail-loud posture).
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    /// Human-readable summary. Names every offending entry id + break kind on a
    /// failure; emits ids and hashes only, never raw `content` (avoids echoing
    /// untrusted content / terminal-escape injection).
    pub fn describe(&self) -> String {
        if self.is_clean() {
            return format!(
                "chain OK: {} entries checked, {} legacy (unverifiable) skipped",
                self.checked, self.skipped_legacy
            );
        }
        let header = format!(
            "chain integrity FAILED: {} violation(s) over {} entries checked",
            self.violations.len(),
            self.checked
        );
        let lines: Vec<String> = self
            .violations
            .iter()
            .map(|v| format!("  entry {}: {}", v.entry_id, v.kind))
            .collect();
        format!("{}\n{}", header, lines.join("\n"))
    }
}

impl fmt::Display for ChainReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

/// A single detected integrity violation, keyed to the offending entry id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainViolation {
    /// Id of the entry that failed verification.
    pub entry_id: u64,
    /// What kind of break was detected.
    pub kind: ViolationKind,
}

/// The specific integrity break detected for an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// Recomputed content hash differs from the stored `content_hash`.
    ContentHashMismatch {
        /// Hash recomputed from `title`/`content`.
        computed: String,
        /// Hash stored on the record.
        stored: String,
    },
    /// `previous_hash` does not match the named predecessor's `content_hash`.
    ChainLinkMismatch {
        /// Predecessor id named by the `supersedes` edge.
        predecessor_id: u64,
        /// What the link SHOULD be (predecessor's `content_hash`).
        expected: String,
        /// What the link IS (the successor's `previous_hash`).
        found: String,
    },
    /// The `supersedes` predecessor is absent from the corpus.
    MissingPredecessor {
        /// Predecessor id that could not be resolved.
        predecessor_id: u64,
    },
    /// `previous_hash` is populated but `supersedes` is `None` (dangling link).
    DanglingPreviousHash,
}

impl fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViolationKind::ContentHashMismatch { computed, stored } => write!(
                f,
                "content hash mismatch (computed {computed}, stored {stored})"
            ),
            ViolationKind::ChainLinkMismatch {
                predecessor_id,
                expected,
                found,
            } => write!(
                f,
                "chain link mismatch vs predecessor {predecessor_id} (expected {expected}, found {found})"
            ),
            ViolationKind::MissingPredecessor { predecessor_id } => {
                write!(f, "predecessor {predecessor_id} not found in corpus")
            }
            ViolationKind::DanglingPreviousHash => {
                write!(
                    f,
                    "previous_hash set but supersedes is None (dangling link)"
                )
            }
        }
    }
}

/// Verify content hashes and chain links across a corpus of entries.
///
/// O(n): builds an `id -> &EntryRecord` index, then a single linear pass. Pure
/// and total — never panics, never returns `Err`. Empty slice yields a clean
/// report. See module docs and ADR-003 for the algorithm.
pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport {
    let index: HashMap<u64, &EntryRecord> = entries.iter().map(|e| (e.id, e)).collect();
    let mut report = ChainReport {
        checked: 0,
        skipped_legacy: 0,
        violations: Vec::new(),
    };

    for e in entries {
        report.checked += 1;

        // (1) Content-hash recompute (AC-04 half). FROZEN signature (C-01).
        let computed = compute_content_hash(&e.title, &e.content);
        if computed != e.content_hash {
            report.violations.push(ChainViolation {
                entry_id: e.id,
                kind: ViolationKind::ContentHashMismatch {
                    computed,
                    stored: e.content_hash.clone(),
                },
            });
            // Do NOT `continue`: an entry can have BOTH a content mismatch and a
            // broken link. Fall through to the chain-link check.
        }

        // (2) Chain-link check (AC-03; C-02 legacy skip).
        if e.previous_hash.is_empty() {
            // Unverifiable-legacy / genesis — NOT a break (FR-06, matches import:429).
            report.skipped_legacy += 1;
            continue;
        }

        // previous_hash populated -> must resolve via the authoritative supersedes edge.
        match e.supersedes {
            None => {
                // Populated link with no chain edge should not occur by
                // construction; fail loud, never ignore (R-09).
                report.violations.push(ChainViolation {
                    entry_id: e.id,
                    kind: ViolationKind::DanglingPreviousHash,
                });
            }
            Some(pred_id) => match index.get(&pred_id) {
                None => report.violations.push(ChainViolation {
                    entry_id: e.id,
                    kind: ViolationKind::MissingPredecessor {
                        predecessor_id: pred_id,
                    },
                }),
                Some(pred) => {
                    if pred.content_hash != e.previous_hash {
                        report.violations.push(ChainViolation {
                            entry_id: e.id,
                            kind: ViolationKind::ChainLinkMismatch {
                                predecessor_id: pred_id,
                                expected: pred.content_hash.clone(),
                                found: e.previous_hash.clone(),
                            },
                        });
                    }
                    // else: link verified — nothing to record.
                }
            },
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Status;

    /// Build a consistent `EntryRecord`: `content_hash` is computed from
    /// `title`/`content` so a "clean" fixture is genuinely self-consistent.
    fn chained(
        id: u64,
        supersedes: Option<u64>,
        prev_hash: &str,
        version: u32,
        title: &str,
        content: &str,
        status: Status,
    ) -> EntryRecord {
        EntryRecord {
            id,
            title: title.to_string(),
            content: content.to_string(),
            topic: "t".to_string(),
            category: "c".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status,
            confidence: 0.0,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: compute_content_hash(title, content),
            previous_hash: prev_hash.to_string(),
            version,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }

    fn hash_of(title: &str, content: &str) -> String {
        compute_content_hash(title, content)
    }

    // ---- Clean walk + legacy skip (R-03, AC-03) --------------------------

    #[test]
    fn test_verify_clean_two_hop_chain_is_clean() {
        let a = chained(1, None, "", 1, "A", "genesis", Status::Deprecated);
        let b = chained(
            2,
            Some(1),
            &hash_of("A", "genesis"),
            2,
            "B",
            "second",
            Status::Active,
        );
        let corpus = vec![a, b];
        let report = verify_entries(&corpus);
        assert!(report.is_clean());
        assert!(report.violations.is_empty());
        assert_eq!(report.checked, corpus.len());
        assert!(report.checked >= 2);
        assert_eq!(report.skipped_legacy, 1);
    }

    #[test]
    fn test_verify_mixed_legacy_and_chained_is_clean() {
        let l1 = chained(1, None, "", 1, "L1", "legacy one", Status::Active);
        let l2 = chained(2, None, "", 1, "L2", "legacy two", Status::Active);
        let l3 = chained(3, None, "", 1, "L3", "legacy three", Status::Deprecated);
        let p = chained(4, None, "", 1, "P", "predecessor", Status::Deprecated);
        let s = chained(
            5,
            Some(4),
            &hash_of("P", "predecessor"),
            2,
            "S",
            "successor",
            Status::Active,
        );
        let corpus = vec![l1, l2, l3, p, s];
        let report = verify_entries(&corpus);
        assert!(report.is_clean());
        // Every empty-prev entry counted as skipped (l1,l2,l3,p) — proves the skip is real.
        let empty_prev = corpus.iter().filter(|e| e.previous_hash.is_empty()).count();
        assert_eq!(report.skipped_legacy, empty_prev);
        assert_eq!(report.skipped_legacy, 4);
        assert_eq!(report.checked, corpus.len());
    }

    #[test]
    fn test_verify_genesis_supersedes_none_empty_prev_skipped_not_dangling() {
        let g = chained(1, None, "", 1, "G", "genesis", Status::Active);
        let corpus = vec![g];
        let report = verify_entries(&corpus);
        assert!(report.is_clean());
        assert_eq!(report.skipped_legacy, 1);
        // Must NOT be recorded as DanglingPreviousHash.
        assert!(report.violations.is_empty());
    }

    // ---- Deprecated predecessor present in checked set (R-02, Critical) ----

    #[test]
    fn test_verify_deprecated_predecessor_counted_as_checked() {
        let mut pred = chained(1, None, "", 1, "P", "pred", Status::Deprecated);
        pred.superseded_by = Some(2);
        let succ = chained(
            2,
            Some(1),
            &hash_of("P", "pred"),
            2,
            "S",
            "succ",
            Status::Active,
        );
        let corpus = vec![pred, succ];
        let report = verify_entries(&corpus);
        assert!(report.is_clean());
        // The Deprecated predecessor is counted in checked (core does NOT filter by status).
        assert_eq!(report.checked, corpus.len());
        assert_eq!(report.checked, 2);
    }

    // ---- Each ViolationKind by a dedicated scenario (R-09, AC-04) ----------

    #[test]
    fn test_verify_content_hash_mismatch_named() {
        let mut e = chained(7, None, "", 1, "T", "original", Status::Active);
        // Mutate content WITHOUT recomputing content_hash — stored hash now stale.
        e.content = "tampered".to_string();
        let stored = e.content_hash.clone();
        let corpus = vec![e];
        let report = verify_entries(&corpus);
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].entry_id, 7);
        match &report.violations[0].kind {
            ViolationKind::ContentHashMismatch {
                computed,
                stored: s,
            } => {
                assert_eq!(s, &stored);
                assert_ne!(computed, s);
                assert_eq!(computed, &hash_of("T", "tampered"));
            }
            other => panic!("expected ContentHashMismatch, got {other:?}"),
        }
    }

    #[test]
    fn test_verify_chain_link_mismatch_named() {
        let pred = chained(1, None, "", 1, "P", "pred", Status::Deprecated);
        // Successor content is consistent, but previous_hash is a wrong non-empty value.
        let mut succ = chained(
            2,
            Some(1),
            &hash_of("P", "pred"),
            2,
            "S",
            "succ",
            Status::Active,
        );
        succ.previous_hash = "deadbeef".to_string();
        let expected = hash_of("P", "pred");
        let corpus = vec![pred, succ];
        let report = verify_entries(&corpus);
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].entry_id, 2);
        assert_eq!(
            report.violations[0].kind,
            ViolationKind::ChainLinkMismatch {
                predecessor_id: 1,
                expected,
                found: "deadbeef".to_string(),
            }
        );
    }

    #[test]
    fn test_verify_missing_predecessor() {
        let succ = chained(2, Some(999), "somehash", 2, "S", "succ", Status::Active);
        let corpus = vec![succ];
        let report = verify_entries(&corpus);
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].entry_id, 2);
        assert_eq!(
            report.violations[0].kind,
            ViolationKind::MissingPredecessor {
                predecessor_id: 999
            }
        );
    }

    #[test]
    fn test_verify_dangling_previous_hash() {
        // Non-empty previous_hash but supersedes == None — fail loud, NOT a legacy-skip.
        let e = chained(3, None, "somehash", 2, "D", "dangle", Status::Active);
        let corpus = vec![e];
        let report = verify_entries(&corpus);
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].entry_id, 3);
        assert_eq!(
            report.violations[0].kind,
            ViolationKind::DanglingPreviousHash
        );
        assert_eq!(report.skipped_legacy, 0);
    }

    // ---- Both violations on one entry (no early continue) ------------------

    #[test]
    fn test_verify_both_content_and_link_violation_on_one_entry() {
        let pred = chained(1, None, "", 1, "P", "pred", Status::Deprecated);
        let mut succ = chained(
            2,
            Some(1),
            &hash_of("P", "pred"),
            2,
            "S",
            "succ",
            Status::Active,
        );
        // Break content hash AND the link on the same entry.
        succ.content = "tampered".to_string();
        succ.previous_hash = "wronglink".to_string();
        let corpus = vec![pred, succ];
        let report = verify_entries(&corpus);
        let for_succ: Vec<_> = report
            .violations
            .iter()
            .filter(|v| v.entry_id == 2)
            .collect();
        assert_eq!(for_succ.len(), 2, "expected content + link violations");
        assert!(
            for_succ
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::ContentHashMismatch { .. }))
        );
        assert!(
            for_succ
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::ChainLinkMismatch { .. }))
        );
    }

    // ---- Fail-loud posture (R-12, NFR-06) ---------------------------------

    #[test]
    fn test_is_clean_false_whenever_violations_nonempty() {
        let kinds = vec![
            ViolationKind::ContentHashMismatch {
                computed: "a".to_string(),
                stored: "b".to_string(),
            },
            ViolationKind::ChainLinkMismatch {
                predecessor_id: 1,
                expected: "a".to_string(),
                found: "b".to_string(),
            },
            ViolationKind::MissingPredecessor { predecessor_id: 1 },
            ViolationKind::DanglingPreviousHash,
        ];
        for kind in kinds {
            let report = ChainReport {
                checked: 1,
                skipped_legacy: 0,
                violations: vec![ChainViolation { entry_id: 1, kind }],
            };
            assert!(!report.is_clean());
        }
    }

    #[test]
    fn test_report_names_every_offending_id() {
        // Two distinct broken entries.
        let mut e1 = chained(11, None, "", 1, "A", "a", Status::Active);
        e1.content = "mutated".to_string(); // content mismatch
        let e2 = chained(22, None, "nonempty", 2, "B", "b", Status::Active); // dangling
        let corpus = vec![e1, e2];
        let report = verify_entries(&corpus);
        assert!(!report.is_clean());
        let out = report.describe();
        assert!(out.contains("11"), "output must name entry 11: {out}");
        assert!(out.contains("22"), "output must name entry 22: {out}");
        // Display mirrors describe().
        assert_eq!(format!("{report}"), out);
    }

    // ---- Edge cases -------------------------------------------------------

    #[test]
    fn test_verify_empty_corpus_is_clean() {
        let report = verify_entries(&[]);
        assert!(report.is_clean());
        assert_eq!(report.checked, 0);
        assert_eq!(report.skipped_legacy, 0);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn test_verify_single_legacy_entry_is_clean() {
        let g = chained(1, None, "", 1, "G", "only", Status::Active);
        let corpus = vec![g];
        let report = verify_entries(&corpus);
        assert!(report.is_clean());
        assert_eq!(report.skipped_legacy, 1);
        assert_eq!(report.checked, 1);
    }

    #[test]
    fn test_verify_long_chain_versions_monotonic_single_pass() {
        // Build a 10-hop all-consistent chain: entry k supersedes k-1.
        let mut corpus: Vec<EntryRecord> = Vec::new();
        let mut prev_hash = String::new();
        let mut prev_title_content: Option<(String, String)> = None;
        for k in 1..=10u64 {
            let title = format!("E{k}");
            let content = format!("content {k}");
            let supersedes = if k == 1 { None } else { Some(k - 1) };
            let prev = if k == 1 {
                String::new()
            } else {
                let (pt, pc) = prev_title_content.as_ref().unwrap();
                hash_of(pt, pc)
            };
            let status = if k == 10 {
                Status::Active
            } else {
                Status::Deprecated
            };
            corpus.push(chained(
                k, supersedes, &prev, k as u32, &title, &content, status,
            ));
            prev_hash = prev;
            prev_title_content = Some((title, content));
        }
        let _ = prev_hash;
        let report = verify_entries(&corpus);
        assert!(report.is_clean(), "{}", report.describe());
        assert_eq!(report.checked, 10);
        assert_eq!(report.skipped_legacy, 1); // only genesis
    }

    #[test]
    fn test_verify_mid_chain_deprecated_predecessor_with_own_predecessor() {
        // A(genesis) <- B(mid, both predecessor & successor) <- C
        let a = chained(1, None, "", 1, "A", "a", Status::Deprecated);
        let mut b = chained(
            2,
            Some(1),
            &hash_of("A", "a"),
            2,
            "B",
            "b",
            Status::Deprecated,
        );
        b.superseded_by = Some(3);
        let c = chained(3, Some(2), &hash_of("B", "b"), 3, "C", "c", Status::Active);
        let corpus = vec![a, b, c];
        let report = verify_entries(&corpus);
        assert!(report.is_clean(), "{}", report.describe());
        assert_eq!(report.checked, 3);
        assert_eq!(report.skipped_legacy, 1);
    }

    #[test]
    fn test_verify_legacy_predecessor_new_successor() {
        // Legacy predecessor (own previous_hash empty) with a chained successor.
        let mut legacy_pred = chained(1, None, "", 1, "L", "legacy", Status::Deprecated);
        legacy_pred.superseded_by = Some(2);
        let succ = chained(
            2,
            Some(1),
            &hash_of("L", "legacy"),
            2,
            "S",
            "succ",
            Status::Active,
        );
        let corpus = vec![legacy_pred, succ];
        let report = verify_entries(&corpus);
        assert!(report.is_clean(), "{}", report.describe());
        // Predecessor's own genesis link legacy-skipped; successor hop checked.
        assert_eq!(report.skipped_legacy, 1);
        assert_eq!(report.checked, 2);
    }

    // ---- Signature guard (C-07, AC-11) ------------------------------------

    #[test]
    fn test_verify_entries_signature_is_slice_of_entryrecord() {
        // Compile-time proof the core takes &[EntryRecord] and returns ChainReport
        // (no CLI/MCP types). If this stops compiling the signature drifted.
        let f: fn(&[EntryRecord]) -> ChainReport = verify_entries;
        let report = f(&[]);
        assert!(report.is_clean());
    }
}
