## ADR-004: Mandatory None-for-Absent Tests for Optional Fields (SR-05)

### Context

SR-05 from the risk assessment is High severity: `deserialize_opt_i64_or_string`
and `deserialize_opt_usize_or_string` are used on five optional fields across
four param structs (`LookupParams.id`, `LookupParams.limit`, `SearchParams.k`,
`BriefingParams.max_tokens`, `RetrospectiveParams.evidence_limit`). A subtle
implementation error — for example, deserializing an absent field as `Some(0)`
instead of `None`, or treating a JSON `null` as a parse error — would silently
corrupt optional query parameters used as limits, result counts, and token
budgets across multiple tools.

The existing test pattern (`serde_json::from_str` directly on param structs) is
fully capable of covering the absent-field and null-field cases. However, the
scope's acceptance criteria list only the happy-path and rejection cases
explicitly. SR-05 recommends enumerating the `None` paths as non-negotiable
tests.

The `serde` `Visitor` pattern for optional fields has two distinct absence cases:

1. **JSON null** (`"field": null`): The deserializer is called with a `Null`
   value. The implementation must return `Ok(None)`.
2. **Absent field** (key not present): Serde skips the deserializer entirely
   when the field is absent, but only if `#[serde(default)]` is present on the
   field. Without `default`, serde returns a missing-field error.

For `Option<T>` fields using `deserialize_with`, `#[serde(default)]` is
required to handle the absent case — the `deserialize_with` function is not
called for absent fields, so the field's `Default` implementation (`None`)
provides the value.

### Decision

For each of the five optional fields, require these mandatory tests (flagged as
required in the test plan, not optional coverage):

| Field | Tests required |
|-------|---------------|
| `LookupParams.id` | string input -> `Some(i64)`, integer input -> `Some(i64)`, null input -> `None`, absent -> `None` |
| `LookupParams.limit` | same four cases |
| `SearchParams.k` | same four cases |
| `BriefingParams.max_tokens` | same four cases |
| `RetrospectiveParams.evidence_limit` | string input -> `Some(usize)`, integer input -> `Some(usize)`, null input -> `None`, absent -> `None`, negative string -> error, `u64` overflow -> error |

All five absent-field tests must use a JSON object that does not include the
field key at all (not `null`) to verify the `#[serde(default)]` path.

All five null-field tests must use `"field": null` explicitly.

These tests live in the `#[cfg(test)]` block of `tools.rs` alongside the
existing `test_retrospective_params_evidence_limit` tests, using the same
`serde_json::from_str` pattern. They do not require a running server or mock.

Additionally, `#[serde(default)]` must be present on all five optional fields
that use `deserialize_opt_*` helpers. The implementation agent must verify this
is applied — it is a silent correctness requirement that compilation does not
enforce.

### Consequences

Easier:
- SR-05 silent corruption risk is eliminated by explicit test coverage for
  every absence and null path.
- The `#[serde(default)]` requirement is documented here so it cannot be
  overlooked during implementation.
- Tests are pure unit tests (no server required) — fast to run, no
  environment dependencies.

Harder:
- Test count is higher than the minimum needed for AC-compliance: 20+ tests
  for optional fields alone. This is intentional — the risk is high and tests
  are cheap.
- The implementation agent must remember to add `#[serde(default)]` alongside
  `#[serde(deserialize_with)]` for all optional fields. Missing it causes a
  compile-time deserialization error when the field is absent, not a silent bug
  — but catching it here prevents a gate failure.
