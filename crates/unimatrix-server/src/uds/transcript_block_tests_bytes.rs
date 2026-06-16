//! Tests for the vnc-025 `extract_transcript_block_from_bytes` entry point:
//! §2 golden parity (R-09, AC-11) and §4 prompt-injection bound (R-13).
//! The moved R-14 inventory lives in `transcript_block_tests.rs`.

use super::tests::make_jsonl_file;
use super::*;
use crate::infra::session_transcript::TranscriptBuffer;

// =========================================================================
// §2 from_bytes + golden parity (R-09.1/.2/.3, AC-11) — new in vnc-025.
// =========================================================================

/// Constants pinned in the NEW module (R-14.2): a transposed constant would
/// change hook and server silently.
#[test]
fn test_constants_pinned() {
    assert_eq!(MAX_PRECOMPACT_BYTES, 3000);
    assert_eq!(TAIL_MULTIPLIER, 4);
}

/// Build a fixture JSONL transcript larger than the 12,000-byte tail window
/// so the path variant's seek lands mid-line.
fn make_large_fixture_lines() -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..40 {
        let user = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"text","text":"user message {} padded with enough repeated filler text to push the file well past the tail window boundary for the golden parity test"}}]}}}}"#,
            i
        );
        let asst = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"assistant response {} padded with enough repeated filler text to push the file well past the tail window boundary for the golden parity test"}}]}}}}"#,
            i
        );
        lines.push(user);
        lines.push(asst);
    }
    lines
}

/// HARD GATE (R-09.1, pattern #3426): expected = extract_transcript_block(path)
/// computed at test time — NO hand-written expectation. Actual = the same file
/// bytes split into deltas, applied shuffled + duplicated through a
/// TranscriptBuffer, then extract_transcript_block_from_bytes(contiguous_tail).
/// Byte-for-byte equality.
#[test]
fn test_golden_parity_from_path_vs_streamed_from_bytes() {
    let lines = make_large_fixture_lines();
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let (_tmp, path) = make_jsonl_file(&line_refs);

    let file_bytes = std::fs::read(&path).unwrap();
    assert!(
        file_bytes.len() > MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER,
        "fixture must exceed the tail window to exercise the mid-line seek"
    );

    let expected = extract_transcript_block(&path);
    assert!(
        expected.is_some(),
        "fixture must yield a block via the path variant"
    );

    // Split into deltas and apply in a deterministic shuffled order with
    // duplicates (idempotent merge): evens ascending, odds descending, then
    // every third delta replayed.
    const DELTA_LEN: usize = 509; // prime, so chunk edges never align with lines
    let deltas: Vec<(u64, &[u8])> = file_bytes
        .chunks(DELTA_LEN)
        .enumerate()
        .map(|(i, chunk)| ((i * DELTA_LEN) as u64, chunk))
        .collect();

    let mut buf = TranscriptBuffer::new(
        4 * 1024 * 1024,
        std::sync::Arc::new(crate::infra::transcript_activity::SignatureScanner::empty()),
    );
    for (offset, chunk) in deltas
        .iter()
        .filter(|(o, _)| (o / DELTA_LEN as u64) % 2 == 0)
    {
        buf.apply_delta(*offset, chunk);
    }
    for (offset, chunk) in deltas
        .iter()
        .rev()
        .filter(|(o, _)| (o / DELTA_LEN as u64) % 2 == 1)
    {
        buf.apply_delta(*offset, chunk);
    }
    for (offset, chunk) in deltas.iter().step_by(3) {
        buf.apply_delta(*offset, chunk); // duplicates
    }

    let tail = buf
        .contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)
        .expect("fully merged buffer must yield a contiguous tail");
    let actual = extract_transcript_block_from_bytes(&tail);

    assert_eq!(expected, actual, "golden parity: path vs streamed bytes");
}

/// R-09.2: a window beginning mid-JSONL-line filters the partial line
/// identically to the path variant's mid-line seek (comparison constructed
/// on the same data, no hand-written expectation).
#[test]
fn test_from_bytes_mid_line_tail_start() {
    let first = r#"{"type":"user","message":{"content":[{"type":"text","text":"PARTIAL_FIRST_LINE_MARKER"}]}}"#;
    let second = r#"{"type":"user","message":{"content":[{"type":"text","text":"second message survives"}]}}"#;
    let third = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"third message survives"}]}}"#;
    let full = format!("{}\n{}\n{}", first, second, third);

    // Slice starting inside the first line — what a tail window produces.
    let mid_line_slice = &full.as_bytes()[20..];

    let from_bytes = extract_transcript_block_from_bytes(mid_line_slice);

    // Path-variant comparison on the same data: a file whose content begins
    // mid-line (equivalent to the seek landing mid-line).
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("midline.jsonl");
    std::fs::write(&path, mid_line_slice).unwrap();
    let from_path = extract_transcript_block(path.to_str().unwrap());

    assert_eq!(
        from_bytes, from_path,
        "mid-line filtering must match the path variant"
    );
    let block = from_bytes.expect("complete lines must still yield a block");
    assert!(
        !block.contains("PARTIAL_FIRST_LINE_MARKER"),
        "partial first line must be filtered"
    );
    assert!(block.contains("second message survives"));
}

#[test]
fn test_from_bytes_empty_input_returns_none() {
    assert!(extract_transcript_block_from_bytes(b"").is_none());
    assert!(extract_transcript_block_from_bytes(b"   \n  \n").is_none());
}

