//! AC-03 recall/volume + AC-V-FUZZ corpus tests for the selection module.
//!
//! Corpus files are embedded via `include_*!` so these tests stay pure (no
//! runtime filesystem I/O) and the fixtures are reviewable/extensible on disk.
//! The AC-03 fixture is independently authored (anchors-before-port); see
//! `corpus/PROVENANCE.md` (OQ-6 review gate).

use serde_json::Value;

use crate::distill::markers::match_families;
use crate::distill::select::select_candidates;
use crate::types::FamilyHint;

const LABELED_CORPUS: &str = include_str!("corpus/labeled_corpus.jsonl");
const LABELS_JSON: &str = include_str!("corpus/labels.json");
const PROVENANCE: &str = include_str!("corpus/PROVENANCE.md");

const FUZZ_TRUNCATED: &[u8] = include_bytes!("corpus/malformed/truncated.jsonl");
const FUZZ_NON_UTF8: &[u8] = include_bytes!("corpus/malformed/non_utf8.jsonl");
const FUZZ_UNKNOWN_TYPE: &[u8] = include_bytes!("corpus/malformed/unknown_type.jsonl");
const FUZZ_EMBEDDED_NUL: &[u8] = include_bytes!("corpus/malformed/embedded_nul.jsonl");

/// Extract the concatenated text of a JSONL transcript line the same way the
/// parser does, for label-level recall scoring (text-content blocks only).
fn line_text(rec: &Value) -> Option<String> {
    let content = rec.get("message").and_then(|m| m.get("content"))?;
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(obj) = item.as_object() {
                    let is_text = obj.get("type").and_then(Value::as_str) == Some("text");
                    if let Some(t) = obj.get("text").and_then(Value::as_str).filter(|_| is_text) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                }
            }
            Some(out)
        }
        _ => None,
    }
}

// ── OQ-6: provenance-header presence/mode review gate ──────────────────────

#[test]
fn test_corpus_provenance_header_present() {
    // The provenance header must exist and declare one of the two accepted
    // independence modes (OQ-6 / R-20). Stage 3c verifies this gate.
    assert!(
        PROVENANCE.contains("Provenance"),
        "corpus PROVENANCE.md must carry a provenance header"
    );
    let has_anchors = PROVENANCE.contains("anchors-before-port");
    let has_diff_author = PROVENANCE.contains("different-author");
    assert!(
        has_anchors || has_diff_author,
        "provenance header must declare independence mode: anchors-before-port OR different-author"
    );
}

// ── AC-03: block-level recall >= 0.90 on the independent corpus ─────────────

#[test]
fn test_independent_corpus_recall_ge_090() {
    let labels: Value = serde_json::from_str(LABELS_JSON).expect("labels.json parses");
    let label_arr = labels["labels"].as_array().expect("labels array");

    let lines: Vec<&str> = LABELED_CORPUS.lines().filter(|l| !l.is_empty()).collect();

    let mut total = 0usize;
    let mut recalled = 0usize;
    for entry in label_arr {
        let idx = entry["line_index"].as_u64().expect("line_index") as usize;
        total += 1;
        let rec: Value = serde_json::from_str(lines[idx]).expect("labeled line is valid JSON");
        let text = line_text(&rec).unwrap_or_default();
        let hints = match_families(&text);
        if !hints.is_empty() {
            recalled += 1;
        } else {
            // Surface misses to aid fixture/regex iteration.
            eprintln!("RECALL MISS at line {idx}: {text}");
        }
    }
    assert!(total > 0, "fixture must carry labeled blocks");
    let recall = recalled as f64 / total as f64;
    assert!(
        recall >= 0.90,
        "block-level recall {recall:.3} < 0.90 ({recalled}/{total})"
    );
}

