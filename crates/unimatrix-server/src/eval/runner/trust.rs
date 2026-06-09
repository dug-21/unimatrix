//! Property-based trust evaluation (nan-018, ADR-004 #4898).
//!
//! Evaluates the three Wave-1 correctness property families — **absence**,
//! **rank-below**, and **redirect-to-head** — against a ranked result list, so
//! trust rides A/B sweeps alongside P@5/MRR/cost (C-03). This module is a PURE
//! evaluator: it takes the result list, the authored [`ExpectedAssertions`], and
//! the load-validated [`AliasMap`], and returns a [`TrustOutcome`]. The run-loop
//! call-site wiring (`ProfileResult.trust` population) lives in report-extensions
//! (Wave 4) — this evaluator stands alone and is unit-tested directly.
//!
//! ## Load-bearing semantics (do NOT soften — R-11 vacuous-pass trap)
//!
//! | Property            | Pass                                                            | Fail                                                        |
//! |---------------------|-----------------------------------------------------------------|------------------------------------------------------------|
//! | **absence**         | `forbidden ∩ top_k == ∅`                                         | any forbidden present                                      |
//! | **rank-below (A,B)**| both present & `rank(A) > rank(B)`; **A absent**; both absent    | **B absent while A present**; both present & `rank(A) ≤ rank(B)` |
//! | **redirect-to-head**| head present AND no present superseded member outranks it       | head absent; superseded member outranks head              |
//!
//! The asymmetric `rank_below` **B-absent ⇒ FAIL** case is the single most
//! likely correctness bug: a naive "either absent ⇒ pass" inverts it. It is
//! asserted explicitly in the truth-table tests.
//!
//! ## Alias-resolution invariant
//!
//! Every alias an assertion can reference was proven to exist at load (R-10), so
//! [`AliasMap::resolve`] is logically total here. The carry-flag from Wave-1
//! corpus is that `resolve` nonetheless returns `Option<u64>` (no-unwrap rule).
//! A `None` is therefore an **internal invariant violation**, NOT a silent
//! vacuous pass: we surface it as a distinct violation string and fail the
//! relevant verdict, never treating the alias as "absent".

use std::collections::HashMap;

use crate::eval::corpus::AliasMap;
use crate::eval::runner::ScoredEntry;
use crate::eval::scenarios::{EntryRef, ExpectedAssertions};

/// Aggregated trust verdict for one profile/scenario (Integration Surface).
///
/// - `absence_pass` aggregates all `forbidden_absent` assertions.
/// - `rank_pass` aggregates `rank_below` + `redirect_to_head` assertions.
/// - `violations` carries one human-legible string per failure (naming the
///   violated anchor) for the report and for regression diffing.
///
/// A scenario with no assertions yields the trivially-passing
/// `TrustOutcome { absence_pass: true, rank_pass: true, violations: vec![] }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustOutcome {
    /// All `forbidden_absent` assertions held.
    pub absence_pass: bool,
    /// All `rank_below` + `redirect_to_head` assertions held.
    pub rank_pass: bool,
    /// Human-legible per-violation strings (empty when both verdicts pass).
    pub violations: Vec<String>,
}

impl TrustOutcome {
    /// The trivially-passing outcome (no assertions to evaluate).
    pub fn trivial_pass() -> Self {
        TrustOutcome {
            absence_pass: true,
            rank_pass: true,
            violations: Vec::new(),
        }
    }
}

