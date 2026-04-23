## ADR-005: Append-Only Trigger Remediation — GC, Import, and Test Infrastructure

### Context

The ASS-050 migration installs `BEFORE UPDATE` and `BEFORE DELETE` triggers on `audit_log` that
raise `ABORT`. This is a permanent structural change. Two production code paths and test
infrastructure are directly broken by it (SR-07 — severity High/High).

**Identified DELETE sites** (confirmed by codebase search):

1. `crates/unimatrix-store/src/retention.rs` — `gc_audit_log()`:
   `DELETE FROM audit_log WHERE timestamp < (strftime('%s', 'now') - ?1 * 86400)`
   This is the audit log GC path called by the background tick.

2. `crates/unimatrix-server/src/import/mod.rs` — `drop_all_data()`:
   `DELETE FROM audit_log;`
   This is called during full import (database restore/overwrite).

**Test infrastructure**:
No test file uses `DELETE FROM audit_log` directly (confirmed by codebase search — only the two
production sites above exist). Tests each use fresh `TempDir` databases, so existing test
isolation is not broken by the trigger. However, two production scenarios are broken.

**Options for each site**:

GC path (`gc_audit_log`):
- (A) Remove `gc_audit_log` entirely — audit logs are compliance evidence; retention-based
  deletion is incompatible with append-only. The GC was a space management mechanism; with
  append-only triggers, it can no longer function. Audit log size is bounded by the operational
  lifetime of the deployment. OSS deployments are single-machine; size is acceptable.
- (B) Replace with an archival path (move old rows to an archive table) — over-engineering for
  vnc-014 scope; archive tooling is not in scope.
- (C) Drop the trigger scope to only UPDATES (allow DELETE) — violates the ASS-050 append-only
  requirement. Rejected.

Option (A) is the correct choice for vnc-014: `gc_audit_log` is removed and its call site in
the background tick is removed. A comment documents that audit log pruning is not supported
under the append-only model and requires a future archival mechanism if needed.

Import path (`drop_all_data`):
- (A) Remove the `audit_log` line from `drop_all_data()` — imports do not restore audit history.
  The audit log is append-only and accumulates across imports. Historical audit records remain.
  This is the correct semantics for an append-only compliance log.
- (B) Drop and recreate the table during import — bypasses triggers but requires DDL in the
  import path, complicating the transaction model.

Option (A) for import: remove `DELETE FROM audit_log;` from `drop_all_data()`. The import
operation simply does not clear the audit log. This is semantically correct — a restore operation
should not destroy audit history.

**Tests**:
No test uses `DELETE FROM audit_log` — test isolation is via fresh `TempDir` databases. No
test infrastructure change needed for the trigger installation. Tests that assert on audit_log
row counts need no change because they insert into fresh DBs.

The `AuditEvent` struct gains `#[serde(default)]` on all four new fields. This ensures any
existing serialized `AuditEvent` (in tests or in-flight via serde) deserializes without error
when the new fields are absent from the serialized form.

### Decision

1. Remove `gc_audit_log()` from `retention.rs` and its call site in the background tick.
   Add a comment explaining append-only semantics prevent time-based deletion.

2. Remove `DELETE FROM audit_log;` from `drop_all_data()` in `import/mod.rs`.
   Add a comment explaining audit log is append-only and not cleared on import.

3. `AuditEvent` struct in `schema.rs` gains four new fields with `#[serde(default)]`:
   ```rust
   #[serde(default)]
   pub credential_type: String,    // default "" (serde); "none" in SQL DEFAULT
   #[serde(default)]
   pub capability_used: String,
   #[serde(default)]
   pub agent_attribution: String,
   #[serde(default)]
   pub metadata: String,           // default "" (serde); "{}" in SQL DEFAULT
   ```
   Note: `serde(default)` gives `String::default()` = `""`. The SQL column defaults differ
   (`'none'` and `'{}'`). Construction sites in code must supply the correct values explicitly
   (not rely on `Default::default()`).

4. Add `impl Default for AuditEvent` so construction sites can use struct update syntax
   (`..AuditEvent::default()`) for the four new fields at sites that do not have `client_type`
   context (non-tool-call paths: background, UDS listener). The `Default` implementation sets:
   - `credential_type: "none".to_string()`
   - `capability_used: String::new()`
   - `agent_attribution: String::new()`
   - `metadata: "{}".to_string()`

5. No changes to test infrastructure — fresh TempDir databases are unaffected by triggers.

### Consequences

Easier:
- No test infrastructure changes needed
- Production code reduction (GC path removed)
- Audit log semantics are now unambiguously append-only

Harder:
- Audit log size grows indefinitely in long-running deployments — no GC path available
  (documented as a known limitation; archival is future work)
- Import no longer clears audit history — semantically correct but may surprise operators
  expecting a clean restore (document in import tool help)
- The `Default` impl on `AuditEvent` must be maintained as fields are added in future features
