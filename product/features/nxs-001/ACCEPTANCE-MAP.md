# nxs-001: Embedded Storage Engine — Acceptance Map

**Date**: 2026-02-22

Maps each acceptance criterion to implementing component(s), test type, risk mitigation, and verification method.

---

## AC-01: Cargo Workspace Compiles

| Attribute | Value |
|-----------|-------|
| Component | C1 (crate-setup) |
| Test type | Build verification |
| Risks mitigated | None (structural) |
| Verification | `cargo build --workspace` succeeds with zero errors and zero warnings. `Cargo.toml` at repo root defines `[workspace]` with `unimatrix-store` as member. Crate is `edition = "2024"`. |

## AC-02: EntryRecord Round-Trip Serialization

| Attribute | Value |
|-----------|-------|
| Component | C2 (schema) |
| Test type | Unit |
| Risks mitigated | R3 (Bincode Serialization Round-Trip Fidelity) |
| Verification | Unit test creates `EntryRecord` with all fields populated, serializes to bytes via `bincode::serde::encode_to_vec`, deserializes back via `bincode::serde::decode_from_slice`, asserts equality. Edge cases: empty strings, empty tags vec, `Option::None` fields, max `u64` timestamps, `f32` edge values (0.0, 1.0, MIN_POSITIVE), all three `Status` variants, unicode content, large content (100KB). |

## AC-03: All 8 Tables Created on Open

| Attribute | Value |
|-----------|-------|
| Component | C4 (store) |
| Test type | Integration |
| Risks mitigated | R10 (Database Lifecycle) |
| Verification | Integration test opens a new database via `Store::open()`, then verifies all 8 tables exist by opening each in a read transaction. Confirms ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX (multimap), TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS. |

## AC-04: Atomic Multi-Table Insert

| Attribute | Value |
|-----------|-------|
| Component | C6 (write) |
| Test type | Integration |
| Risks mitigated | R1 (Index-Entry Desync), R6 (Transaction Atomicity) |
| Verification | (1) Insert an entry, then read from ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX to verify presence in all tables. (2) Simulate partial failure by dropping a write transaction before commit — verify no tables were modified. |

## AC-05: Monotonic Entry ID Generation

| Attribute | Value |
|-----------|-------|
| Component | C5 (counter), C6 (write) |
| Test type | Integration |
| Risks mitigated | R5 (Monotonic ID Generation Correctness) |
| Verification | Insert 100 entries sequentially, assert each returned ID is strictly greater than the previous. First insert returns ID `1`. Read `"next_entry_id"` counter after all inserts, assert it equals `last_assigned_id + 1`. |

## AC-06: Point Lookup by Entry ID

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R1 (Index-Entry Desync) |
| Verification | Insert an entry, retrieve by ID via `Store::get()`, assert all fields match inserted values. Also assert that looking up a non-existent ID returns `StoreError::EntryNotFound`. |

## AC-07: Topic Index Query

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R1 (Index-Entry Desync), R2 (Update Path Orphaning) |
| Verification | Insert 5 entries across 3 topics. Query for topic `"auth"` via `Store::query_by_topic()`, assert exactly the entries with that topic are returned (correct count, correct IDs). Query non-existent topic returns empty vec. |

## AC-08: Category Index Query

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R1 (Index-Entry Desync), R2 (Update Path Orphaning) |
| Verification | Insert entries across multiple categories. Query one category via `Store::query_by_category()`, assert correct set returned. Same pattern as AC-07 but on CATEGORY_INDEX. |

## AC-09: Tag Intersection Query

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R9 (Tag Index Set Operations) |
| Verification | Insert entries with overlapping tag sets. Query for `["rust", "error"]` via `Store::query_by_tags()`, assert only entries with BOTH tags returned. Edge cases: single tag, non-existent tag returns empty vec, 3-tag intersection where only 1 entry matches all three. |

## AC-10: Time Range Query

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R1 (Index-Entry Desync) |
| Verification | Insert entries with timestamps at 1000, 2000, 3000, 4000, 5000. Query range `2000..=4000` via `Store::query_by_time_range()`, assert exactly 3 entries returned. Edge cases: empty range, single-point range (`start == end`), inverted range returns empty. |

## AC-11: Status Index Query

| Attribute | Value |
|-----------|-------|
| Component | C7 (read) |
| Test type | Integration |
| Risks mitigated | R8 (Status Transition Atomicity) |
| Verification | Insert entries with different statuses (Active, Deprecated, Proposed). Query for `Status::Active` via `Store::query_by_status()`, assert only active entries returned. |

## AC-12: Atomic Status Update with Index Migration

| Attribute | Value |
|-----------|-------|
| Component | C6 (write) |
| Test type | Integration |
| Risks mitigated | R8 (Status Transition Atomicity) |
| Verification | Insert entry with `Status::Active`, call `Store::update_status(id, Status::Deprecated)`. Assert: STATUS_INDEX no longer contains `(0, id)`, STATUS_INDEX contains `(1, id)`, ENTRIES record shows `status == Deprecated`, `"total_active"` counter decremented, `"total_deprecated"` counter incremented. Additional transitions: Proposed->Active, Deprecated->Active (reactivation). |

