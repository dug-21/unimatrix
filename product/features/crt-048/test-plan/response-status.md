# Test Plan: `mcp/response/status.rs` — Status Report Types and Formatting

## Component Scope

Defines `StatusReport` struct, `StatusReportJson` struct, `Default` impl,
`From<&StatusReport>` impl, and three format branches (text/markdown/JSON).
This component owns the type definitions that make all downstream test fixture
sites in `mod.rs` fail to compile when fields are removed.

---

## Risks Owned by This Component

| Risk | Coverage Requirement |
|------|---------------------|
| R-05 | JSON output must not contain `confidence_freshness_score` or `stale_confidence_count` keys |
| R-08 | `From<&StatusReport>` impl must not reference removed fields |

---

## Static Analysis Assertions (Grep — MANDATORY in Stage 3c)

**Assertion 1 — no freshness fields anywhere in `mcp/` (R-05, AC-06):**
```bash
grep -rn "confidence_freshness\|stale_confidence_count" \
    crates/unimatrix-server/src/mcp/
```
Must return zero matches.

**Assertion 2 — field removal from `From` impl (R-08):**
```bash
grep -n "confidence_freshness_score\|stale_confidence_count" \
    crates/unimatrix-server/src/mcp/response/status.rs
```
Must return zero matches. Any hit in this file indicates an incomplete `From` impl
or `Default` impl update.

---

## Unit Test Expectations

### Struct field removal (AC-06, AC-14)

The primary test mechanism is compile-time:
- `StatusReport` must not have `confidence_freshness_score: f64` field
- `StatusReport` must not have `stale_confidence_count: u64` field
- `StatusReportJson` must not have either field
- `Default` impl must not set either field
- `From<&StatusReport>` impl must not assign either field

`cargo build --workspace` enforces this. No additional unit test is needed for
struct field absence — it is a compile-time invariant.

---

### Text format — freshness line absent

**Test name:** `test_status_text_no_freshness_line`

**Arrangement:**
```rust
let report = StatusReport::default();
let output = format_status_report(&report, "text");
```

**Assertions:**
```rust
assert!(!output.contains("confidence_freshness"),
    "text format must not contain confidence_freshness");
assert!(!output.contains("stale_confidence"),
    "text format must not contain stale_confidence");
```

The coherence line in text format must contain only graph, contradiction,
embedding, and lambda values — not freshness.

---

### Markdown format — freshness bullet absent

**Test name:** `test_status_markdown_no_freshness_bullet`

**Arrangement:**
```rust
let report = StatusReport::default();
let output = format_status_report(&report, "markdown");
```

**Assertions:**
```rust
assert!(!output.contains("Confidence Freshness"),
    "markdown must not contain Confidence Freshness bullet");
assert!(!output.contains("stale_confidence"),
    "markdown must not contain stale confidence count");
```

---

### JSON format — field key absence (R-05, AC-06)

**Test name:** `test_status_json_no_freshness_keys`

**Arrangement:**
```rust
let report = StatusReport::default();
let output = format_status_report(&report, "json");
```

**Assertions:**
```rust
assert!(!output.contains("confidence_freshness_score"),
    "JSON must not serialize confidence_freshness_score key");
assert!(!output.contains("stale_confidence_count"),
    "JSON must not serialize stale_confidence_count key");
```

This is distinct from the integration test in `test_tools.py` — it tests the
formatting function directly without a live server. Both tests are required:
this one catches a `serde(rename)` or leftover field issue in the struct, while
the integration test catches a wire-protocol level regression.

---

### `From<&StatusReport>` field mapping (R-08)

**Test name:** (covered by build gate + JSON key-absence test)

If the `From<&StatusReport>` impl for `StatusReportJson` retained a stale
assignment like `confidence_freshness_score: report.confidence_freshness_score`,
the build fails immediately because the field no longer exists on `StatusReport`.

The subtler R-08 failure — an assignment reusing another field by name — is
covered by:
1. Grep assertion: zero matches for `confidence_freshness_score` in `status.rs`
2. JSON key-absence test: if the field were silently assigned from another field,
   the key would appear in JSON output

No additional test beyond these two is required.

---

## Integration Test Expectations

### New integration test: `test_status_json_no_freshness_fields` (R-05, AC-06)

This test must be added to `suites/test_tools.py` in Stage 3c (as specified in
OVERVIEW.md). It is the authoritative end-to-end verification that the wire
protocol JSON does not contain removed keys.

```python
def test_status_json_no_freshness_fields(server):
    """AC-06, R-05: Removed JSON keys must be absent from context_status response."""
    response = server.call_tool("context_status", {"format": "json"})
    import json
    payload = json.loads(response["content"][0]["text"])
    assert "confidence_freshness_score" not in payload
    assert "stale_confidence_count" not in payload
```

Fixture: `server` (default, fresh DB). No special state required.

---

## Deleted Tests (verify absence in Stage 3c)

These four tests in `mcp/response/mod.rs` must not exist post-delivery:

| Test Name | Reason Deleted |
|-----------|---------------|
| `test_coherence_json_all_fields` | Asserted removed fields present in JSON |
| `test_coherence_json_f64_precision` | Referenced `confidence_freshness_score` value |
| `test_coherence_stale_count_rendering` | Rendered `stale_confidence_count` in output |
| `test_coherence_default_values` | Asserted `confidence_freshness_score == 1.0` and `stale_confidence_count == 0` |

In Stage 3c: confirm none appear in `cargo test -- --list` for `unimatrix-server`.

---

## Edge Cases

| Scenario | Expected |
|----------|---------|
| `StatusReport::default()` formatted as JSON | No `confidence_freshness_score` or `stale_confidence_count` keys present |
| `StatusReport::default()` formatted as text | Coherence line contains `lambda:`, `graph:`, not `freshness:` |
| `StatusReport::default()` formatted as markdown | No `**Confidence Freshness**` bullet; no `Stale confidence entries:` line |
| `StatusReportJson` serde output with custom `confidence_freshness_score`-named field (should not exist) | Not possible — field removed from struct; any such field is a compile error |
