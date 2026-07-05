//! Shared verbosity primitives for the two-axis output contract (vnc-044, ADR-001 SR-03).
//!
//! Single source for the tool-agnostic pieces every adopter imports:
//! - `CONTENT_PREVIEW_BYTES` — the 256-byte preview cap (the only definition of 256 in the
//!   graph path; C-9 / R-12).
//! - `Detail` — the verbosity axis enum (`summary | full`).
//! - `parse_detail` — case-insensitive parser mirroring `parse_format`.
//! - `content_preview` — UTF-8-safe preview builder (never panics; R-01 DoS mitigation).
//!
//! Items are referenced by full path (`crate::mcp::response::verbosity::…`); they are NOT
//! re-exported into `response::*`, keeping the shared enum surface of `response/mod.rs`
//! untouched (C-4 / SR-06).

use crate::error::ServerError;

/// Preview cap in **bytes** (single source; C-9 / SR-03 / R-12). Never re-literalled downstream.
pub const CONTENT_PREVIEW_BYTES: usize = 256;

/// Verbosity axis: `summary` (lean projection) vs `full` (complete record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Summary,
    Full,
}

/// Parse the optional `detail` parameter (verbosity axis).
///
/// Mirrors the established `parse_format` idiom: case-insensitive via `to_lowercase`, `None`
/// → the suite default (`summary`; ADR-001 §3, FR-3, AC-05). An unknown or empty string yields
/// `ServerError::InvalidInput`, which maps to `ERROR_INVALID_PARAMS` via the existing
/// `From<ServerError> for ErrorData`.
pub fn parse_detail(detail: &Option<String>) -> Result<Detail, ServerError> {
    match detail {
        None => Ok(Detail::Summary),
        Some(d) => match d.to_lowercase().as_str() {
            "summary" => Ok(Detail::Summary),
            "full" => Ok(Detail::Full),
            _ => Err(ServerError::InvalidInput {
                field: "detail".to_string(),
                reason: "must be summary or full".to_string(),
            }),
        },
    }
}

