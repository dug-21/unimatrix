# col-001 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | OUTCOME_INDEX table exists with `(&str, u64) -> ()` schema | test | Unit test in db.rs: open table, insert and read a (str, u64) pair | PENDING |
| AC-02 | OUTCOME_INDEX created during Store::open (13 tables total) | test | Unit test in db.rs: open store, verify OUTCOME_INDEX accessible in read txn | PENDING |
| AC-03 | Structured tags follow `key:value` format with recognized keys | test | Unit tests in outcome_tags.rs: parse all 6 recognized keys | PENDING |
| AC-04 | context_store with category "outcome" rejects unknown key:value keys | test | Unit test: validate_outcome_tags with `["type:feature", "severity:high"]` returns error | PENDING |
| AC-05 | Tags without `:` pass through as plain tags | test | Unit test: validate_outcome_tags with `["type:feature", "important"]` succeeds | PENDING |
| AC-06 | `type` tag required for outcome entries | test | Unit test: validate_outcome_tags with `["gate:3a", "result:pass"]` (no type) returns error | PENDING |
| AC-07 | `type` values validated: feature, bugfix, incident, process | test | Unit tests: each valid value passes; `type:unknown` fails | PENDING |
| AC-08 | `result` values validated: pass, fail, rework, skip | test | Unit tests: each valid value passes; `result:maybe` fails | PENDING |
| AC-09 | `gate` accepts any non-empty string | test | Unit tests: `gate:3a`, `gate:custom`, `gate:1b` all pass; `gate:` (empty) fails | PENDING |
| AC-10 | Outcome with non-empty feature_cycle indexed in OUTCOME_INDEX | test | Integration test: store outcome with feature_cycle, read OUTCOME_INDEX, verify entry present | PENDING |
| AC-11 | Outcome with empty feature_cycle stored but NOT indexed | test | Integration test: store outcome without feature_cycle, verify OUTCOME_INDEX empty | PENDING |
| AC-12 | StoreParams includes feature_cycle: Option<String> | test | Unit test: deserialize StoreParams JSON with and without feature_cycle | PENDING |
| AC-13 | context_lookup with outcome tags returns matching entries | test | Integration test: store 3 outcomes with different tags, lookup with tag filter, verify correct subset returned | PENDING |
| AC-14 | context_status includes outcome stats | test | Integration test: store outcomes, call status, verify total_outcomes, outcomes_by_type, outcomes_by_result, outcomes_by_feature_cycle fields | PENDING |
| AC-15 | OUTCOME_INDEX population is transactional (same commit) | test | Integration test: verify entry and OUTCOME_INDEX row exist together after store | PENDING |
| AC-16 | Tag parsing/validation in server crate only | grep | `grep -r "OutcomeTagKey\|validate_outcome_tags" crates/unimatrix-store/` returns nothing | PENDING |
| AC-17 | Existing tests pass, no regressions | shell | `cargo test --workspace` passes with no failures | PENDING |
| AC-18 | Unit tests for tag parsing, validation, OUTCOME_INDEX | test | Test functions exist in outcome_tags.rs and db.rs covering parsing, acceptance, rejection | PENDING |
| AC-19 | Integration tests for store+lookup+status outcome flow | test | Server integration tests covering end-to-end outcome lifecycle | PENDING |
| AC-20 | `#![forbid(unsafe_code)]`, no new dependencies | grep | `grep "forbid(unsafe_code)" crates/*/src/lib.rs` confirms all crates; `diff Cargo.lock` shows no new deps | PENDING |
| AC-21 | Schema version remains 2 | test | Unit test: Store::open does not trigger migration; schema_version counter is 2 | PENDING |