## AC-13: VECTOR_MAP Insert and Lookup

| Attribute | Value |
|-----------|-------|
| Component | C6 (write), C7 (read) |
| Test type | Integration |
| Risks mitigated | R11 (VECTOR_MAP Bridge Table) |
| Verification | Write `(entry_id=42, hnsw_data_id=7)` via `Store::put_vector_mapping()`. Read back via `Store::get_vector_mapping()`, assert `7`. Overwrite with `hnsw_data_id=99`, read back, assert `99`. Lookup non-existent entry returns `None`. Boundary: `u64::MAX` round-trips correctly. |

## AC-14: Database Lifecycle (Open/Create/Cache/Compact)

| Attribute | Value |
|-----------|-------|
| Component | C4 (store) |
| Test type | Integration |
| Risks mitigated | R10 (Database Lifecycle) |
| Verification | (1) `Store::open()` creates new file — assert file exists on disk. (2) Close and reopen — assert previously inserted entries still present. (3) `Store::open_with_config()` with 128 MiB cache — no error. (4) `Store::compact()` — no error, file size does not increase. |

## AC-15: Typed Result Errors (No Panics)

| Attribute | Value |
|-----------|-------|
| Component | C3 (error), C6 (write), C7 (read) |
| Test type | Unit + Integration |
| Risks mitigated | R12 (Error Type Discrimination) |
| Verification | Unit tests verify each `StoreError` variant is constructible and displays meaningful messages. Integration tests: `Store::get(nonexistent_id)` returns `EntryNotFound`, corrupt bytes cause `Deserialization` error. `#![forbid(unsafe_code)]` at crate level verified by compilation. |

## AC-16: Schema Evolution via serde(default)

| Attribute | Value |
|-----------|-------|
| Component | C2 (schema) |
| Test type | Unit |
| Risks mitigated | R4 (Schema Evolution) |
| Verification | Serialize a reduced struct (simulating "v1" without `#[serde(default)]` fields), deserialize as full `EntryRecord`. New fields must default to their `serde(default)` values (`0`, `None`, `0.0`). Serialize full record, add hypothetical new `#[serde(default)]` field, deserialize old bytes — existing fields preserved, new field gets default. **This test must be written FIRST (W1 alignment warning).** |

## AC-17: QueryFilter Combined Query

| Attribute | Value |
|-----------|-------|
| Component | C8 (query) |
| Test type | Integration + Property |
| Risks mitigated | R7 (QueryFilter Intersection Correctness) |
| Verification | (1) Insert entries varying in topic, category, tags, status, time. Apply `QueryFilter` with `topic="auth"` + `status=Active` + `tags=["jwt"]`, assert only intersection returned. (2) Empty `QueryFilter` returns all active entries. (3) Disjoint filter returns empty vec. (4) Single-field filters. (5) Property tests: random entry sets + random filters, verify results match brute-force filter. |

## AC-18: Atomic Update with Index Migration

| Attribute | Value |
|-----------|-------|
| Component | C6 (write) |
| Test type | Integration |
| Risks mitigated | R2 (Update Path Stale Index Orphaning) |
| Verification | (1) Insert entry with `topic="auth"`, update to `topic="security"`. Assert TOPIC_INDEX scan for `"auth"` returns empty, scan for `"security"` returns the entry. (2) Same pattern for category change. (3) Tag add/remove: verify TAG_INDEX reflects exact new tag set. (4) Multi-field simultaneous change (topic + category + tags): all stale entries removed, all new entries inserted. (5) No-change update: all indexes unchanged. |

## AC-19: Reusable Test Infrastructure

| Attribute | Value |
|-----------|-------|
| Component | C9 (test-infra) |
| Test type | Structural |
| Risks mitigated | None (quality infrastructure) |
| Verification | Test helpers exist providing: `TestDb` struct (temp dir + database, `Drop` cleanup), `TestEntry` builder with factory functions, `assert_index_consistent()` and `assert_index_absent()` helpers. Accessible within crate via `#[cfg(test)]` and to downstream crates via `test-support` feature flag. |

---

## Summary Matrix

| AC | Component(s) | Test Type | Risk(s) |
|----|-------------|-----------|---------|
| AC-01 | C1 | Build | — |
| AC-02 | C2 | Unit | R3 |
| AC-03 | C4 | Integration | R10 |
| AC-04 | C6 | Integration | R1, R6 |
| AC-05 | C5, C6 | Integration | R5 |
| AC-06 | C7 | Integration | R1 |
| AC-07 | C7 | Integration | R1, R2 |
| AC-08 | C7 | Integration | R1, R2 |
| AC-09 | C7 | Integration | R9 |
| AC-10 | C7 | Integration | R1 |
| AC-11 | C7 | Integration | R8 |
| AC-12 | C6 | Integration | R8 |
| AC-13 | C6, C7 | Integration | R11 |
| AC-14 | C4 | Integration | R10 |
| AC-15 | C3, C6, C7 | Unit + Integration | R12 |
| AC-16 | C2 | Unit | R4 |
| AC-17 | C8 | Integration + Property | R7 |
| AC-18 | C6 | Integration | R2 |
| AC-19 | C9 | Structural | — |