/// Build a preview of `content`, capped at [`CONTENT_PREVIEW_BYTES`] bytes and floored to a
/// UTF-8 char boundary. Returns `(preview, truncated)`.
///
/// Uses the codebase's established char-boundary idiom — NOT `&content[..256]` (panics on a
/// non-boundary; request-triggered DoS, R-01/#3706), NOT nightly `str::floor_char_boundary`,
/// NOT `.chars().take(N)` (char count cannot enforce a *byte* cap; R-01/#4350).
///
/// Contract:
/// - `content.len()` is **bytes** (Rust `str::len`), matching the byte cap.
/// - `truncated == content.len() > CONTENT_PREVIEW_BYTES` — decoupled from where `end` floored.
///   257-byte ASCII floors `end` to exactly 256, yet `truncated` MUST be `true` because the
///   length compare (not `end != 256`) decides it (R-02 false-negative trap).
/// - No ellipsis / marker is appended; `truncated` is the only truncation signal.
/// - Total: never panics, never errors (that totality is the R-01 DoS mitigation).
pub fn content_preview(content: &str) -> (String, bool) {
    if content.len() <= CONTENT_PREVIEW_BYTES {
        return (content.to_string(), false);
    }
    let mut end = CONTENT_PREVIEW_BYTES;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    (content[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- R-01: content_preview UTF-8 char-boundary flooring (Critical, DoS) ---

    // #1 — empty content.
    #[test]
    fn test_content_preview_empty_returns_empty_not_truncated() {
        let (preview, truncated) = content_preview("");
        assert_eq!(preview, "");
        assert!(!truncated);
        assert!(!preview.contains('…'));
        assert!(!preview.ends_with("..."));
    }

    // #2 — short ASCII (well below the cap).
    #[test]
    fn test_content_preview_short_ascii_returns_whole_not_truncated() {
        let input = "a".repeat(10);
        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview, input);
        assert!(!truncated);
        assert!(!preview.contains('…'));
    }

    // #3 — 255-byte ASCII (one below the cap).
    #[test]
    fn test_content_preview_255_ascii_returns_whole_not_truncated() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES - 1);
        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview, input);
        assert!(!truncated);
    }

    // #4 — exactly 256 bytes: whole content, NOT truncated at the cap.
    #[test]
    fn test_content_preview_exactly_256_ascii_returns_whole_not_truncated() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES);
        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview, input);
        assert_eq!(preview.len(), CONTENT_PREVIEW_BYTES);
        assert!(!truncated);
        assert!(!preview.contains('…'));
    }

    // #5 — 257 ASCII: floors to a 256-byte prefix, truncated true (the R-02 trap surfaces here).
    #[test]
    fn test_content_preview_257_ascii_floors_to_256_truncated() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES + 1);
        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), CONTENT_PREVIEW_BYTES);
        assert_eq!(preview, input[..CONTENT_PREVIEW_BYTES]);
        assert!(truncated);
        assert!(!preview.contains('…'));
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
    }

    // #6 — 2-byte codepoint straddling byte 256: floors BELOW 256 on a char boundary.
    #[test]
    fn test_content_preview_2byte_straddle_floors_below_256_truncated() {
        // 255 ASCII (bytes 0-254) + é (U+00E9, 2 bytes at 255-256) => 257 bytes; byte 256 is
        // inside é, so end floors to 255 (the start of é).
        let two_byte = char::from_u32(0xE9).unwrap();
        assert_eq!(two_byte.len_utf8(), 2);
        let mut input = "a".repeat(CONTENT_PREVIEW_BYTES - 1);
        input.push(two_byte);
        assert_eq!(input.len(), CONTENT_PREVIEW_BYTES + 1);

        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), 255);
        assert!(truncated);
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(!preview.contains(two_byte));
        assert!(!preview.contains('…'));
    }

    // #7 — 3-byte codepoint straddling byte 256: floors to 254.
    #[test]
    fn test_content_preview_3byte_straddle_floors_to_254_truncated() {
        // 254 ASCII (bytes 0-253) + € (U+20AC, 3 bytes at 254-256) => 257 bytes; byte 256 is
        // inside €, so end floors to 254.
        let three_byte = char::from_u32(0x20AC).unwrap();
        assert_eq!(three_byte.len_utf8(), 3);
        let mut input = "a".repeat(CONTENT_PREVIEW_BYTES - 2);
        input.push(three_byte);
        assert_eq!(input.len(), CONTENT_PREVIEW_BYTES + 1);

        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), 254);
        assert!(truncated);
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(!preview.contains(three_byte));
        assert!(!preview.contains('…'));
    }

    // #8 — 4-byte codepoint straddling byte 256: floors to 253.
    #[test]
    fn test_content_preview_4byte_straddle_floors_to_253_truncated() {
        // 253 ASCII (bytes 0-252) + emoji (U+1F600, 4 bytes at 253-256) => 257 bytes; byte 256
        // is inside the emoji, so end floors to 253.
        let four_byte = '\u{1F600}';
        assert_eq!(four_byte.len_utf8(), 4);
        let mut input = "a".repeat(CONTENT_PREVIEW_BYTES - 3);
        input.push(four_byte);
        assert_eq!(input.len(), CONTENT_PREVIEW_BYTES + 1);

        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), 253);
        assert!(truncated);
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(!preview.contains(four_byte));
        assert!(!preview.contains('…'));
    }

    // #9 — byte 256 lands exactly on a boundary between two multibyte chars: floors to 256.
    #[test]
    fn test_content_preview_boundary_exact_multibyte_floors_to_256_truncated() {
        // 129 × é (2 bytes each) = 258 bytes. Char boundaries fall on every even offset, so
        // byte 256 is a clean boundary between char 128 and char 129; end stays at 256.
        let two_byte = char::from_u32(0xE9).unwrap();
        let input: String = std::iter::repeat(two_byte).take(129).collect();
        assert_eq!(input.len(), 258);
        assert!(input.is_char_boundary(CONTENT_PREVIEW_BYTES));

        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), CONTENT_PREVIEW_BYTES);
        assert!(truncated);
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(!preview.contains('…'));
    }

    // #10 — all-multibyte long content: floors to the last whole char <=256.
    #[test]
    fn test_content_preview_all_multibyte_long_floors_to_whole_char() {
        // 200 × é = 400 bytes. Byte 256 is an even offset => clean boundary; floors to 256.
        let two_byte = char::from_u32(0xE9).unwrap();
        let input: String = std::iter::repeat(two_byte).take(200).collect();
        assert_eq!(input.len(), 400);

        let (preview, truncated) = content_preview(&input);
        assert!(preview.len() <= CONTENT_PREVIEW_BYTES);
        assert_eq!(preview.len() % 2, 0, "must floor to a whole 2-byte char");
        assert!(truncated);
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert!(input.starts_with(&preview));
    }

    // --- R-01 deterministic fuzz (Security): never panics on arbitrary unicode ---

    #[test]
    fn test_content_preview_never_panics_on_arbitrary_unicode() {
        let adversarial: Vec<char> = vec![
            '\u{FEFF}',  // BOM
            '\u{0301}',  // combining acute accent
            '\u{0308}',  // combining diaeresis
            '\u{1F600}', // 4-byte emoji
            '\u{20AC}',  // 3-byte euro
            '\u{00E9}',  // 2-byte é
            'a',         // ASCII
        ];

        // 4-byte emoji at every ASCII-padding offset around the cap (253..=259) — the straddle
        // is placed so its bytes land on either side of byte 256 for each pad length.
        for pad in 250..=260usize {
            let mut s = "a".repeat(pad);
            s.push('\u{1F600}');
            let (preview, truncated) = content_preview(&s);
            assert!(preview.len() <= CONTENT_PREVIEW_BYTES);
            assert!(s.starts_with(&preview), "preview must be a genuine prefix");
            assert!(s.is_char_boundary(preview.len()));
            assert_eq!(truncated, s.len() > CONTENT_PREVIEW_BYTES);
        }

        // Mixed adversarial content of growing length crossing the cap.
        for n in 0..300usize {
            let s: String = adversarial.iter().cycle().take(n).collect();
            let (preview, truncated) = content_preview(&s);
            assert!(preview.len() <= CONTENT_PREVIEW_BYTES);
            assert!(s.starts_with(&preview));
            assert!(s.is_char_boundary(preview.len()));
            assert_eq!(truncated, s.len() > CONTENT_PREVIEW_BYTES);
            assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        }
    }

    // --- R-02: content_truncated == (content.len() > CONTENT_PREVIEW_BYTES) ---

    // NON-NEGOTIABLE: an impl deriving the flag from `end != 256` returns false here and fails.
    #[test]
    fn test_content_truncated_257_ascii_true() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES + 1);
        let (_preview, truncated) = content_preview(&input);
        assert!(
            truncated,
            "257B ASCII floors end to 256 but MUST report truncated"
        );
    }

    #[test]
    fn test_content_truncated_empty_false() {
        assert!(!content_preview("").1);
    }

    #[test]
    fn test_content_truncated_255_false() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES - 1);
        assert!(!content_preview(&input).1);
    }

    #[test]
    fn test_content_truncated_exactly_256_false() {
        let input = "a".repeat(CONTENT_PREVIEW_BYTES);
        assert!(!content_preview(&input).1);
    }

    #[test]
    fn test_content_truncated_multibyte_over_256_true() {
        // 254 ASCII + emoji (4 bytes) = 258 bytes, floors to 254; flag still true.
        let mut input = "a".repeat(CONTENT_PREVIEW_BYTES - 2);
        input.push('\u{1F600}');
        assert_eq!(input.len(), CONTENT_PREVIEW_BYTES + 2);
        let (preview, truncated) = content_preview(&input);
        assert_eq!(preview.len(), 254);
        assert!(truncated);
    }

    // Direct invariant across a spread of lengths — decoupled from the flooring index.
    #[test]
    fn test_content_truncated_equals_byte_compare_invariant() {
        for len in [0usize, 1, 255, 256, 257, 300, 1000] {
            let input = "a".repeat(len);
            let (_preview, truncated) = content_preview(&input);
            assert_eq!(
                truncated,
                len > CONTENT_PREVIEW_BYTES,
                "flag must equal byte compare at len {len}"
            );
        }
    }

    // --- R-10: boundary/empty projection fidelity + no marker/normalization ---

    #[test]
    fn test_content_preview_empty_projection_pair() {
        assert_eq!(content_preview(""), (String::new(), false));
    }

    #[test]
    fn test_content_preview_no_marker_or_normalization() {
        // The preview is exactly the prefix — nothing appended, nothing normalized.
        let input = "a".repeat(CONTENT_PREVIEW_BYTES + 10);
        let (preview, _truncated) = content_preview(&input);
        assert_eq!(preview, input[..CONTENT_PREVIEW_BYTES]);
        assert!(!preview.contains('…'));
        assert!(!preview.ends_with("..."));
    }

    // --- parse_detail: verbosity axis parse ---

    #[test]
    fn test_parse_detail_none_defaults_summary() {
        assert_eq!(parse_detail(&None).unwrap(), Detail::Summary);
    }

    #[test]
    fn test_parse_detail_summary() {
        assert_eq!(
            parse_detail(&Some("summary".to_string())).unwrap(),
            Detail::Summary
        );
    }

    #[test]
    fn test_parse_detail_full() {
        assert_eq!(
            parse_detail(&Some("full".to_string())).unwrap(),
            Detail::Full
        );
    }

    #[test]
    fn test_parse_detail_case_insensitive() {
        for s in ["Summary", "SUMMARY"] {
            assert_eq!(parse_detail(&Some(s.to_string())).unwrap(), Detail::Summary);
        }
        for s in ["Full", "FULL"] {
            assert_eq!(parse_detail(&Some(s.to_string())).unwrap(), Detail::Full);
        }
    }

    #[test]
    fn test_parse_detail_unknown_rejected() {
        for s in ["brief", "bogus", ""] {
            assert!(
                parse_detail(&Some(s.to_string())).is_err(),
                "unknown value {s:?} must error"
            );
        }
    }
}
