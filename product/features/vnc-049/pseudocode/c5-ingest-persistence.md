# C5 — Ingest attribution persistence (schema + write path)

**Location:** `unimatrix-store/src/db.rs` (CREATE), `unimatrix-store/src/migration.rs` (ALTER),
`unimatrix-server/src/uds/listener.rs` (`insert_observation:3383`, `insert_observations_batch:3416`).
**ADRs:** ADR-001 (persist provider-first, opencode-only, fail-loud), ADR-002 (model_id column), ADR-005
(over-cap thin wiring). **Risks:** R-01, R-02, R-04, R-05, R-15, R-17. **Wave:** 1 (schema) + 2 (write).

## Purpose

Persist `source_domain` and `model_id` at ingest so a stored OpenCode record reads back
`source_domain="opencode"` and a queryable `model_id`, instead of the read-derived `claude-code`
default. This is the AC-03/AC-06 mechanism — a domain pack alone cannot distinguish OpenCode.

## Part A — Schema (Wave 1)

### `db.rs` — `observations` CREATE
Existing cols: `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet,
topic_signal, phase, topic_source`. Add two nullable columns:
```
... topic_source TEXT,
source_domain TEXT,   -- vnc-049 ADR-001; NULL for non-opencode + legacy rows
model_id TEXT         -- vnc-049 ADR-002; NULL when no backend model / legacy
```

### `migration.rs` — additive ALTER + version bump
```
fn migrate_to_v<NEXT>(conn):           # NEXT = CURRENT_SCHEMA_VERSION + 1, resolved at delivery
    # Idempotency pre-check per pattern #1264 — do NOT blind-ALTER (may already exist on fresh CREATE)
    cols = pragma_table_info("observations")
    if "source_domain" not in cols:
        ALTER TABLE observations ADD COLUMN source_domain TEXT
    if "model_id" not in cols:
        ALTER TABLE observations ADD COLUMN model_id TEXT
```
- **Do NOT hardcode the version number** (OQ / R-04): resolve `NEXT` against `CURRENT_SCHEMA_VERSION`
  (`migration.rs:26`, 32 today) at delivery; a concurrent PR may advance it. Register the step in the
  migration dispatch table and bump `CURRENT_SCHEMA_VERSION`.
- Migration is version-independent and idempotent; re-run does not double-apply (R-04.4).
- **Cascade checklist (#4373/#4153):** (1) fresh CREATE has the cols; (2) ALTER migrate yields the same
  cols; (3) existing migration-test assertions updated for the new version + column count; (4) idempotent.

## Part B — Write path (Wave 2)

### Write-path `ObservationRow` struct (see OVERVIEW OQ-A)
The struct bound at `listener.rs:3383/3416` gains:
```
source_domain: Option<String>,
model_id: Option<String>,
```
Populated where the row is constructed from `ImplantEvent` (in listener.rs). Derivation:
```
fn derive_source_domain(event: &ImplantEvent) -> Option<String>:
    match event.provider.as_deref():
        Some("opencode") => Some("opencode")     # ADR-001 opencode-only stamp
        _                => None                  # claude-code/gemini/codex/None → NULL → read-derived (R-05)

fn resolve_model_id(event: &ImplantEvent) -> Option<String>:
    match &event.model_id:
        Some(m) if is_valid_model_id(m) => Some(m.clone())   # C4 charset (R-15)
        Some(_bad)                      => { tracing::warn!("invalid model_id dropped"); None }
        None                            => None
```

**Fail-loud canary (ADR-001 §4, R-02.4):** mirror the vnc-013 provider canary —
```
debug_assert!(!(event.provider.as_deref()==Some("opencode") && derived_source_domain.is_none()),
              "opencode event reached write path with no source_domain");
```
An opencode event whose stamp cannot resolve is an error/canary, NEVER a silent fall to `claude-code`.

### `insert_observation` (`:3383`) and `insert_observations_batch` (`:3416`)
Extend both INSERTs identically (over-cap: minimal wiring only, ADR-005):
```
INSERT INTO observations
  (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
   topic_signal, phase, topic_source, source_domain, model_id)
VALUES (?1..?10, ?11, ?12)
  ... .bind(&obs.source_domain)   # ?11
      .bind(&obs.model_id)        # ?12
```
- Both sites MUST change together (batch + single) — a missed site drops the carrier (R-01 wire→INSERT drop).
- Parameterized bind only, never string interpolation (existing ADR-005 convention at these sites).

## Data flow

IN: `ImplantEvent { provider, model_id }` (C4) → construct `ObservationRow` with derived
`source_domain` + validated `model_id` → bound at INSERT. OUT: stored row carries both columns → C6 read.

## Error handling

- Invalid `model_id` → drop to NULL + warn (fail-open, R-15).
- opencode + no derivable source_domain → `debug_assert`/error canary (fail-loud, R-02.4).
- DB errors: existing `StoreError::Database` mapping unchanged.

## Key test scenarios (hints for tester)

1. **Stored source_domain** (R-02.1/AC-03): ingest a real opencode event through the assembled path;
   SELECT the row → `source_domain == "opencode"`. **Negative:** NOT `"claude-code"` (R-02.2).
2. **opencode-only stamp** (R-05.1): gemini-cli/codex-cli/claude-code event → `source_domain` NULL at write.
3. **model_id persisted** (R-01): opencode local-model event → stored `model_id == "ollama/qwen3-coder"`;
   dropped-carrier negative assertion (field homogenized/NULLed → test fails).
4. **Fresh vs migrated column parity** (R-04.1): both CREATE and ALTER paths yield both columns.
5. **Migration idempotence** (R-04.4): re-run migration → no error, no double-apply.
6. **Both insert sites** (R-01): single-insert and batch-insert both persist the columns.
7. **Fail-loud** (R-02.4): opencode event with unresolvable stamp trips the canary, does not store claude-code.
8. **charset reject** (R-15): oversized/illegal `model_id` stored as NULL, not raw.
</content>
