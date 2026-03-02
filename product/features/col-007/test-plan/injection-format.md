# Test Plan: injection-format

## Unit Tests

### format_injection() core behavior

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_format_injection_single_entry` | 1 entry, content < 1400 bytes | Some(formatted text with header, title, category, confidence%, content) | R-09 |
| `test_format_injection_multiple_entries` | 3 entries, combined < 1400 bytes | All 3 entries in output, in input order | R-09 |
| `test_format_injection_empty` | Empty slice | None | R-04 |
| `test_format_injection_preserves_rank_order` | 3 entries in score order | Output entries in same order as input | R-01 |

### Byte budget enforcement

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_format_injection_under_budget` | Entries totaling 800 bytes | output.len() == 800 (approximately) | R-03 |
| `test_format_injection_at_budget` | Entries totaling exactly MAX_INJECTION_BYTES | output.len() <= MAX_INJECTION_BYTES | R-03 |
| `test_format_injection_over_budget_truncates` | First entry 1000 bytes, second 800 bytes | Second entry truncated, total <= 1400 | R-03 |
| `test_format_injection_remaining_under_100_omits` | First entry 1350 bytes, second 200 bytes | Only first entry included (remaining < 100) | R-03 |

### UTF-8 safety

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_format_injection_ascii_only` | ASCII content | output.len() <= MAX_INJECTION_BYTES, valid UTF-8 | R-03 |
| `test_format_injection_cjk_content` | CJK characters (3 bytes each) | No split characters, valid UTF-8 | R-03 |
| `test_format_injection_emoji_content` | Emoji (4 bytes each) | No split characters, valid UTF-8 | R-03 |
| `test_format_injection_mixed_multibyte` | Mix of ASCII, CJK, emoji | Valid UTF-8, within budget | R-03 |
| `test_truncate_utf8_at_char_boundary` | String with multi-byte chars, truncate mid-char | Result is shorter but valid UTF-8 | R-03 |

### Output format verification

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_format_injection_header_present` | Any non-empty entries | Starts with "--- Unimatrix Context ---" | R-09 |
| `test_format_injection_entry_metadata` | Entry with known title, category, confidence | Output contains "[title] (category, N% confidence)" | R-09 |
| `test_format_injection_entry_id_in_comment` | Entry with id=42 | Output contains "<!-- id:42" | R-09 |
| `test_format_injection_adversarial_content` | Entry with markdown headings, code blocks, XML tags | Output is well-formed, no control chars | R-09 |

## Assertions

- `format_injection` with empty entries returns `None`
- `format_injection` output.len() <= MAX_INJECTION_BYTES (always)
- `format_injection` output is valid UTF-8 (always)
- Entries appear in input order (rank order from server)
- Each entry has title, category, confidence percentage, content, id comment
- Truncation at UTF-8 boundary: `str::is_char_boundary()` holds for truncated output
- No ANSI escape codes or control characters in output

## Edge Cases

- Single entry exactly at MAX_INJECTION_BYTES: included without truncation
- Entry with empty title: formatted as "[] (category, ...)"
- Entry with very high confidence (1.0): shown as "100% confidence"
- Entry with zero confidence (0.0): shown as "0% confidence"
- Entry with content containing "\n\n" (paragraph breaks): preserved
