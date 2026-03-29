# vnc-012 Implementation Brief
# Accept String-Encoded Integers for All Numeric MCP Parameters

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-012/SCOPE.md |
| Architecture | product/features/vnc-012/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-012/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-012/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-012/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| mcp/serde_util.rs (new) | pseudocode/serde_util.md | test-plan/serde_util.md |
| mcp/tools.rs (modified) | pseudocode/tools.md | test-plan/tools.md |
| mcp/mod.rs (modified) | pseudocode/mod.md | test-plan/mod.md |
| infra-001/test_tools.py (modified) | pseudocode/infra_001.md | test-plan/infra_001.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component
Map lists the four components confirmed by the architecture. Actual file paths are filled
during delivery.

---

## Goal

Add server-side coercion of string-encoded integers for all nine numeric MCP parameter
fields across five parameter structs in `unimatrix-server`, so that agents emitting
`"id": "3770"` receive a valid response instead of an `invalid type: string` MCP error.
The fix operates entirely at the serde deserialization boundary via three `pub(crate)`
helper functions in a new `mcp/serde_util.rs` module, leaving validation logic, handler
code, rmcp, and JSON Schema output unchanged (except `evidence_limit` gaining the
semantically correct `minimum: 0`).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Where to place the three deserializer helpers | New private submodule `crates/unimatrix-server/src/mcp/serde_util.rs`, consistent with the existing `response/` submodule pattern; `pub(crate)` visibility scoped to `mcp` namespace | SCOPE.md OQ-2, ARCHITECTURE.md | product/features/vnc-012/architecture/ADR-001-serde-util-submodule.md |
| How to preserve `type: integer` in the published JSON Schema after adding `deserialize_with` | Use `#[schemars(with = "i64")]` / `#[schemars(with = "Option<i64>")]` / `#[schemars(with = "Option<u64>")]` paired attributes — declarative, no schema function required, verified against schemars 1.2.1 | SCOPE.md OQ-1, ARCHITECTURE.md | product/features/vnc-012/architecture/ADR-002-schemars-with-override.md |
| Whether an integration test over the MCP transport is required (SR-03) | Yes — mandatory. IT-01 (`test_get_with_string_id`) and IT-02 (`test_deprecate_with_string_id`) in `product/test/infra-001/suites/test_tools.py`, both marked `@pytest.mark.smoke`. A Rust AC-13 in-process test is also required. Unit tests alone are insufficient because they do not exercise the rmcp `Parameters<T>` dispatch path | SCOPE.md OQ-4, RISK-TEST-STRATEGY.md R-02 | product/features/vnc-012/architecture/ADR-003-integration-test-requirement.md |
| How to guarantee `None`-for-absent on optional fields using `deserialize_with` | Pair `#[serde(default)]` with every `#[serde(deserialize_with)]` on optional fields. Require 20+ mandatory unit tests: one absent-field test and one null-field test per optional field, separate from the happy-path tests | SCOPE.md OQ-3, RISK-TEST-STRATEGY.md R-01 R-03 | product/features/vnc-012/architecture/ADR-004-optional-field-none-guarantee.md |
| How to handle usize overflow on 32-bit targets | Use `usize::try_from(val_u64)` — never `as usize`. Parse via `u64` first to reject negatives, then convert. Overflow returns a serde error | SCOPE.md OQ-3 | product/features/vnc-012/architecture/ADR-004-optional-field-none-guarantee.md |
| Whether float JSON Numbers (e.g., `3.0` as Number type) should be coerced or rejected | Strictly rejected. `visit_f64` and `visit_f32` must return `de::Error::invalid_type(de::Unexpected::Float(v), &self)`. Silent truncation forbidden | SPECIFICATION.md FR-13 | (codified in SPECIFICATION.md FR-13; no standalone ADR) |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/serde_util.rs` | Create | Three `pub(crate)` serde deserializer functions: `deserialize_i64_or_string`, `deserialize_opt_i64_or_string`, `deserialize_opt_usize_or_string`; unit tests for all acceptance, rejection, null, and absent paths |
| `crates/unimatrix-server/src/mcp/mod.rs` | Modify | Add `mod serde_util;` declaration to expose the new submodule |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Add paired `#[serde(deserialize_with)]` + `#[schemars(with)]` + `#[serde(default)]` attributes to nine fields across five structs; add Rust unit tests for all AC criteria; add schema snapshot test (AC-10) |
| `product/test/infra-001/suites/test_tools.py` | Modify | Add `test_get_with_string_id` (IT-01) and `test_deprecate_with_string_id` (IT-02), both marked `@pytest.mark.smoke` |

---

## Data Structures

### Affected parameter structs (field annotations added, no type changes)

```rust
// GetParams — tools.rs (existing struct, field annotated)
struct GetParams {
    #[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    id: i64,
    // ... other fields unchanged
}

// DeprecateParams — same annotation pattern on `id: i64`
// QuarantineParams — same annotation pattern on `id: i64`

// CorrectParams — annotation on `original_id: i64`
struct CorrectParams {
    #[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    original_id: i64,
    // ... other fields unchanged
}

// LookupParams — two optional fields
struct LookupParams {
    #[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
    #[schemars(with = "Option<i64>")]
    id: Option<i64>,

    #[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
    #[schemars(with = "Option<i64>")]
    limit: Option<i64>,
    // ... other fields unchanged
}

// SearchParams — one optional field
struct SearchParams {
    #[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
    #[schemars(with = "Option<i64>")]
    k: Option<i64>,
    // ... other fields unchanged
}

// BriefingParams — one optional field
struct BriefingParams {
    #[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
    #[schemars(with = "Option<i64>")]
    max_tokens: Option<i64>,
    // ... other fields unchanged
}

// RetrospectiveParams — one optional usize field
struct RetrospectiveParams {
    #[serde(default, deserialize_with = "serde_util::deserialize_opt_usize_or_string")]
    #[schemars(with = "Option<u64>")]
    evidence_limit: Option<usize>,
    // ... other fields unchanged
}
```

