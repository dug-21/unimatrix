# crt-026: Test Plan — Components 7 & 8: UDS handle_context_search + handle_compact_payload

**File under test**: `crates/unimatrix-server/src/uds/listener.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `listener.rs` (or a new submodule
for histogram-specific tests)

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-11 | `test_compact_payload_histogram_block_present_and_absent` (gate blocker) |
| AC-05 partial | `test_uds_search_path_histogram_pre_resolution` |

Risk coverage: R-05 (UDS path pre-resolution), R-10 (empty histogram suppression).

---

## Component 7: `handle_context_search` Pre-Resolution

Component 7 mirrors the MCP handler pre-resolution (Component 4) but the `session_id`
source is different: it comes from `HookRequest::ContextSearch.session_id` (the hook
payload field), not from `audit_ctx`. `sanitize_session_id` is already applied on the
UDS path before any registry access.

### T-UDS-01: `test_uds_search_path_histogram_pre_resolution`
**AC-05 partial | R-05**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("hook-session-1", None, None);
reg.record_category_store("hook-session-1", "decision");
reg.record_category_store("hook-session-1", "pattern");
```

**Act** (simulating the UDS handler's pre-resolution block):
```rust
// Simulating: session_id from HookRequest::ContextSearch (NOT from audit_ctx)
let session_id_from_hook = Some("hook-session-1".to_string());

let category_histogram: Option<HashMap<String, u32>> =
    session_id_from_hook.as_deref().and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Assert**:
```rust
assert!(
    category_histogram.is_some(),
    "UDS path must pre-resolve histogram to Some when session has stores (R-05)"
);
let h = category_histogram.unwrap();
assert_eq!(h.get("decision"), Some(&1));
assert_eq!(h.get("pattern"), Some(&1));
```

**Notes**: The test simulates the pre-resolution using the same logic as Component 4,
but explicitly documents that `session_id` comes from the hook payload — not `audit_ctx`.
This test validates the R-05 risk scenario: that the UDS path does not silently omit the
histogram resolution.

**Module**: `uds/listener.rs` `#[cfg(test)] mod tests`

---

### T-UDS-02: `test_uds_search_path_empty_session_produces_none_histogram`
**AC-08 partial | R-02**

**Arrange**:
```rust
let reg = SessionRegistry::new();
reg.register_session("hook-session-cold", None, None);
// No stores
```

**Act**:
```rust
let category_histogram: Option<HashMap<String, u32>> =
    Some("hook-session-cold").and_then(|sid| {
        let h = reg.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

**Assert**:
```rust
assert!(
    category_histogram.is_none(),
    "UDS path must produce None histogram for a session with no stores (cold start)"
);
```

**Module**: `uds/listener.rs` `#[cfg(test)] mod tests`

---

### T-UDS-03: `test_sanitize_session_id_before_histogram_lookup` (code-review test)
**R-05 (sanitization ordering)**

This test documents the sanitization ordering invariant. It is a structural documentation
test that runs but has no runtime logic to verify (the sanitize call is in the full handler,
not extractable as a unit).

**Notes**: Verify by code inspection of `handle_context_search` in `listener.rs`:
1. `sanitize_session_id` is called on the raw `session_id` from `HookRequest::ContextSearch`.
2. The histogram pre-resolution (`get_category_histogram`) is placed AFTER the sanitize call.
3. No registry access occurs before the sanitized `session_id` is available.

This is a code-review checklist item for Stage 3b (implementer) and Gate 3a (reviewer).
Document in the RISK-COVERAGE-REPORT as "verified by code review" — not a failing test.

---

## Component 8: `handle_compact_payload` — Histogram Summary

The `format_compaction_payload` function (or the `handle_compact_payload` function that
calls it) must conditionally append the `Recent session activity: ...` block.

### T-UDS-04: `test_compact_payload_histogram_block_present_and_absent` **(GATE BLOCKER)**
**AC-11 | R-10**

#### Subtest A — Non-empty histogram: block present

**Arrange**:
```rust
use std::collections::HashMap;
let mut histogram: HashMap<String, u32> = HashMap::new();
histogram.insert("decision".to_string(), 3);
histogram.insert("pattern".to_string(), 2);
```

**Act**:
```rust
// Call format_compaction_payload (or the equivalent extraction) with non-empty histogram.
// If format_compaction_payload does not take histogram as a direct parameter, simulate
// the appending logic extracted from handle_compact_payload.
let payload = format_compaction_payload(/* existing args */, &histogram);
// Or if histogram is appended inline in handle_compact_payload:
let block = format_histogram_summary(&histogram);
```

**Assert**:
```rust
assert!(
    payload.contains("Recent session activity:"),
    "non-empty histogram must produce 'Recent session activity:' block in CompactPayload"
);
assert!(
    payload.contains("decision") && payload.contains("3"),
    "histogram block must include category name and count"
);
assert!(
    payload.contains("pattern") && payload.contains("2"),
    "histogram block must include all categories"
);
```

#### Subtest B — Empty histogram: block absent

**Arrange**:
```rust
let empty_histogram: HashMap<String, u32> = HashMap::new();
```

**Act**:
```rust
let payload = format_compaction_payload(/* existing args */, &empty_histogram);
// Or:
let no_block = format_histogram_summary(&empty_histogram);
```

**Assert**:
```rust
assert!(
    !payload.contains("Recent session activity"),
    "empty histogram must NOT produce 'Recent session activity' block (no spurious output)"
);
```

**Notes**: The exact function signature depends on the implementation in Stage 3b. If
`format_compaction_payload` takes a `category_histogram: &HashMap<String, u32>` parameter,
test it directly. If the appending is inline in `handle_compact_payload` and not extractable
to a pure function, use the `format_histogram_summary` helper (if extracted) or test the
output of `handle_compact_payload` via a mock `SessionRegistry`. The important assertion is
that the output string either contains or does not contain `"Recent session activity"`.

**Module**: `uds/listener.rs` `#[cfg(test)] mod tests`

---

### T-UDS-05: `test_compact_payload_histogram_top5_cap`
**R-10 (top-5 cap), EC-07**

**Arrange**:
```rust
use std::collections::HashMap;
let mut histogram: HashMap<String, u32> = HashMap::new();
// 7 categories — only top 5 by count should appear
histogram.insert("decision".to_string(), 10);
histogram.insert("pattern".to_string(), 8);
histogram.insert("convention".to_string(), 6);
histogram.insert("lesson-learned".to_string(), 4);
histogram.insert("procedure".to_string(), 2);
histogram.insert("adr".to_string(), 1);         // rank 6 — should NOT appear
histogram.insert("outcome".to_string(), 1);     // rank 7 — should NOT appear
```

**Act**:
```rust
let block = format_histogram_summary(&histogram);
// or extract via format_compaction_payload
```

**Assert**:
```rust
// Top 5 appear
assert!(block.contains("decision"), "top-1 category must appear");
assert!(block.contains("pattern"), "top-2 category must appear");
assert!(block.contains("convention"), "top-3 category must appear");
assert!(block.contains("lesson-learned"), "top-4 category must appear");
assert!(block.contains("procedure"), "top-5 category must appear");
// 6th and 7th do NOT appear
assert!(!block.contains("adr"), "rank-6 category must not appear (top-5 cap)");
assert!(!block.contains("outcome"), "rank-7 category must not appear (top-5 cap)");
```

**Notes**: EC-04 (equal-count tie-breaking): `adr` and `outcome` both have count 1.
Either may appear if only 5 categories are shown, but since we have 7 categories and
the 5th is `procedure` with count 2, neither `adr` nor `outcome` (count 1) should appear
regardless of tie-breaking order.

**Module**: `uds/listener.rs` `#[cfg(test)] mod tests`

---

### T-UDS-06: `test_compact_payload_histogram_format`
**AC-11 format verification**

**Arrange**:
```rust
let mut histogram: HashMap<String, u32> = HashMap::new();
histogram.insert("decision".to_string(), 3);
histogram.insert("pattern".to_string(), 2);
```

**Act**:
```rust
let block = format_histogram_summary(&histogram);
```

**Assert**:
```rust
// Format from SPECIFICATION.md FR-12: "Recent session activity: decision × 3, pattern × 2"
// (sorted by count descending, × separator, comma-separated)
assert!(
    block.contains("Recent session activity:"),
    "block must start with the canonical prefix"
);
// Decision (3) must appear before pattern (2) — sorted by count descending
let decision_pos = block.find("decision").expect("decision must be in block");
let pattern_pos = block.find("pattern").expect("pattern must be in block");
assert!(
    decision_pos < pattern_pos,
    "categories must be sorted by count descending: decision before pattern"
);
// Verify block size is under 100 bytes (MAX_INJECTION_BYTES budget)
assert!(
    block.len() < 100,
    "histogram block must be < 100 bytes for typical sessions; got {} bytes",
    block.len()
);
```

**Module**: `uds/listener.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions

**R-05 sanitization ordering**: Verify `sanitize_session_id` is called before
`get_category_histogram` in `handle_context_search`. This must be confirmed by reading
the implementation in Stage 3b — not testable as a unit test.

**R-12 construction sites**: `ServiceSearchParams` construction in `handle_context_search`
must explicitly set `session_id` and `category_histogram`. Verify by inspection.

**NFR-05 (hook timeout)**: The histogram summary block is pure string formatting on
pre-resolved in-memory data. No I/O, no SQL. Verify no blocking operations are in the
path from `get_category_histogram` call to `format_compaction_payload` return.
