## ADR-005: outcome Category Retirement — Ingest Block Only, No Data Deletion

### Context

The product vision states: "Outcomes belong to CYCLE_EVENTS (workflow layer), not the knowledge base." The `outcome` category in the ENTRIES table stores workflow-level lifecycle conclusions as knowledge entries, conflating two distinct concerns:

- **Knowledge entries**: reusable patterns, decisions, conventions — intended to be retrieved and injected into future sessions.
- **Outcome records**: point-in-time conclusions about a specific feature cycle — not reusable knowledge, not useful for retrieval injection.

With `CYCLE_EVENTS` now providing structured, queryable outcome data per phase transition, the `outcome` category in ENTRIES is redundant and semantically incorrect.

SR-03 notes that removing `outcome` from `CategoryAllowlist` is a silent ingest breakage for callers currently using this category, and that existing tests assert `al.validate("outcome").is_ok()`.

Two options:

**Option A: Remove outcome from INITIAL_CATEGORIES + delete existing outcome entries.**
Cleanest semantic state: the knowledge base contains no outcome entries. Rejected: data deletion in a migration is irreversible and the existing outcome entries may have co-access edges, embeddings, and confidence scores that form part of the graph. Deleting them would corrupt graph edges without cascading cleanup. This is also out of scope per SCOPE §Goals item 7 (which says "retire the outcome category", not "delete existing entries"). GH #338 tracks follow-up cleanup.

**Option B: Remove outcome from INITIAL_CATEGORIES only (block new ingest).**
Existing entries remain queryable and injectable. New stores with category `outcome` return a category-rejected error. This is a forward-looking gate, not a retroactive deletion.

An audit of active call sites confirms: no production code path in the server, hook path, or protocols emits `context_store` calls with `category: "outcome"`. The `context_cycle_review` handler auto-persists `lesson-learned` entries (not `outcome` entries). The `col-001` outcome tracking system uses `OUTCOME_INDEX` (a separate table), not ENTRIES with category `outcome`.

The test suite has `test_validate_outcome` and related tests asserting `outcome` is valid. These must be updated to assert `outcome` is rejected.

### Decision

**Option B**: Remove `"outcome"` from `INITIAL_CATEGORIES` in `categories.rs`. This blocks new ingest via `context_store`. Existing entries with category `"outcome"` are not touched — they remain in the database, remain injectable, and are not migrated.

No SQL DELETE is emitted in the v14 → v15 migration. The migration is limited to structural schema changes (new table, new column).

Affected tests in `categories.rs` (at minimum):
- `test_validate_outcome` — assert changes from `is_ok()` to `is_err()`.
- `test_new_allows_outcome_and_decision` — `outcome` assertion changes to `is_err()`.
- `test_poison_recovery_validate` line that calls `al.validate("outcome").is_ok()`.
- `test_list_categories_sorted` — count changes from 8 to 7.

### Consequences

**Easier**:
- New knowledge stored after WA-1 ships cannot be miscategorized as `outcome`.
- `CYCLE_EVENTS` is the canonical location for all outcome-type data going forward.
- Migration is non-destructive; no rollback risk from data loss.

**Harder**:
- Existing `outcome` entries remain in the knowledge base with no category label that reflects their current semantics. Cleanup is deferred to GH #338.
- Any external caller (non-production tooling, manual `context_store` calls) that uses category `outcome` will now receive an error with no migration path offered. The error message from `CategoryAllowlist` will list the valid categories, making the failure self-explanatory.