/// Each labeled positive should also be SELECTED end-to-end (parsed, matched,
/// surfaced) by `select_candidates` over the whole corpus.
#[test]
fn test_independent_corpus_labeled_blocks_selected() {
    let labels: Value = serde_json::from_str(LABELS_JSON).expect("labels.json parses");
    let label_arr = labels["labels"].as_array().expect("labels array");
    let lines: Vec<&str> = LABELED_CORPUS.lines().filter(|l| !l.is_empty()).collect();

    let selected = select_candidates(LABELED_CORPUS.as_bytes(), "corpus", 0, 1024 * 1024);
    let selected_texts: Vec<&str> = selected.iter().map(|c| c.text.as_str()).collect();

    let mut hits = 0usize;
    for entry in label_arr {
        let idx = entry["line_index"].as_u64().unwrap() as usize;
        let rec: Value = serde_json::from_str(lines[idx]).unwrap();
        let text = line_text(&rec).unwrap_or_default();
        if selected_texts.iter().any(|s| *s == text) {
            hits += 1;
        }
    }
    let recall = hits as f64 / label_arr.len() as f64;
    assert!(
        recall >= 0.90,
        "end-to-end selection recall {recall:.3} < 0.90"
    );
}

// ── NFR-3: selected volume <= 10% of raw bytes ──────────────────────────────

#[test]
fn test_selected_volume_le_10pct() {
    let labels: Value = serde_json::from_str(LABELS_JSON).expect("labels.json parses");
    let raw_bytes = labels["raw_bytes"].as_u64().expect("raw_bytes") as usize;
    assert_eq!(
        raw_bytes,
        LABELED_CORPUS.len(),
        "labels.json raw_bytes must equal the corpus byte length"
    );

    let selected = select_candidates(LABELED_CORPUS.as_bytes(), "corpus", 0, 1024 * 1024);
    let selected_bytes: usize = selected.iter().map(|c| c.text.len()).sum();
    let pct = selected_bytes as f64 / raw_bytes as f64;
    assert!(
        pct <= 0.10 + f64::EPSILON || selected_bytes <= raw_bytes / 10 + 1,
        "selected volume {selected_bytes}B is {pct:.3} of raw {raw_bytes}B; must be <= 10%"
    );
}

// ── Family-coverage sanity: all four families appear in the corpus ──────────

#[test]
fn test_corpus_covers_all_four_families() {
    let selected = select_candidates(LABELED_CORPUS.as_bytes(), "corpus", 0, 1024 * 1024);
    let mut seen = [false; 4];
    for c in &selected {
        for h in &c.family_hints {
            match h {
                FamilyHint::Decision => seen[0] = true,
                FamilyHint::Rework => seen[1] = true,
                FamilyHint::Lesson => seen[2] = true,
                FamilyHint::PhaseGate => seen[3] = true,
            }
        }
    }
    assert!(
        seen.iter().all(|&s| s),
        "corpus exercises all four families: {seen:?}"
    );
}

// ── AC-V-FUZZ: committed malformed corpus → skip-with-count, never panic ────

#[test]
fn test_corpus_fuzz_truncated_no_panic() {
    let out = select_candidates(FUZZ_TRUNCATED, "s", 0, 1024 * 1024);
    // The one complete leading line is recoverable; the truncated tail is skipped.
    assert_eq!(out.len(), 1);
    assert!(out[0].text.contains("decided"));
}

#[test]
fn test_corpus_fuzz_non_utf8_no_panic() {
    let out = select_candidates(FUZZ_NON_UTF8, "s", 0, 1024 * 1024);
    assert_eq!(
        out.len(),
        1,
        "valid leading line survives; non-UTF-8 line skipped"
    );
}

#[test]
fn test_corpus_fuzz_unknown_type_no_panic() {
    let out = select_candidates(FUZZ_UNKNOWN_TYPE, "s", 0, 1024 * 1024);
    assert!(
        out.is_empty(),
        "unknown record types yield no candidates, no panic"
    );
}

#[test]
fn test_corpus_fuzz_embedded_nul_no_panic() {
    let out = select_candidates(FUZZ_EMBEDDED_NUL, "s", 0, 1024 * 1024);
    assert!(out.is_empty(), "embedded-NUL line skipped, no panic");
}

#[test]
fn test_corpus_all_fuzz_files_total() {
    // Aggregate no-panic pass across the whole malformed corpus.
    for f in [
        FUZZ_TRUNCATED,
        FUZZ_NON_UTF8,
        FUZZ_UNKNOWN_TYPE,
        FUZZ_EMBEDDED_NUL,
    ] {
        let _ = select_candidates(f, "s", 0, 1024 * 1024); // must not panic
    }
}