/// The internal assertion class (nan-018, SR-06).
///
/// `ExpectedAssertions` is the on-disk shape; the evaluator lowers each authored
/// item into one of these variants so future correctness properties
/// (quarantine-absent, contradiction-suppressed) slot in without changing the
/// call site. Wave-1 ships EXACTLY these three variants — no speculative ones.
enum Assertion<'a> {
    /// `forbidden_absent`: the alias must be absent from the result top-k.
    Absence(&'a EntryRef),
    /// `rank_below (A, B)`: A must rank strictly below B.
    RankBelow(&'a EntryRef, &'a EntryRef),
    /// `redirect_to_head`: the terminal-active chain head must out- (or co-)
    /// rank every present superseded member.
    RedirectToHead(&'a EntryRef),
}

/// Evaluate the property assertions for one profile/scenario result list.
///
/// `entries` is the ranked result list (index 0 = best rank). `assertions` is
/// the authored ground truth; `alias_map` resolves each `EntryRef` to a concrete
/// id (load-validated, R-10). Returns the aggregated [`TrustOutcome`].
///
/// An empty `entries` yields defined verdicts (absence trivially passes,
/// rank-below both-absent passes, redirect-to-head fails head-absent) — never a
/// panic.
pub fn evaluate_trust(
    entries: &[ScoredEntry],
    assertions: &ExpectedAssertions,
    alias_map: &AliasMap,
) -> TrustOutcome {
    // Rank index: id -> 0-based rank in `entries` (lower = better).
    let rank_of: HashMap<u64, usize> = entries
        .iter()
        .enumerate()
        .map(|(rank, e)| (e.id, rank))
        .collect();

    let mut violations: Vec<String> = Vec::new();
    let mut absence_pass = true;
    let mut rank_pass = true;

    for assertion in lower(assertions) {
        match assertion {
            Assertion::Absence(fref) => {
                let fid = match resolve(alias_map, fref, &mut violations) {
                    Some(id) => id,
                    None => {
                        // Unresolvable alias is an internal invariant error, not
                        // a vacuous pass — fail the absence verdict.
                        absence_pass = false;
                        continue;
                    }
                };
                if let Some(rank) = rank_of.get(&fid) {
                    absence_pass = false;
                    violations.push(format!(
                        "absence: forbidden '{fref}' present at rank {rank}"
                    ));
                }
            }
            Assertion::RankBelow(aref, bref) => {
                let aid = resolve(alias_map, aref, &mut violations);
                let bid = resolve(alias_map, bref, &mut violations);
                let (aid, bid) = match (aid, bid) {
                    (Some(a), Some(b)) => (a, b),
                    _ => {
                        // Resolution failure is an invariant violation, not a
                        // silent pass.
                        rank_pass = false;
                        continue;
                    }
                };
                match (rank_of.get(&aid), rank_of.get(&bid)) {
                    (Some(ra), Some(rb)) => {
                        // A must rank strictly BELOW B (numerically greater rank).
                        if ra <= rb {
                            rank_pass = false;
                            violations.push(format!(
                                "rank_below: '{aref}'(rank {ra}) not below '{bref}'(rank {rb})"
                            ));
                        }
                    }
                    // A absent ⇒ PASS (vacuously below — A can't be too high).
                    (None, _) => {}
                    // *** B absent while A present ⇒ FAIL (load-bearing asymmetry). ***
                    (Some(_), None) => {
                        rank_pass = false;
                        violations.push(format!(
                            "rank_below: '{bref}' absent but '{aref}' present \
                             (should-rank-higher anchor missing)"
                        ));
                    }
                }
            }
            Assertion::RedirectToHead(head_ref) => {
                let head_id = match resolve(alias_map, head_ref, &mut violations) {
                    Some(id) => id,
                    None => {
                        rank_pass = false;
                        continue;
                    }
                };
                match rank_of.get(&head_id) {
                    None => {
                        // Head absent (covers dead-end / no-valid-head chains:
                        // defined FAIL, never a panic).
                        rank_pass = false;
                        violations.push(format!(
                            "redirect_to_head: head '{head_ref}' absent from results"
                        ));
                    }
                    Some(&head_rank) => {
                        // No present superseded member may outrank the head.
                        for m in alias_map.head_members(head_ref) {
                            if let Some(&m_rank) = rank_of.get(m)
                                && m_rank < head_rank
                            {
                                rank_pass = false;
                                violations.push(format!(
                                    "redirect_to_head: superseded member (rank {m_rank}) \
                                     outranks head '{head_ref}'(rank {head_rank})"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    TrustOutcome {
        absence_pass,
        rank_pass,
        violations,
    }
}

/// Lower the on-disk `ExpectedAssertions` into the internal [`Assertion`] class.
///
/// The order is fixed (absence, rank-below, redirect-to-head) so the resulting
/// `violations` ordering is deterministic across runs.
fn lower(a: &ExpectedAssertions) -> impl Iterator<Item = Assertion<'_>> {
    a.forbidden_absent
        .iter()
        .map(Assertion::Absence)
        .chain(a.rank_below.iter().map(|(x, y)| Assertion::RankBelow(x, y)))
        .chain(a.redirect_to_head.iter().map(Assertion::RedirectToHead))
}

/// Resolve an alias to its id, pushing an invariant-violation string on `None`.
///
/// The loader (R-10) guarantees every assertion alias resolves, so `None` is an
/// internal invariant breach — surfaced loudly rather than silently degrading to
/// "absent" (which would be a vacuous pass).
fn resolve(alias_map: &AliasMap, r: &str, violations: &mut Vec<String>) -> Option<u64> {
    match alias_map.resolve(r) {
        Some(id) => Some(id),
        None => {
            violations.push(format!(
                "internal: alias '{r}' did not resolve (load-time R-10 invariant violated)"
            ));
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Build a `ScoredEntry` with only the rank-relevant `id` populated.
    fn scored(id: u64) -> ScoredEntry {
        ScoredEntry {
            id,
            title: format!("entry-{id}"),
            category: "pattern".to_string(),
            final_score: 0.0,
            similarity: 0.0,
            confidence: 0.0,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        }
    }

    /// Build an `AliasMap` from `(alias, id)` pairs plus optional head-member sets.
    /// Uses the public test seam exposed below; see `AliasMap::for_test`.
    fn alias_map(aliases: &[(&str, u64)], head_members: &[(&str, &[u64])]) -> AliasMap {
        let alias_to_id: BTreeMap<EntryRef, u64> =
            aliases.iter().map(|(a, id)| (a.to_string(), *id)).collect();
        let members: BTreeMap<EntryRef, BTreeSet<u64>> = head_members
            .iter()
            .map(|(h, ids)| (h.to_string(), ids.iter().copied().collect()))
            .collect();
        AliasMap::for_test(alias_to_id, members)
    }

    fn absence(refs: &[&str]) -> ExpectedAssertions {
        ExpectedAssertions {
            forbidden_absent: refs.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn rank_below(pairs: &[(&str, &str)]) -> ExpectedAssertions {
        ExpectedAssertions {
            rank_below: pairs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn redirect(heads: &[&str]) -> ExpectedAssertions {
        ExpectedAssertions {
            redirect_to_head: heads.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    // --- rank-below truth table (R-11, AC-03) ---------------------------------

    #[test]
    fn test_rank_below_both_present_a_after_b_pass() {
        // entries: B at rank 0, A at rank 1 ⇒ rank(A) > rank(B) ⇒ pass.
        let entries = [scored(20), scored(10)];
        let map = alias_map(&[("A", 10), ("B", 20)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(
            out.rank_pass,
            "rank(A)>rank(B) must pass: {:?}",
            out.violations
        );
        assert!(out.violations.is_empty());
    }

    #[test]
    fn test_rank_below_both_present_a_before_b_fail() {
        // entries: A at rank 0, B at rank 1 ⇒ rank(A) < rank(B) ⇒ fail.
        let entries = [scored(10), scored(20)];
        let map = alias_map(&[("A", 10), ("B", 20)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(!out.rank_pass, "rank(A)<rank(B) must fail");
        assert!(out.violations.iter().any(|v| v.contains("not below")));
    }

    #[test]
    fn test_rank_below_a_after_b_equal_rank_impossible_but_le_fails() {
        // Defensive: a tie (rank(A) == rank(B)) cannot occur with distinct ids,
        // but the `<=` boundary must FAIL, not pass. Use A==B alias to force it.
        let entries = [scored(10)];
        let map = alias_map(&[("A", 10), ("B", 10)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(!out.rank_pass, "rank(A)==rank(B) must fail (strict-below)");
    }

    #[test]
    fn test_rank_below_a_absent_pass() {
        // A absent ⇒ PASS (vacuously below — A can't be too high if absent).
        let entries = [scored(20)]; // only B present
        let map = alias_map(&[("A", 10), ("B", 20)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(out.rank_pass, "A absent must pass: {:?}", out.violations);
        assert!(out.violations.is_empty());
    }

    #[test]
    fn test_rank_below_b_absent_fail() {
        // *** Load-bearing asymmetry: B absent while A present ⇒ FAIL. ***
        let entries = [scored(10)]; // only A present
        let map = alias_map(&[("A", 10), ("B", 20)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(!out.rank_pass, "B absent while A present MUST fail");
        assert!(
            out.violations.iter().any(|v| v.contains("absent but")),
            "violation must name the missing should-rank-higher anchor: {:?}",
            out.violations
        );
    }

    #[test]
    fn test_rank_below_both_absent_pass() {
        // Both absent ⇒ A-absent arm dominates ⇒ PASS (documented rule).
        let entries: [ScoredEntry; 0] = [];
        let map = alias_map(&[("A", 10), ("B", 20)], &[]);
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(out.rank_pass, "both absent must pass (A-absent dominates)");
        assert!(out.violations.is_empty());
    }

    // --- redirect-to-head (R-11.2, AC-05) -------------------------------------

    #[test]
    fn test_redirect_to_head_present_above_members_pass() {
        // head H at rank 0, member M at rank 1 ⇒ head outranks member ⇒ pass.
        let entries = [scored(100), scored(50)];
        let map = alias_map(&[("H", 100)], &[("H", &[50])]);
        let out = evaluate_trust(&entries, &redirect(&["H"]), &map);
        assert!(
            out.rank_pass,
            "head above member must pass: {:?}",
            out.violations
        );
        assert!(out.violations.is_empty());
    }

    #[test]
    fn test_redirect_to_head_member_absent_pass() {
        // Member not in results ⇒ no outranking possible ⇒ pass.
        let entries = [scored(100)];
        let map = alias_map(&[("H", 100)], &[("H", &[50])]);
        let out = evaluate_trust(&entries, &redirect(&["H"]), &map);
        assert!(out.rank_pass, "absent member cannot outrank head");
        assert!(out.violations.is_empty());
    }

    #[test]
    fn test_redirect_to_head_head_absent_fail() {
        // Head absent ⇒ FAIL (NOT a vacuous pass).
        let entries = [scored(50)]; // member present, head absent
        let map = alias_map(&[("H", 100)], &[("H", &[50])]);
        let out = evaluate_trust(&entries, &redirect(&["H"]), &map);
        assert!(!out.rank_pass, "head absent must fail");
        assert!(
            out.violations
                .iter()
                .any(|v| v.contains("absent from results"))
        );
    }

    #[test]
    fn test_redirect_to_head_member_outranks_head_fail() {
        // member M at rank 0, head H at rank 1 ⇒ member outranks head ⇒ FAIL.
        let entries = [scored(50), scored(100)];
        let map = alias_map(&[("H", 100)], &[("H", &[50])]);
        let out = evaluate_trust(&entries, &redirect(&["H"]), &map);
        assert!(!out.rank_pass, "member outranking head must fail");
        assert!(out.violations.iter().any(|v| v.contains("outranks head")));
    }

    #[test]
    fn test_redirect_to_head_no_valid_head_defined_failure() {
        // Dead-end chain: head alias resolves to an id absent from results
        // (find_terminal_active yielded no live head) ⇒ defined FAIL, no panic.
        let entries = [scored(50)];
        // No head_members registered for H (empty set) and head id absent.
        let map = alias_map(&[("H", 999)], &[]);
        let out = evaluate_trust(&entries, &redirect(&["H"]), &map);
        assert!(
            !out.rank_pass,
            "dead-end / no-valid-head must be a defined fail"
        );
        assert!(
            out.violations
                .iter()
                .any(|v| v.contains("absent from results"))
        );
    }

    // --- absence (R-11.3, AC-02) ----------------------------------------------

    #[test]
    fn test_absence_forbidden_not_in_topk_pass() {
        let entries = [scored(10), scored(20)];
        let map = alias_map(&[("F", 99)], &[]);
        let out = evaluate_trust(&entries, &absence(&["F"]), &map);
        assert!(out.absence_pass, "forbidden absent must pass");
        assert!(out.violations.is_empty());
    }

    #[test]
    fn test_absence_forbidden_present_fail() {
        let entries = [scored(10), scored(99)];
        let map = alias_map(&[("F", 99)], &[]);
        let out = evaluate_trust(&entries, &absence(&["F"]), &map);
        assert!(!out.absence_pass, "forbidden present must fail");
        assert!(
            out.violations
                .iter()
                .any(|v| v.contains("present at rank 1"))
        );
    }

    #[test]
    fn test_absence_empty_result_set_pass() {
        // Empty result set ⇒ forbidden trivially absent ⇒ pass (vacuous edge;
        // AC-14 non-vacuous test uses a non-empty set elsewhere).
        let entries: [ScoredEntry; 0] = [];
        let map = alias_map(&[("F", 99)], &[]);
        let out = evaluate_trust(&entries, &absence(&["F"]), &map);
        assert!(out.absence_pass, "empty set ⇒ absence trivially passes");
    }

    // --- empty-result-set composite + k>corpus --------------------------------

    #[test]
    fn test_empty_result_set_defined_verdicts_no_panic() {
        let entries: [ScoredEntry; 0] = [];
        let map = alias_map(
            &[("A", 10), ("B", 20), ("F", 99), ("H", 100)],
            &[("H", &[50])],
        );
        let assertions = ExpectedAssertions {
            forbidden_absent: vec!["F".to_string()],
            rank_below: vec![("A".to_string(), "B".to_string())],
            redirect_to_head: vec!["H".to_string()],
        };
        let out = evaluate_trust(&entries, &assertions, &map);
        // absence: trivially passes; rank-below both-absent: passes;
        // redirect-to-head: head absent ⇒ rank_pass fails.
        assert!(out.absence_pass);
        assert!(!out.rank_pass, "head absent in empty set ⇒ rank fail");
    }

    #[test]
    fn test_k_larger_than_corpus_absence_is_strict() {
        // When every entry is returned (k >= corpus), a forbidden entry present
        // makes the absence assertion strict (it WILL fire).
        let entries = [scored(10), scored(20), scored(99)];
        let map = alias_map(&[("F", 99)], &[]);
        let out = evaluate_trust(&entries, &absence(&["F"]), &map);
        assert!(!out.absence_pass, "k>=corpus, forbidden present ⇒ fail");
    }

    // --- alias resolution stability (R-10) ------------------------------------

    #[test]
    fn test_alias_resolution_stable_across_id_assignment() {
        // Same logical assertion, two different id assignments ⇒ same verdict.
        let assertions = rank_below(&[("A", "B")]);

        // Assignment 1: A=10 (rank 1), B=20 (rank 0) ⇒ pass.
        let map1 = alias_map(&[("A", 10), ("B", 20)], &[]);
        let entries1 = [scored(20), scored(10)];
        let out1 = evaluate_trust(&entries1, &assertions, &map1);

        // Assignment 2: A=70 (rank 1), B=80 (rank 0) ⇒ pass (same verdict).
        let map2 = alias_map(&[("A", 70), ("B", 80)], &[]);
        let entries2 = [scored(80), scored(70)];
        let out2 = evaluate_trust(&entries2, &assertions, &map2);

        assert_eq!(out1, out2, "verdict must be stable across id renumber");
        assert!(out1.rank_pass);
    }

    // --- no-assertions / trivial pass -----------------------------------------

    #[test]
    fn test_no_assertions_trivial_pass() {
        let entries = [scored(10)];
        let map = alias_map(&[], &[]);
        let out = evaluate_trust(&entries, &ExpectedAssertions::default(), &map);
        assert_eq!(out, TrustOutcome::trivial_pass());
    }

    // --- unresolvable alias is an invariant violation, not a vacuous pass -----

    #[test]
    fn test_unresolvable_absence_alias_fails_not_vacuous() {
        // Alias 'F' is NOT in the map (a load invariant breach). The evaluator
        // must FAIL the absence verdict, never treat it as silently absent.
        let entries = [scored(10)];
        let map = alias_map(&[], &[]);
        let out = evaluate_trust(&entries, &absence(&["F"]), &map);
        assert!(
            !out.absence_pass,
            "unresolvable alias must not be a vacuous pass"
        );
        assert!(out.violations.iter().any(|v| v.contains("did not resolve")));
    }

    #[test]
    fn test_unresolvable_rank_below_alias_fails() {
        let entries = [scored(10)];
        let map = alias_map(&[("A", 10)], &[]); // B missing
        let out = evaluate_trust(&entries, &rank_below(&[("A", "B")]), &map);
        assert!(!out.rank_pass, "unresolvable rank-below alias must fail");
        assert!(out.violations.iter().any(|v| v.contains("did not resolve")));
    }

    // --- aggregation across mixed assertions ----------------------------------

    #[test]
    fn test_mixed_assertions_aggregate_independently() {
        // absence fails, rank-below passes ⇒ absence_pass=false, rank_pass=true.
        let entries = [scored(99), scored(20), scored(10)]; // F at 0, B at 1, A at 2
        let map = alias_map(&[("A", 10), ("B", 20), ("F", 99)], &[]);
        let assertions = ExpectedAssertions {
            forbidden_absent: vec!["F".to_string()],
            rank_below: vec![("A".to_string(), "B".to_string())], // rank A=2 > B=1 ⇒ pass
            redirect_to_head: vec![],
        };
        let out = evaluate_trust(&entries, &assertions, &map);
        assert!(!out.absence_pass);
        assert!(out.rank_pass);
        assert_eq!(out.violations.len(), 1, "only the absence violation");
    }
}
