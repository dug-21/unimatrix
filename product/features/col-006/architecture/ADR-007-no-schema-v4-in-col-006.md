## ADR-007: No Schema v4 Migration in col-006

### Context

ASS-014's feature scoping (D14-7) recommended including the schema v3->v4 migration in col-006 to "establish the tables early." This migration would add 3 new telemetry tables (SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) and a `session_id` field to `EntryRecord`.

However, col-006's SCOPE.md explicitly states:
- "No new redb tables: col-006 does not add telemetry tables. Those are deferred to col-010."
- The 7 build components do not include schema migration.
- Non-goals include: "Telemetry tables -- SESSIONS, INJECTION_LOG, SIGNAL_QUEUE tables are deferred to col-010"
- Non-goals include: "Schema v4 migration -- Deferred to col-010 which introduces the telemetry tier"

col-006 is the transport layer. Its smoke tests (Ping/Pong, SessionStart/Stop) validate connectivity and round-trip latency. They do not write to telemetry tables. The SessionRegister and SessionClose requests in col-006 are logged (tracing) but do not persist to redb.

### Decision

col-006 does not include schema v4 migration. No new redb tables. No new fields on `EntryRecord`.

The UDS handler processes `SessionRegister` and `SessionClose` requests by:
1. Logging the event via `tracing::info!` (for observability during transport validation)
2. Returning `HookResponse::Ack`
3. NOT writing to any redb table

When col-010 ships, it adds the SESSIONS table and the handler logic that persists session data. The `HookRequest::SessionRegister` and `HookRequest::SessionClose` variants already exist in the wire protocol (defined by col-006) — col-010 only changes the server-side handler from "log and ack" to "persist and ack."

This separation aligns with the SCOPE.md principle: col-006 validates the transport, col-010 validates session lifecycle persistence.

### Consequences

**Easier:**
- col-006 ships without database schema changes, eliminating migration risk entirely.
- The zero-regression guarantee is simpler to verify — no new tables means no new table interaction bugs.
- col-006 can be tested without a running database (the Ping/Pong test only needs the UDS transport).

**Harder:**
- col-010 must implement both the schema migration AND the handler logic that consumes SessionRegister/SessionClose. This is a larger feature than if the tables existed from col-006.
- Integration tests for col-006 smoke tests (SessionStart/Stop) only verify transport round-trip, not data persistence. Data persistence is tested in col-010.
