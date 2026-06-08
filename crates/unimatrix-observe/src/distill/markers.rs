//! Four marker families for transcript candidate selection (crt-052, C3).
//!
//! ~50 case-insensitive regex patterns grouped into four families, ported from
//! the ass-070 `extractor.py` rule set (FINDINGS Q2/Q4). Built ONCE via a
//! [`OnceLock`] of [`regex::RegexSet`]s — no per-call compilation.
//!
//! # Advisory only (Constraint 6 / Non-Goal)
//! A non-empty match means "this block is a candidate"; the family label is an
//! ADVISORY hint. The server never classifies authoritatively — the calling
//! agent re-classifies and does all semantic extraction. No `FamilyHint`
//! carries extracted meaning.
//!
//! # Dependency posture (AC-13 / NFR-6)
//! Uses only the `regex` crate (a regex-class dependency already vetted in the
//! workspace via `unimatrix-server`; `cargo audit` clean). No heavyweight
//! runtime dependency is introduced.

use std::sync::OnceLock;

use regex::RegexSet;

use crate::types::FamilyHint;

/// The four compiled family pattern sets, built once.
struct FamilyPatterns {
    decision: RegexSet,
    rework: RegexSet,
    lesson: RegexSet,
    phasegate: RegexSet,
}

static FAMILY_SET: OnceLock<FamilyPatterns> = OnceLock::new();

/// Decision-phrase patterns — choices, resolutions, commitments, trade-off
/// selections, ADRs. (Family: [`FamilyHint::Decision`].)
const DECISION_PATTERNS: &[&str] = &[
    r"(?i)\bwe(?:'ll| will| should| are going to)\s+(?:use|go with|adopt|choose|pick|keep|drop|switch)",
    r"(?i)\b(?:decided|decision|deciding)\b",
    r"(?i)\b(?:let's|let us)\s+(?:use|go with|adopt|keep|drop)",
    r"(?i)\bgoing with\b",
    r"(?i)\b(?:i'?ll|i will)\s+(?:use|go with|implement|adopt)",
    r"(?i)\b(?:chosen|choosing|the choice)\b",
    r"(?i)\b(?:adr|architectural decision)\b",
    r"(?i)\b(?:trade[- ]?off|tradeoff)\b",
    r"(?i)\b(?:option [ab]\b|the right cut|the approach is)",
    r"(?i)\b(?:resolved|resolution)\b",
    r"(?i)\b(?:instead of|rather than)\b.*\b(?:we|i|use|go)",
    r"(?i)\b(?:pinned|pin|naming pin)\b",
    r"(?i)\b(?:rejected|off the table|out of scope|not in scope)\b",
];

/// Rework-signal patterns — failures, retries, reverts, regressions, fixes,
/// mistakes. (Family: [`FamilyHint::Rework`].)
const REWORK_PATTERNS: &[&str] = &[
    r"(?i)\b(?:revert|reverted|reverting|roll ?back|rolled back)\b",
    r"(?i)\b(?:broke|broken|breaks|breaking)\b",
    r"(?i)\b(?:regression|regressed)\b",
    r"(?i)\b(?:retry|retried|retrying|try again)\b",
    r"(?i)\b(?:doesn'?t work|didn'?t work|not working|failed|failing|failure)\b",
    r"(?i)\b(?:bug|defect)\b",
    r"(?i)\b(?:fix|fixed|fixing)\b",
    r"(?i)\b(?:mistake|wrong|incorrect|oops)\b",
    r"(?i)\b(?:redo|re-?do|rework|re-?work|re-?implement)\b",
    r"(?i)\b(?:turns out|actually it|it was actually)\b",
    r"(?i)\b(?:undo|undone|back out|backed out)\b",
    r"(?i)\b(?:still (?:fails|failing|broken|not)|again failed)\b",
];

/// Lesson / pattern-learning patterns — gotchas, takeaways, "next time",
/// generalizable insights. (Family: [`FamilyHint::Lesson`].)
const LESSON_PATTERNS: &[&str] = &[
    r"(?i)\b(?:lesson|learned|takeaway|take-away)\b",
    r"(?i)\b(?:gotcha|pitfall|trap|footgun)\b",
    r"(?i)\b(?:next time|in future|going forward|from now on)\b",
    r"(?i)\b(?:turns out that|the trick is|the key is)\b",
    r"(?i)\b(?:note that|important:|caveat|caution)\b",
    r"(?i)\b(?:always|never)\s+(?:use|do|call|hold|store|commit)\b",
    r"(?i)\b(?:pattern|convention|idiom)\b",
    r"(?i)\b(?:remember to|be careful|watch out)\b",
    r"(?i)\b(?:the root cause|root-caused|because the)\b",
    r"(?i)\b(?:should have|shouldn'?t have|would have been better)\b",
    r"(?i)\b(?:realized|realised|insight)\b",
];

