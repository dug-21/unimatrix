# Scope Risk Assessment: col-001

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | OUTCOME_INDEX as 13th table increases Store::open complexity; incorrect table init could prevent database opening | Med | Low | Architect should follow exact pattern of existing 12-table initialization in db.rs. Test Store::open with 13 tables explicitly. |
| SR-02 | Structured tag validation in server crate introduces a category-conditional code path in context_store (only triggers for category "outcome"). Conditional validation is a source of subtle bugs. | Med | Med | Architect should isolate validation into its own module. Ensure non-outcome entries are completely unaffected. |
| SR-03 | StoreParams extension (adding feature_cycle) changes the MCP tool schema. Existing callers that send unrecognized fields may behave differently depending on serde deserialization settings. | Med | Low | Verify serde(deny_unknown_fields) is NOT set on StoreParams. Confirm Option<String> defaults to None when omitted. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope says "no retroactive indexing" but agents may expect existing outcome entries (if any) to appear in OUTCOME_INDEX queries. Silent data absence could confuse downstream col-002. | Low | Low | Document this limitation in the tool response or status output. |
| SR-05 | The required `type` tag creates a new enforcement pattern — no other category has required tags. This precedent may need to extend to other categories later, increasing maintenance surface. | Low | Med | Architect should design the validation as extensible (per-category rule sets), not outcome-specific hardcoding. |
| SR-06 | Unknown structured tag key rejection (tags with `:` where key is not recognized) could break agents that use ad-hoc colon-separated tags for non-outcome entries. Scope says validation is outcome-only, but the boundary must be clear. | High | Med | Architect must ensure structured tag validation ONLY fires when category == "outcome". Non-outcome entries must pass through unaffected regardless of tag format. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | context_status outcome statistics require scanning OUTCOME_INDEX and TAG_INDEX. If the scan is expensive at scale, it could slow status queries. | Low | Low | Consider lazy computation or caching for outcome stats. At expected scale (<1000 outcomes) this is negligible. |
| SR-08 | OUTCOME_INDEX population is inline (same write transaction as entry creation). If the index insert fails, it rolls back the entire entry store. This is intentional but must not introduce new failure modes for non-outcome entries. | Med | Low | Ensure OUTCOME_INDEX insert only happens when category == "outcome" AND feature_cycle is non-empty. Transaction isolation must be clean. |

## Assumptions

1. **No pre-existing outcome entries**: Scope assumes zero or negligible outcome entries stored before col-001 (Background Research section). If this is wrong, OUTCOME_INDEX will be incomplete without a migration scan.
2. **bincode backward compatibility**: The feature_cycle field already exists on EntryRecord with serde(default). No deserialization risk from adding OUTCOME_INDEX (it is a separate table, not a schema change).
3. **TAG_INDEX handles key:value tags as plain strings**: The existing tag intersection logic treats `gate:3a` as an opaque string match. No parsing needed at the lookup layer.
4. **Category allowlist already includes "outcome"**: Confirmed in categories.rs line 9. No allowlist change needed.

## Design Recommendations

- **SR-02, SR-06**: Isolate outcome tag validation into a dedicated module (e.g., `outcome_tags.rs`) with clear entry point: `validate_outcome_tags(tags) -> Result`. Call it from context_store ONLY when category == "outcome". Keep the store crate tag-agnostic.
- **SR-03**: Add integration test confirming existing StoreParams JSON without feature_cycle still deserializes correctly.
- **SR-05**: Design validation as a trait or rule-set pattern so future categories can add their own tag requirements without modifying outcome-specific code.
- **SR-08**: Add explicit test that non-outcome entry storage does NOT touch OUTCOME_INDEX, even when feature_cycle is populated.