#[test]
fn test_from_bytes_all_malformed_returns_none() {
    let bytes = b"not json\nalso not json\n{broken";
    assert!(extract_transcript_block_from_bytes(bytes).is_none());
}

/// Invalid UTF-8 (window boundary splitting a multi-byte char, or arbitrary
/// bytes) is lossy-decoded — no panic, no error.
#[test]
fn test_from_bytes_invalid_utf8_lossy_no_panic() {
    let valid = r#"{"type":"user","message":{"content":[{"type":"text","text":"after garbage"}]}}"#;
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE, 0x80, b'g', b'a', b'r', b'b', b'\n'];
    bytes.extend_from_slice(valid.as_bytes());

    let result = extract_transcript_block_from_bytes(&bytes);
    let block = result.expect("valid line after invalid-UTF-8 garbage must yield a block");
    assert!(block.contains("after garbage"));

    // Pure invalid bytes: no panic, None.
    assert!(extract_transcript_block_from_bytes(&[0xFF, 0xFE, 0x80]).is_none());
}

/// A hole inside the last 12 KB upstream produces a short tail — short input
/// alone must still yield a valid block or None: no panic, no garbage, never
/// pre-window bytes (the buffer guarantees hole exclusion; FR-19).
#[test]
fn test_from_bytes_hole_truncated_window_well_formed() {
    // Well-formed short input (single complete line, far below 12 KB).
    let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"short tail"}]}}"#;
    let block = extract_transcript_block_from_bytes(line.as_bytes())
        .expect("complete short input must yield a block");
    assert!(block.starts_with("=== Recent conversation"));
    assert!(block.ends_with("=== End recent conversation ==="));
    assert!(block.contains("short tail"));

    // Short input starting mid-line with no complete parseable line: None, no panic.
    let partial = &line.as_bytes()[5..40];
    assert!(extract_transcript_block_from_bytes(partial).is_none());
}

/// Same budget rule as the path variant (mirrors
/// extract_transcript_block_respects_byte_budget on direct bytes).
#[test]
fn test_from_bytes_respects_byte_budget() {
    let mut lines = Vec::new();
    for i in 0..20 {
        lines.push(format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"text","text":"user message number {} with some padding to make it longer"}}]}}}}"#,
            i
        ));
        lines.push(format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"assistant response number {} with some padding too"}}]}}}}"#,
            i
        ));
    }
    let bytes = lines.join("\n").into_bytes();
    let result = extract_transcript_block_from_bytes(&bytes);
    if let Some(s) = result {
        assert!(
            s.len() <= MAX_PRECOMPACT_BYTES,
            "byte budget exceeded: {} > {}",
            s.len(),
            MAX_PRECOMPACT_BYTES
        );
        assert!(s.starts_with("=== Recent conversation"));
        assert!(s.ends_with("=== End recent conversation ==="));
    }
    // None is also acceptable if all exchanges are too large for the budget
}

// =========================================================================
// §4 Prompt-injection bound (R-13) — document-and-accept: block CONTENT is
// untrusted by design, identical exposure to the local hook reading a local
// file. No sanitization in scope (like-for-like). The only mitigations are
// the byte budget and structural header/footer wrapping, asserted here.
// =========================================================================

/// R-13.1: an attacker cannot inflate the block. Adversarial 1 MiB inputs —
/// both a single giant turn and many small turns — never exceed the body
/// budget plus header/footer + join framing.
#[test]
fn test_block_bounded_regardless_of_input_size() {
    // Framing allowance: header (~50B) + footer (31B) + joining newlines
    // (one per included turn; turns are at least ~8 bytes each, so at most
    // MAX_PRECOMPACT_BYTES / 8 newlines).
    let framing_bound = 100 + MAX_PRECOMPACT_BYTES / 8;

    // Single 1 MiB "turn": exceeds the budget on its own — no turn fits.
    let giant_text = "x".repeat(1024 * 1024);
    let giant_line = format!(
        r#"{{"type":"user","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
        giant_text
    );
    let result = extract_transcript_block_from_bytes(giant_line.as_bytes());
    if let Some(s) = &result {
        assert!(s.len() <= MAX_PRECOMPACT_BYTES + framing_bound);
    }

    // 1 MiB of many small turns: block emitted, still bounded.
    let small_line =
        r#"{"type":"user","message":{"content":[{"type":"text","text":"spam turn payload"}]}}"#;
    let count = (1024 * 1024) / (small_line.len() + 1);
    let many = vec![small_line; count].join("\n");
    assert!(many.len() >= 1024 * 1024 - small_line.len() - 1);
    let block = extract_transcript_block_from_bytes(many.as_bytes())
        .expect("many small turns must yield a block");
    assert!(
        block.len() <= MAX_PRECOMPACT_BYTES + framing_bound,
        "block inflated: {} > {}",
        block.len(),
        MAX_PRECOMPACT_BYTES + framing_bound
    );
}

/// R-13: output carries the same structural header/footer wrapping as the
/// local hook — the only in-scope mitigation.
#[test]
fn test_block_structurally_wrapped() {
    let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"wrapped"}]}}"#;
    let block = extract_transcript_block_from_bytes(line.as_bytes()).unwrap();
    assert!(block.starts_with("=== Recent conversation (last "));
    assert!(block.contains(" exchanges) ===\n"));
    assert!(block.ends_with("\n=== End recent conversation ==="));
}