/// Phase / gate-marker patterns — protocol-stage transitions, gate outcomes,
/// waves, stages, retros. (Family: [`FamilyHint::PhaseGate`].)
const PHASEGATE_PATTERNS: &[&str] = &[
    r"(?i)\b(?:gate)\b",
    r"(?i)\b(?:pass(?:ed|es)?|fail(?:ed|s)?)\b.*\b(?:gate|review|stage|phase)\b",
    r"(?i)\b(?:phase|stage)\s+\d",
    r"(?i)\b(?:wave [ab]\b|wave 1|wave 2)",
    r"(?i)\b(?:design|delivery|bugfix|research)\s+(?:phase|session|protocol)\b",
    r"(?i)\b(?:scope|specification|architecture|pseudocode|test[- ]plan)\s+(?:phase|stage|gate)\b",
    r"(?i)\b(?:retro(?:spective)?)\b",
    r"(?i)\b(?:merge gate|review gate|acceptance)\b",
    r"(?i)\b(?:stage 3[abc]|session [12])\b",
    r"(?i)\b(?:cycle review|cycle_review|cycle-review)\b",
    r"(?i)\b(?:milestone|checkpoint)\b",
    r"(?i)\b(?:approved|ratified|sign-?off|signed off)\b",
];

/// Compile all four family sets. Patterns are authored to compile; if a pattern
/// fails (it should not), it is dropped from its set rather than panicking, so
/// selection never aborts.
fn build() -> FamilyPatterns {
    FamilyPatterns {
        decision: compile_set(DECISION_PATTERNS),
        rework: compile_set(REWORK_PATTERNS),
        lesson: compile_set(LESSON_PATTERNS),
        phasegate: compile_set(PHASEGATE_PATTERNS),
    }
}

/// Build a [`RegexSet`] from `patterns`. The patterns are static and
/// known-valid; on the impossible event of a compile error, fall back to an
/// empty set (never panics in production).
fn compile_set(patterns: &[&str]) -> RegexSet {
    RegexSet::new(patterns).unwrap_or_else(|_| {
        RegexSet::new(std::iter::empty::<&str>()).expect("empty RegexSet is always valid")
    })
}

fn family_set() -> &'static FamilyPatterns {
    FAMILY_SET.get_or_init(build)
}

/// Match a text block against the four families, returning the advisory hints
/// it triggers (possibly empty). Order is fixed: Decision, Rework, Lesson,
/// PhaseGate. A non-empty result means the block is a selection candidate.
pub fn match_families(text: &str) -> Vec<FamilyHint> {
    let set = family_set();
    let mut hints = Vec::with_capacity(4);
    if set.decision.is_match(text) {
        hints.push(FamilyHint::Decision);
    }
    if set.rework.is_match(text) {
        hints.push(FamilyHint::Rework);
    }
    if set.lesson.is_match(text) {
        hints.push(FamilyHint::Lesson);
    }
    if set.phasegate.is_match(text) {
        hints.push(FamilyHint::PhaseGate);
    }
    hints
}

/// Total pattern count across all four families (~50). Used by the dep/scale
/// test; documents the ass-070 envelope.
pub fn pattern_count() -> usize {
    DECISION_PATTERNS.len()
        + REWORK_PATTERNS.len()
        + LESSON_PATTERNS.len()
        + PHASEGATE_PATTERNS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markers_four_families_match() {
        // A representative block per family yields the correct advisory hint.
        let decision = "After weighing the trade-off we decided to go with Option B.";
        let rework = "That broke the build; reverted the change and will retry.";
        let lesson = "Lesson learned: never hold the lock across an await point.";
        let phasegate = "Stage 3b gate passed; moving to the retrospective phase.";

        assert!(match_families(decision).contains(&FamilyHint::Decision));
        assert!(match_families(rework).contains(&FamilyHint::Rework));
        assert!(match_families(lesson).contains(&FamilyHint::Lesson));
        assert!(match_families(phasegate).contains(&FamilyHint::PhaseGate));
    }

    #[test]
    fn test_markers_unmatched_block_empty() {
        let neutral = "The weather is pleasant and the coffee is warm.";
        assert!(match_families(neutral).is_empty());
    }

    #[test]
    fn test_markers_built_once_oncelock() {
        // Two calls return the same underlying static set (no per-call compile).
        let a = family_set() as *const FamilyPatterns;
        let b = family_set() as *const FamilyPatterns;
        assert_eq!(a, b, "FamilyPatterns is built once and shared");
    }

    #[test]
    fn test_markers_hint_is_advisory_only() {
        // A matched block produces a non-empty Vec<FamilyHint> that carries only
        // the family label — no extracted meaning, no semantic payload.
        let hints = match_families("We decided to adopt the new convention; lesson learned.");
        assert!(!hints.is_empty());
        // The hint is a bare enum (Copy, no fields) — structurally cannot carry
        // extracted semantics. This is the Constraint-6 guarantee.
        for h in &hints {
            let _: FamilyHint = *h; // Copy: no owned extracted data
        }
    }

    #[test]
    fn test_markers_multiple_families_on_one_block() {
        // A block may legitimately trigger multiple families.
        let text = "We decided to revert the change; lesson learned for the next gate.";
        let hints = match_families(text);
        assert!(hints.len() >= 2, "multi-family block: {hints:?}");
    }

    #[test]
    fn test_markers_pattern_count_envelope() {
        // ~50 patterns ported from ass-070 extractor.py.
        let n = pattern_count();
        assert!(
            (40..=60).contains(&n),
            "expected ~50 patterns across four families, got {n}"
        );
    }

    #[test]
    fn test_markers_all_patterns_compile() {
        // Every authored pattern must compile (no silent empty-set fallback
        // masking a broken regex).
        for p in DECISION_PATTERNS
            .iter()
            .chain(REWORK_PATTERNS)
            .chain(LESSON_PATTERNS)
            .chain(PHASEGATE_PATTERNS)
        {
            assert!(
                regex::Regex::new(p).is_ok(),
                "pattern failed to compile: {p}"
            );
        }
    }
}
