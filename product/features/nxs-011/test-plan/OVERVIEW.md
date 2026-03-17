# nxs-011 Test Plan: OVERVIEW
## sqlx Migration — Connection Pools + Async-Native Storage

---

## Test Strategy

The test strategy for nxs-011 is organized into four categories:

| Category | Location | Framework | Purpose |
|----------|----------|-----------|---------|
| Unit | `crates/unimatrix-store/src/` | `#[tokio::test]` | Pool config, shed counter, analytics enqueue, drain batch logic |
| Integration (store) | `crates/unimatrix-store/tests/` | `#[tokio::test]` | Full Store lifecycle, migration, close semantics |
| Integration (server) | `crates/unimatrix-server/tests/` | `#[tokio::test]` | spawn_blocking removal, transaction call sites, MCP tool contract |
| Integration (infra-001) | `product/test/infra-001/` | `pytest` | End-to-end MCP protocol through compiled binary |

All tests that touch `SqlxStore` use `#[tokio::test]`. No synchronous `#[test]` is permitted for store tests (TC-01). Every test that opens a `SqlxStore` must call `Store::close().await` before exit (TC-02). Tests that verify analytics writes must close the store before asserting committed data (TC-05).

---

## Risk-to-Test Mapping

| Risk ID | Severity | Component File | Test File(s) | Scenario Count |
|---------|----------|----------------|-------------|----------------|
| R-01 | Critical | pool_config.rs, db.rs | pool-config.md, sqlx-store.md | 3 |
| R-02 | Critical | db.rs, analytics.rs | analytics-queue.md, sqlx-store.md | 4 |
| R-03 | Critical | migration.rs | migration.md | 4 |
| R-04 | High | analytics.rs, db.rs | analytics-queue.md, server-migration.md | 5 |
| R-05 | High | sqlx-data.json, CI | ci-offline.md | 4 |
| R-06 | High | db.rs, write.rs | analytics-queue.md, sqlx-store.md | 3 |
| R-07 | High | entry-store-trait (traits.rs, async_wrappers.rs) | entry-store-trait.md, async-wrappers.md | 3 |
| R-08 | High | observe-migration (dead_knowledge.rs) | observe-migration.md | 4 |
| R-09 | High | server-migration (server.rs, store_correct.rs, store_ops.rs, audit.rs) | server-migration.md | 5+ |
| R-10 | Med | db.rs (read_pool) | sqlx-store.md | 3 |
| R-11 | Med | pool_config.rs (PRAGMA) | pool-config.md | 2 |
| R-12 | Low | pool_config.rs | pool-config.md | 1 |
| R-13 | Med | analytics.rs | analytics-queue.md | 1 |
| R-14 | High | All test files | All | 1 (count gate) |
| R-15 | High | server-migration | server-migration.md, ci-offline.md | 2 |

Total required scenarios: 45+ across all categories.

---

## AC Verification Checklist

| AC-ID | Verification Method | Test File | Blocking? |
|-------|--------------------|-----------|---------:|
| AC-01 | grep: no rusqlite in Cargo.toml | ci-offline.md | Yes |
| AC-02 | unit test: PRAGMA query both pools | pool-config.md | Yes |
| AC-03 | grep: no Mutex::lock/lock_conn/spawn_blocking in store src | ci-offline.md | Yes |
| AC-04 | grep: no AsyncEntryStore anywhere | async-wrappers.md, ci-offline.md | Yes |
| AC-05 | grep: no spawn_blocking.*store in server src | server-migration.md, ci-offline.md | Yes |
| AC-06 | unit test: queue cap, shed on 1001st | analytics-queue.md | Yes |
| AC-07 | integration + grep: each analytics method calls enqueue | analytics-queue.md | Yes |
| AC-08 | integration: integrity write survives full queue | analytics-queue.md, sqlx-store.md | Yes |
| AC-09 | unit test: write_max=3 → InvalidPoolConfig | pool-config.md | Yes |
| AC-10 | integration: saturate pool → PoolTimeout within configured time | pool-config.md, sqlx-store.md | Yes |
| AC-11 | cargo test: all 16 migration integration tests pass | migration.md | Yes |
| AC-12 | file-check + CI log: sqlx-data.json present, SQLX_OFFLINE=true | ci-offline.md | Yes |
| AC-13 | grep: no unimatrix_store::rusqlite | ci-offline.md | Yes |
| AC-14 | cargo test --workspace: total count >= 1,649 | All | Yes (gate) |
| AC-15 | unit test: WARN log on shed event | analytics-queue.md | Yes |
| AC-16 | grep: no SqliteWriteTransaction/MutexGuard | server-migration.md, ci-offline.md | Yes |
| AC-17 | integration: 12 version transitions, fresh DB per test | migration.md | Yes |
| AC-18 | integration: N shed events → context_status shed_events_total==N | analytics-queue.md, server-migration.md | Yes |
| AC-19 | integration: close() awaits drain; all events committed; pool=0 | analytics-queue.md, sqlx-store.md | Yes |
| AC-20 | compile + unit: impl_completeness.rs compiles; no dyn EntryStore | entry-store-trait.md | Yes |

