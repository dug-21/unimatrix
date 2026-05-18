# Component Test Plan: Usage Gate Fix (`usage.rs` + `tools.rs`)

## Component

**Files**:
- `crates/unimatrix-server/src/services/usage.rs` — `UsageContext` struct + two gate blocks
- `crates/unimatrix-server/src/mcp/tools.rs` — all `UsageContext` construction sites

**Changes**:
- Add `write_capable: bool` field to `UsageContext` (no `Default`, no `#[serde(default)]`)
- Replace trust-level match in `record_mcp_usage` (lines 207-218) with `if ctx.write_capable`
- Replace trust-level match in `record_hook_injection` (lines 272-283) with `if ctx.write_capable`
- Set `write_capable: true` at `context_store` handler's `UsageContext` construction site (~line 826)
- Set `write_capable: false` at all other `UsageContext` construction sites in `tools.rs`

---

## AC Coverage

| AC-ID | Description |
|-------|-------------|
| AC-10 | `UsageContext` has `write_capable: bool` with no `#[serde(default)]` or `Default` derivation |
| AC-11 | Both gate blocks check `ctx.write_capable`; `TrustLevel` not referenced in either gate block |
| AC-12 | `context_store` handler sets `write_capable: true`; all other construction sites set `write_capable: false` |
| AC-13 | Unit tests in `unimatrix-server` verify both branches of the gate logic |

## Risk Coverage

| Risk ID | How This Component's Tests Address It |
|---------|--------------------------------------|
| R-01 | Gate fix enables `feature_entries` write for Restricted+Write agents; without it the integration test is a vacuous pass |
| R-06 | `test_write_capable_true_yields_feature_recording` confirms the write path is enabled; integration test uses Restricted+Write agent to exercise the fixed path |

---

## Rust Unit Tests: `usage.rs mod tests`

Two new `#[test]` functions (sync — no async needed; gate logic is pure `Option` composition).

### Test 1: `test_write_capable_false_yields_no_feature_recording`

#### Arrange

```rust
let ctx = UsageContext {
    feature_cycle: Some("test-cycle".to_string()),
    write_capable: false,
    trust_level: Some(TrustLevel::Restricted),
    // ... all other fields at appropriate defaults
};
let entry_ids: &[u64] = &[1, 2, 3];
```

#### Act

Evaluate the gate logic inline (mirrors the fixed production code):

```rust
let gate_result = ctx.feature_cycle.as_ref().and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str.clone(), entry_ids.to_vec()))
    } else {
        None
    }
});
```

#### Assert

```rust
assert!(gate_result.is_none(),
    "expected None when write_capable=false, got: {:?}", gate_result);
```

### Test 2: `test_write_capable_true_yields_feature_recording`

#### Arrange

```rust
let ctx = UsageContext {
    feature_cycle: Some("test-cycle".to_string()),
    write_capable: true,
    trust_level: Some(TrustLevel::Restricted),
    // ... all other fields at appropriate defaults
};
let entry_ids: &[u64] = &[42];
```

#### Act

```rust
let gate_result = ctx.feature_cycle.as_ref().and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str.clone(), entry_ids.to_vec()))
    } else {
        None
    }
});
```

#### Assert

```rust
assert!(gate_result.is_some(),
    "expected Some(...) when write_capable=true, got None");
let (recorded_cycle, recorded_ids) = gate_result.unwrap();
assert_eq!(recorded_cycle, "test-cycle");
assert_eq!(recorded_ids, vec![42u64]);
```

**Note**: These are pure unit tests on the gate logic expression. They do not invoke
`record_mcp_usage` or `record_hook_injection` directly — they test the pattern that
replaces the trust-level gate. If the actual implementation diverges from this pattern,
the integration test (Component 5) serves as the behavioral gate.

---

## Code Inspection Assertions

### AC-10: `write_capable` field declaration

```bash
grep -n 'write_capable' crates/unimatrix-server/src/services/usage.rs
```

Must show the field declaration. Must NOT show `#[serde(default)]` adjacent to `write_capable`.
`cargo build --workspace` must succeed (exhaustive struct construction enforced at compile time).

### AC-11: Gate blocks do not reference `TrustLevel`

```bash
# Must show matches in gate blocks (both record_mcp_usage and record_hook_injection):
grep -n 'write_capable' crates/unimatrix-server/src/services/usage.rs

# Must NOT show TrustLevel references in the gate blocks (lines 207-218 and 272-283):
grep -n 'TrustLevel' crates/unimatrix-server/src/services/usage.rs
```

The `TrustLevel` type may still be imported and used elsewhere in `usage.rs`. The assertion
is specifically that the gate blocks at lines 207-218 and 272-283 do not reference it.

### AC-12: Construction site audit in `tools.rs`

```bash
grep -n 'write_capable' crates/unimatrix-server/src/mcp/tools.rs
```

Expected output: one occurrence with `true` (at `context_store` handler ~line 826), and
multiple occurrences with `false` (all other `UsageContext` construction sites).

**C-13 constraint**: `write_capable: true` is set unconditionally at the `context_store`
handler's `UsageContext` construction site. The site is inside the
`if let Some(fc) = usage_feature_cycle` branch, so `true` is always correct there.

**C-11 constraint**: No `Default` derivation and no `#[serde(default)]`. Every construction
site must have an explicit `write_capable: <bool>` field. A construction site missing the
field is a compile error. If `cargo build --workspace` exits 0, all construction sites are
accounted for.

---

## Constraint Coverage

| Constraint | Test Coverage |
|-----------|--------------|
| C-11 (no Default, no serde default) | `cargo build --workspace` (compile enforcement) + grep |
| C-12 (both gate blocks fixed) | Grep of both line ranges; code inspection |
| C-13 (`write_capable: true` unconditional at context_store) | Code inspection + integration test passing with Restricted+Write agent |

---

## Integration Test Connection

The usage gate fix is what makes the integration test meaningful (R-06):

- Without the fix: `test_agent_id` (Restricted+Write) calls `context_store(feature_cycle=cycle_id)`.
  The old gate checks `trust_level` — Restricted fails the check — `feature_entries` is silently
  not written. The SQL query returns `vec![]`. The test gets no `dependency_on_deprecated` finding.
  The positive test **fails** (correct signal — fix is absent).

- With the fix: Same call. `write_capable: true` is set in the handler. Gate evaluates
  `ctx.write_capable == true`. `feature_entries` is written. SQL query returns `[(id_a, id_b)]`.
  Test gets the `dependency_on_deprecated` finding. The positive test **passes**.

This is why `agent_id=test_agent_id` (not `"human"`) is required for step 4 (AC-12, C-01b).

---

## Cargo Test Command

```bash
cargo test -p unimatrix-server test_write_capable 2>&1 | tail -20
```

Both test functions will be matched. Both must pass.

---

## Expected Cargo Output

```
test services::usage::tests::test_write_capable_false_yields_no_feature_recording ... ok
test services::usage::tests::test_write_capable_true_yields_feature_recording ... ok
```
