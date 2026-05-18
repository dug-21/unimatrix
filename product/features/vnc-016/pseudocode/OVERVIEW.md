# vnc-016 Pseudocode Overview

## Purpose

Fix a confirmed SQLite column-name bug in `query_stale_prerequisite_edges_for_cycle`, fix a
production trust-level gate bug in `UsageContext` that silently dropped `feature_entries` writes
for Restricted-trust agents with Write capability, and close the AC-12 gap from vnc-015 by
delivering end-to-end integration tests for the `DependencyOnDeprecatedRule` detection path.

## Components Involved

| Component | File | Action |
|-----------|------|--------|
| SQL Fix | `crates/unimatrix-store/src/read.rs:1618` | One-token WHERE clause fix |
| Rust Unit Test | `crates/unimatrix-store/src/read.rs mod tests` | Two `#[tokio::test]` functions |
| Harness Client Extension | `product/test/infra-001/harness/client.py` | New keyword arg on `context_store` |
| Usage Gate Fix | `crates/unimatrix-server/src/services/usage.rs` + `mcp/tools.rs` | New `write_capable` field + gate replacement |
| Integration Tests | `product/test/infra-001/suites/test_tools.py` | Two pytest functions after vnc-015 section |

No new files are created. All changes extend existing files.

## Data Flow

```
test_tools.py
    |
    |-- context_enroll(test_agent, restricted+write)   [Admin: human]
    |
    |-- context_store(content, feature_cycle=cycle_id, agent_id=test_agent)
    |       tools.rs: require_cap(Write) passes
    |       tools.rs: UsageContext { write_capable: true, feature_cycle: Some(cycle_id) }
    |       usage.rs: feature_recording = Some((cycle_id, [id_a]))   [GATE FIXED]
    |       write_ext.rs: INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase)
    |               feature_id = cycle_id   [column name is "feature_id", NOT "feature_cycle"]
    |
    |-- context_edge("add", id_a, "Prerequisite", id_b)
    |       graph_edges: (source_id=id_a, target_id=id_b, relation_type='Prerequisite')
    |
    |-- context_correct(id_a, ...)
    |       entries.status = 1 for id_a   [Deprecated]
    |
    |-- _seed_observation_sql(db_path, [cycle_id], num_records=20)
    |       sessions + observations tables populated
    |
    |-- context_cycle_review(cycle_id, force=True, format="json")
            tools.rs: query_stale_prerequisite_edges_for_cycle(cycle_id)
                    read.rs: WHERE fe.feature_id = ?1   [SQL FIXED]
                    returns: [(id_a, id_b)]
            detect_hotspots(history, stale_edge_pairs=[(id_a, id_b)])
            DependencyOnDeprecatedRule fires
            JSON response: { "hotspots": [{ "rule_name": "dependency_on_deprecated", ... }] }
```

## Shared Types

### `UsageContext` (modified in `usage.rs`)

New field added — no `Default`, no `#[serde(default)]`:

```
struct UsageContext {
    session_id:    Option<String>,
    agent_id:      Option<String>,
    helpful:       Option<bool>,
    feature_cycle: Option<String>,
    trust_level:   Option<TrustLevel>,   // retained; no longer used in feature_recording gate
    access_weight: u32,
    current_phase: Option<String>,
    write_capable: bool,                  // NEW — true only at context_store callsite
}
```

### `feature_entries` table (authoritative, `db.rs:616-621`)

```
feature_entries (
    feature_id  TEXT    NOT NULL,   -- the cycle string; NOT "feature_cycle"
    entry_id    INTEGER NOT NULL,
    phase       TEXT                -- nullable
)
```

The column is named `feature_id`. Application code uses the variable name `feature_cycle` for
the value. This distinction is the root cause of the SQL bug.

## Wave Dependencies

All five components are independent of each other except that:
1. The SQL Fix (Component 1) must be complete before the Rust Unit Test (Component 2) can
   produce a green result, because the positive path test asserts the fixed query works.
2. The Harness Client Extension (Component 3) must be in place before the Integration Tests
   (Component 5) can call `context_store` with `feature_cycle`.
3. The Usage Gate Fix (Component 4) must be complete before the Integration Tests (Component 5)
   produce a meaningful signal — the test specifically uses a Restricted+Write agent to validate
   the gate.

Single-wave delivery is viable: all components are small, isolated changes. Parallel
implementation is safe because no component modifies a shared interface; they all touch separate
files (except `usage.rs` and `tools.rs` which are coordinated in Component 4).

## Sequencing Constraints

Build/test order:
1. Apply SQL Fix first — cargo test -p unimatrix-store will fail on the positive Rust unit test
   until the fix is in place.
2. Apply Usage Gate Fix second — adds a required struct field; the Rust compiler will reject all
   `UsageContext { ... }` literals in tools.rs that omit `write_capable` until every callsite
   is updated.
3. Harness client and integration test changes are Python-only and have no ordering constraint
   relative to each other, but both require the Usage Gate Fix to be in place for the positive
   integration test to pass.
