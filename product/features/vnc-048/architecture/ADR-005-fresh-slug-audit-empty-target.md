## ADR-005: Non-empty-`audit_log` pre-flight refusal; the supported restore target is a freshly-registered slug

### Context
`insert_audit_log` (`import/inserters.rs:157-177`) is a plain `INSERT` with an explicit `event_id`, and `drop_all_data` **cannot** clear `audit_log` — append-only `BEFORE UPDATE/DELETE` triggers reject the DELETE (vnc-014 / schema v25; audit history is preserved across import resets per ADR-005 vnc-014, #4359). So restoring into a slug store that has accumulated audit rows hits a UNIQUE collision on `event_id` — even with `--force`, which only clears the importable tables, not `audit_log` (SR-05, C-5). Left raw, the operator sees an opaque SQLite UNIQUE error. The restore is only well-defined against a store whose `audit_log` is empty.

Verified (SCOPE Background Research): a freshly-registered slug store has a **provably zero-row** `audit_log`. `project register` → `Store::open` runs `create_tables_if_needed`, which creates `audit_log` empty and installs only the append-only triggers (neither inserts a row); its only INSERTs are `INSERT OR IGNORE INTO counters`. No "project created" audit event exists. So the supported target passes pre-flight, and the explicit-`event_id` import INSERT cannot collide.

### Decision
`import --slug` performs a **pre-flight refusal** when the destination `audit_log` is non-empty (`SELECT COUNT(*) FROM audit_log > 0`), before the ingest transaction. The message is actionable — "restore targets a fresh slug; run `project register <new-slug>` and import there" — and **never** the raw SQLite UNIQUE error (OQ-2, AC-10). The supported restore target is therefore a **freshly-registered (audit-empty) slug store**; restoring over a slug store that already has audit history is out of scope and fails loud. This holds C-5 as a constraint (fail-loud), not a redesign — the audit filter and append-only triggers are untouched.

This gate composes with the two other import pre-flights, all before any write: (1) live-PID refusal (ADR-003), (2) existing entry-count `--force` check (`import/mod.rs:259-265`, unchanged), (3) this non-empty-`audit_log` refusal. Ordering runs the cheapest structural gates first (PID, then existence via ADR-002) and the DB-query gates after `open`.

### Consequences
Easier: the operator gets a next-action message instead of a raw UNIQUE error; the collision-freedom of the supported path rests on a verified invariant (fresh slug ⇒ zero audit rows), so the happy path is provably safe. The append-only audit guarantee (ADR-005 vnc-014) is preserved — import never mutates audit history.

Harder: re-importing into an already-restored slug is refused — the operator must `register` a fresh slug and import there (accepted; it is the documented flow). This design depends on the assumption "a freshly-registered slug store has a zero-row `audit_log`"; if `register` ever writes an audit row at creation, the supported target collapses and this gate would refuse *every* target — a fixture asserting `register`-then-`COUNT(audit_log) == 0` guards that invariant.