### String-encoded integer (domain model)
A JSON String value whose content is a valid base-10 integer literal accepted by
`str::parse::<i64>()` (e.g., `"3770"`, `"0"`, `"-5"`). Does not include floats, hex
literals, whitespace-padded strings, or non-numeric text.

---

## Function Signatures

```rust
// crates/unimatrix-server/src/mcp/serde_util.rs

/// Accept JSON Number (integer) or JSON String containing a base-10 integer.
/// Rejects float Numbers, non-numeric strings, booleans, arrays, objects.
pub(crate) fn deserialize_i64_or_string<'de, D>(d: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>;

/// As above, for Option<i64> fields.
/// JSON null -> Ok(None). Absent field (via #[serde(default)]) -> Ok(None).
/// JSON Number or String -> Ok(Some(i64)).
pub(crate) fn deserialize_opt_i64_or_string<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>;

/// As above, for Option<usize> fields (evidence_limit).
/// Parses via u64 first (rejects negatives at parse time).
/// Converts to usize via usize::try_from — never `as usize`.
/// JSON null -> Ok(None). Absent field -> Ok(None).
pub(crate) fn deserialize_opt_usize_or_string<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>;
```

### Visitor requirements (per FR-13)
Each Visitor implementation must implement:
- `visit_i64` / `visit_u64`: pass through (or convert for usize)
- `visit_str` / `visit_string`: parse via `str::parse`; error on failure
- `visit_f64` / `visit_f32`: return `de::Error::invalid_type(de::Unexpected::Float(v), &self)`
- `visit_none` / `visit_unit` (optional variants): return `Ok(None)`

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | rmcp is pinned at `version = "=0.16.0"`. No version bump. |
| C-02 | No new crate-level dependencies. Use only existing `serde` (with `derive`) and `serde_json`. |
| C-03 | `deserialize_opt_*` helpers must handle JSON null (key present, value null) and absent field (key missing) as distinct code paths. Requires `#[serde(default)]` on all five optional fields. |
| C-04 | All nine affected fields must retain `type: integer` in the published JSON Schema. `evidence_limit` may gain `minimum: 0`. |
| C-05 | No coercion of float JSON Numbers, booleans, arrays, or objects. |
| C-06 | `usize::try_from(val_u64)` — never `as usize`. |
| C-07 | String coercion must not be applied to non-numeric fields (`format`, `agent_id`, `status`, `category`, etc.). No changes to `infra/validation.rs`. |
| C-08 | Helpers live in `mcp/serde_util.rs` only — not promoted to crate-level. |

---

## Dependencies

| Dependency | Version | Role |
|------------|---------|------|
| `serde` | workspace (derive feature) | `Deserializer`, `Visitor`, `de::Error` traits in `serde_util.rs` |
| `serde_json` | workspace | Used in tests (`serde_json::from_str`) and in the rmcp dispatch path |
| `schemars` | 1.2.1 | `#[schemars(with = "T")]` attribute for schema override on annotated fields |
| `rmcp` | =0.16.0 (pinned) | `Parameters<T>` transparent serde wrapper; `CallToolRequestParams`; `ServerHandler::call_tool` |
| `unimatrix-server` (existing) | — | `UnimatrixServer`, `tools.rs` structs, `make_server()` helper for schema snapshot test (AC-10) |
| `unimatrix-store` | workspace | `NewEntry`, `Store::insert` used in AC-13 integration test setup |

---

## NOT in Scope

- String coercion for non-numeric fields (`format`, `agent_id`, `category`, `status`,
  `query`, `topic`, `content`, `reason`, `tags`, `feature`, `title`, `source`).
- Changes to `crates/unimatrix-server/src/infra/validation.rs` — `validated_id`,
  `validated_k`, `validated_limit`, `validated_max_tokens` are untouched.
- Coercion of float strings (e.g., `"3.5"`) — rejected as non-numeric.
- Coercion of float JSON Numbers (e.g., `3.0` as a Number type) — rejected per FR-13.
- Coercion of boolean parameters.
- A new `serde_util` crate or any cross-crate shared utility.
- Version bump of rmcp.
- Updates to agent definitions (`.claude/agents/`), protocol files
  (`.claude/protocols/`), or `CLAUDE.md` — tracked as GH #448 follow-up.
- Schema changes beyond the `minimum: 0` permitted on `evidence_limit`.

---

## Alignment Status

**Result: PASS — no variances.**

All six alignment checks pass (Vision Alignment, Milestone Fit, Scope Gaps, Scope
Additions, Architecture Consistency, Risk Completeness).

Two prior variances from an earlier review were both resolved before this brief
was produced:

1. Python infra-001 IT-01/IT-02 tests were initially excluded from SPECIFICATION.md
   but required by ARCHITECTURE.md — resolved by adding AC-13 (Rust) + IT-01 + IT-02
   (Python, `@pytest.mark.smoke`) as required criteria in SPECIFICATION.md.

2. FR-13 (float JSON Number rejection via `visit_f64`) was absent from SPEC FR/AC —
   resolved by adding FR-13 and AC-09-FLOAT-NUMBER to SPECIFICATION.md.

No open questions remain unresolved. All ADRs are recorded (Unimatrix #3787–#3790).