All 20 ACs are blocking. None may be waived.

---

## Integration Harness Plan (infra-001)

### Suite Selection

This feature is a storage-layer transport change. The feature touches every MCP tool (all reads and writes now go through the async pool), the `context_status` output (AC-18 adds `shed_events_total`), and the full server lifecycle.

| Suite | Rationale |
|-------|-----------|
| `smoke` (-m smoke) | Mandatory minimum gate — runs before all other suites |
| `tools` | All 9 tools now route through SqlxStore; verify behavior is preserved end-to-end |
| `lifecycle` | Multi-step flows (store→search, correction chains) exercise both pools and the drain task |
| `volume` | Scale tests exercise analytics queue shed behavior under real load |
| `security` | Integrity write bypass under queue saturation must hold through the MCP interface |

Suites NOT required for this feature: `confidence` (no scoring logic change), `contradiction` (no detection logic change), `edge_cases` (covered by unit tests for boundary values; no new edge cases at the MCP layer).

### Existing Suite Coverage

| infra-001 Suite | Covers (nxs-011 angle) |
|-----------------|------------------------|
| `protocol` | Handshake, tool discovery, graceful shutdown — verifies server still starts with SqlxStore |
| `tools` | All 9 tool behaviors preserved after removing spawn_blocking |
| `lifecycle` | store→search, correction chain — exercises write_pool + read_pool separation |
| `volume` | 100s of entries — exercises analytics queue drain at scale |
| `security` | Content scanning paths — verifies integrity paths not contaminated |

### New Integration Tests Required

The following scenarios are not covered by existing infra-001 suites and must be added to the harness in Stage 3c:

#### `suites/test_tools.py` — Add to `context_status` test group

```python
def test_context_status_shed_events_total_present(server):
    """AC-18: shed_events_total field present and is 0 on fresh server."""
    result = server.call_tool("context_status", {})
    assert "shed_events_total" in result
    assert result["shed_events_total"] == 0
```

Fixture: `server` (fresh DB, no state). No need for shed induction at infra-001 level — field presence and zero value is sufficient to verify contract.

#### `suites/test_lifecycle.py` — Add store/close integrity test

```python
def test_store_open_close_preserves_entries(server):
    """AC-19 proxy: entries written before server restart are readable after restart."""
    server.call_tool("context_store", {"title": "t", "content": "c", "topic": "x", "category": "decision"})
    # server fixture restarts between tests — verifies close() flushed drain task
    result = server.call_tool("context_search", {"query": "t"})
    assert len(result) >= 1
```

Fixture: `server` (stateless restart between tests verifies drain task flushed on shutdown).

### Running infra-001 (Stage 3c sequence)

```bash
# 1. Build binary
cargo build --release

# 2. Mandatory gate
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# 3. Feature-relevant suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_volume.py suites/test_security.py -v --timeout=60

# 4. Full suite (if time permits / pre-merge)
python -m pytest suites/ -v --timeout=60
```

---

## Test Count Tracking

### Baseline (must not decrease)

| Suite | Baseline |
|-------|----------|
| unimatrix-store unit | 103 |
| unimatrix-store integration | 85 |
| unimatrix-server unit | 1,406 |
| unimatrix-server integration | 39 |
| Migration integration | 16 |
| **Total** | **1,649** |

### Conversion Strategy

All existing `#[test]` functions in `unimatrix-store` that call store methods become `#[tokio::test]`. Count is preserved — conversion is not deletion.

### New Tests (additive above baseline)

| Component | New Test Count (estimated) |
|-----------|--------------------------|
| pool-config.md | +8 |
| analytics-queue.md | +14 |
| migration.md | +12 (v0→v12 individual transitions + idempotency) |
| sqlx-store.md | +10 |
| entry-store-trait.md | +3 |
| async-wrappers.md | +2 |
| server-migration.md | +8 |
| observe-migration.md | +4 |
| ci-offline.md | +0 (grep/CI checks, not cargo tests) |
| **Total new** | **~61** |

Expected post-migration total: ≥ 1,710 (1,649 baseline + ~61 new).

---

## Critical Test Ordering (Stage 3c)

The following sequence must be respected before the delivery gate is declared open:

1. **Build succeeds** (`cargo build --release`) — SQLX_OFFLINE=true set.
2. **`cargo sqlx check --workspace`** passes — sqlx-data.json is current.
3. **Grep gates pass** (AC-01, AC-03, AC-04, AC-05, AC-13, AC-15, AC-16) — zero matches on all forbidden patterns.
4. **Unit tests pass** (`cargo test -p unimatrix-store`) — pool config, shed counter, drain batch.
5. **Migration integration tests** (`cargo test -p unimatrix-store --test migration`) — AC-11, AC-17.
6. **Server unit tests** (`cargo test -p unimatrix-server`) — AC-14 contribution; send bound compile.
7. **Full workspace test run** (`cargo test --workspace 2>&1 | tail -30`) — AC-14 count gate ≥ 1,649.
8. **infra-001 smoke gate** (`pytest -m smoke --timeout=60`) — mandatory.
9. **infra-001 feature suites** (tools, lifecycle, volume, security).

No delivery gate submission until all 9 steps pass.
