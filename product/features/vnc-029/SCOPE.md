# vnc-029 — Audit Log Retention/Archival Contract

## Problem Statement

vnc-014 (schema v25) made `audit_log` append-only: `BEFORE DELETE` / `BEFORE UPDATE` DDL
triggers reject all mutation, `gc_audit_log()` is a no-op, and import no longer clears audit
history (ADR-005, entry #4359). The table therefore grows without bound. Security review
FINDING 5 (PR #577) flagged this and required a decided retention/archival contract before
HTTP-mode load begins accumulating rows.

The urgency is architectural, not volumetric (issue #578 corrected framing): production data
shows 3,882 rows / ~330 KB over 10 weeks of single-user stdio use — audit_log is noise at
personal cloud scale. But F2 (vnc-024 HTTP server) is shipped and F3 (vnc-026, #679) puts
remote hook clients on it; once rows accumulate under HTTP load, retrofitting retention onto
a live append-only table is harder than deciding the contract now. OSS users need a defined,
documented path for managing audit_log growth, and any row removal needs a new ADR that
sanctions the exception without compromising the tamper-evidence rationale of ADR-005.

## Goals

1. Decide and document the **archival contract**: archive format (sidecar JSONL vs separate
   SQLite vs cloud storage), integrity guarantees the archive carries, and queryability
   expectations — answered by a new ADR.
2. Decide and document the **sanctioned trigger-suspension mechanism**: the exact, narrowly
   scoped procedure by which archived rows may be removed from the live table despite the
   `audit_log_no_delete` trigger, governed by ADR and constrained so the append-only
   guarantee remains intact outside the archival path.
3. Decide **manual vs automatic** archival and the trigger condition (row count, file size,
   age, or documented manual cadence) for the personal-cloud tier.
4. Explicitly **accept or defer** the long-term separate-file placement of audit_log —
   recorded as a decision, not silently foreclosed by the implementation.
5. Define the **reader contract** post-archival: which consumers (S8 graph enrichment,
   rate-limit counting, cycle review, export/import) read audit_log, and what invariant each
   requires of the archival boundary.
6. Ship the minimal implementation the contract requires for the personal-cloud tier
   (expected: a manual, user-initiated archival path — see Open Question OQ-1).

## Non-Goals

- **Broader DB-size management.** Entry/vector accumulation and analytics-table growth are
  the real size drivers and are tracked separately: #510 (context_purge), #363 (content
  efficacy model), crt-036 (analytics retention, complete). This feature touches audit_log
  only.
- **Weakening the append-only posture.** No general-purpose delete path, no
  `audit_log_retention_days`-driven background deletion, no restoring `gc_audit_log()` to a
  deleting implementation.
- **Enterprise compliance features.** No cryptographic signing service, external WORM
  storage integration, or multi-tenant retention policies. The contract may name these as
  future extensions.
- **Moving other analytics tables.** The separate-file question applies to audit_log alone;
  observations/co_access/query_log/etc. remain in the single DB per the cross-table join
  requirement.
- **Server-side F3 changes.** vnc-026 is client-side JS; this feature must not create a
  dependency that blocks #679.

## Background Research

### audit_log today (codebase, schema v27)

- **Schema**: 12 columns (`event_id`, `timestamp`, `session_id`, `agent_id`, `operation`,
  `target_ids`, `outcome`, `detail`, `credential_type`, `capability_used`,
  `agent_attribution`, `metadata`), 4 indexes, 2 append-only triggers. DDL duplicated
  byte-identically in `crates/unimatrix-store/src/db.rs` (create path) and
  `migration.rs` v24→v25 block (R-11 mitigation, ADR-004 entry #4358).
- **event_id** is allocated from the `next_audit_id` counter (monotonic, seeded from
  `MAX(event_id)` on migration; lesson #4405 covers the counter-name pitfall). Archival that
  removes rows leaves gaps in the live table but the counter guarantees no reuse — archive +
  live remain a disjoint, totally ordered set.
- **gc_audit_log()** (`retention.rs:271`) is a no-op returning `Ok(0)`; still called from
  the cycle-GC step 4f in `services/status.rs:1561` with the (now vestigial)
  `retention_config.audit_log_retention_days` field. Tests pin the no-op behavior.

### Readers — correction to the issue framing

Issue #578 states audit_log is "outside the intelligence join graph." That is **mostly but
not entirely accurate**, and the exception is load-bearing for the archival boundary:

1. **S8 graph enrichment (crt-041)** — `services/graph_enrichment_tick.rs:311` reads
   `audit_log WHERE operation='context_search' AND outcome=0 AND event_id > {watermark}`
   to derive co-retrieved pairs. It is watermark-gated and forward-only: rows at or below
   the persisted S8 watermark are never re-read. **Archival constraint: only rows with
   `event_id <= s8_watermark` may be removed.**
2. **Rate-limit counting** — `audit_write_count_since()` (`store/audit.rs:81`) counts recent
   `context_store`/`context_correct` rows by timestamp. Recency-bound; archiving old rows is
   safe given any sane age threshold.
3. **Session join (GH-582)** — audit_log joins `sessions` on `session_id` in tests/
   diagnostics; crt-036 already prunes old sessions, so old audit rows already have dangling
   session_ids. No new constraint.
4. **Export** (`export.rs:623`) — dumps all live rows. Post-archival, export contains only
   live rows; the contract must state whether the archive is part of the export story.
5. **Cycle review / retrospectives** — reads recent operations by `operation`/`detail`
   (e.g., vnc-025 transcript-purge audits). Recency-bound.

### Tamper-evidence: actual threat model

The triggers are enforceable only against the application and well-behaved tools — any local
user with `sqlite3` can `DROP TRIGGER`. There is no hash chain or signature on rows today.
The guarantee ADR-005 provides is **defense-in-depth against application bugs and accidental
deletion**, not adversarial tamper-proofing. The new ADR's "does not compromise
tamper-evidence" bar should be stated against this honest threat model; the archive can
*raise* the bar cheaply (e.g., hash-chained JSONL) but is not required to solve a problem the
live table never solved. Aligns with goal entry #4548 (trustworthy provenance).

### Trigger-suspension mechanics (SQLite)

SQLite has no `DISABLE TRIGGER`. Suspension means `DROP TRIGGER audit_log_no_delete` →
`DELETE ... WHERE event_id <= boundary` → re-`CREATE TRIGGER`, all inside one transaction on
the single-connection `write_pool_server()` (serialized by construction; ASS-047 ceiling
~200 integrity writes/s is irrelevant to a manual batch operation). The ADR must pin: the
drop/recreate happens only inside the archival routine, the trigger DDL is byte-identical to
db.rs/migration.rs, and the transaction either fully completes (rows verifiably in archive
before delete) or rolls back with triggers restored.

### Precedents

- **crt-036** (`retention.rs`): K-window cycle GC — per-cycle transactions, `max_per_tick`
  caps, explicit delete ordering. The configuration/reporting shape to follow if archival is
  automated.
- **Export/import** (`export.rs`/`import/`): JSONL row format for audit_log already exists
  (`format.rs` `AuditLogRow`), reusable for a sidecar archive format.
- **No `ATTACH DATABASE` usage anywhere** in the workspace — a separate SQLite archive file
  would introduce a new infrastructure pattern; a JSONL sidecar reuses an existing one.
- **crt-007 history**: col-012 deliberately eliminated JSONL in favor of SQLite for *queried*
  data (entry #407). Archive data is read rarely (compliance review, retrospective forensics),
  so that precedent does not force SQLite here, but the ADR should address it.

### HTTP-mode load (F2/F3)

vnc-026 (#679) is a pure-JS client; the server-side HTTP surface shipped in F2 (vnc-024/025).
Audit rows are written per MCP tool call and for server events (e.g., transcript purges) —
`/observe` hook ingestion writes observations, not audit rows. Realistic personal-cloud HTTP
load is a small multiple of stdio rate; even 100× current rate is ~3.3 MB/10 weeks. This
confirms the issue's framing: the deliverable is a decided contract and a working manual
path, not an aggressive automated pipeline.

## Proposed Approach

Recommendation for the design session (rationale follows; all three #578 questions get an
explicit answer):

1. **Archive format: hash-chained JSONL sidecar** (e.g.,
   `{data_dir}/audit-archive/audit-{first_event_id}-{last_event_id}.jsonl` plus a chain
   record carrying the previous segment's hash). Reuses the existing export row format;
   human-readable; greppable; strictly raises the integrity bar over the live table.
   Separate SQLite rejected for now (new ATTACH pattern, no query demand); cloud storage
   deferred to enterprise tier.
2. **Sanctioned suspension: single-transaction drop-trigger/delete/recreate-trigger inside
   one `archive_audit_log(boundary)` store method**, callable only after the archive segment
   is written and fsynced, with boundary clamped to
   `min(requested, s8_watermark, now - min_age)`. Governed by a new ADR
   (vnc-029 ADR-001) that amends vnc-014 ADR-005.
3. **Manual first**: user-initiated archival (CLI subcommand or MCP admin tool — OQ-1) with a
   documented cadence in OSS docs; no background automation. A row-count advisory in
   `context_status` (e.g., surfaced when audit_log exceeds a threshold) keeps it discoverable
   without automating deletion.
4. **Separate-file placement: explicitly DEFERRED, not foreclosed.** The S8 watermark is the
   only intelligence-pipeline coupling; the archival boundary already respects it, so a future
   move to a dedicated file remains possible. Record as a deferred-option clause in the ADR.

## Acceptance Criteria

- AC-01: A new ADR (stored via /uni-store-adr) defines the archival contract: archive format,
  integrity guarantee (hash chain or equivalent), and queryability expectations, with
  explicit rationale against the sidecar-JSONL / separate-SQLite / cloud-storage option set.
- AC-02: The ADR sanctions a single, narrowly scoped trigger-suspension procedure (drop →
  delete → recreate within one transaction) as the *only* permitted mutation path, and states
  why tamper-evidence (per the honest threat model: application-level defense-in-depth) is
  preserved.
- AC-03: The ADR explicitly accepts or defers the separate-file long-term placement of
  audit_log, with rationale referencing the S8 watermark as the sole join-graph coupling.
- AC-04: The archival boundary is provably safe for all identified readers: rows with
  `event_id > s8_watermark` are never archived; a minimum-age guard protects rate-limit
  counting and cycle-review reads. (Testable: archival with a fresh watermark removes zero
  rows.)
- AC-05: Archival is atomic and verifiable: rows are present in the archive segment (and the
  segment durable) before any delete commits; on failure the transaction rolls back and both
  triggers are restored. (Testable: injected failure leaves row count and trigger DDL
  unchanged.)
- AC-06: After archival, both append-only triggers exist with DDL byte-identical to
  db.rs/migration.rs, and a subsequent direct `DELETE FROM audit_log` still aborts.
- AC-07: `event_id` continuity holds across archive + live table: the union is gap-free and
  duplicate-free; the `next_audit_id` counter is untouched by archival.
- AC-08: A documented manual cadence exists in OSS-facing docs (when to archive, how, what
  the archive contains, how to verify the hash chain).
- AC-09: The decision (manual vs automatic, and the advisory threshold if any) is recorded;
  if an advisory is implemented, `context_status` surfaces audit_log size without performing
  any deletion.
- AC-10: The vestigial `audit_log_retention_days` config field and the no-op step-4f call in
  `services/status.rs` are reconciled with the new contract (removed, repurposed, or
  explicitly retained with documentation) — no dead config implying time-based deletion
  remains.

## Constraints

- **Hard**: schema v25+ append-only triggers; `gc_audit_log()` no-op; any row removal
  requires the new ADR (vnc-014 ADR-005 amendment). Trigger DDL must remain byte-identical
  across db.rs / migration.rs / post-archival recreate.
- **Hard**: S8 watermark (crt-041) — archival must never remove unconsumed
  `context_search` rows (`event_id > watermark`).
- **Hard**: single write connection (`write_pool_server`, max_connections=1); archival
  transaction serializes against all integrity writes — must be bounded/batched so it does
  not stall HTTP-mode writes for long periods.
- **Hard**: `next_audit_id` counter is the event_id authority; archival must not reset or
  reseed it (lesson #4405).
- Export currently dumps all live audit rows; import appends; `drop_all_data` does not touch
  audit_log. The contract must state archive ↔ export/import semantics.
- Scope boundary: audit_log only (#510, #363, crt-036 own the broader size question).
- F3 (#679) must not be blocked: no server API changes that vnc-026 would depend on.
- Schema changes, if any, follow the migration cascade checklist (entry #4358 / #4125).

## Open Questions

- OQ-1: **Surface for manual archival** — CLI subcommand (`unimatrix-server` admin verb),
  MCP admin tool (trust-gated), or a documented offline procedure? CLI avoids exposing a
  delete-adjacent capability over MCP; MCP tool is reachable remotely. Human call.
- OQ-2: **Contract-only vs contract + implementation** — does vnc-029 ship the working
  archival path (Goal 6 / AC-04..AC-07), or only the ADR + docs with implementation as a
  follow-up? Issue #578 says "not necessarily a full implementation, but a decided and
  documented contract." Recommend shipping the manual path (small, testable) — confirm.
- OQ-3: **Minimum-age guard value** — what age threshold protects recency-bound readers
  (rate-limit window, cycle-review lookback)? Needs the rate-limit window constant and
  typical cycle duration; 30–90 days is the plausible range.
- OQ-4: **Archive in the export/import story** — is the JSONL archive considered part of a
  full backup (`context export` ignores it; user copies the directory), or should export
  optionally include archive segments?
- OQ-5: **Hash chain ceremony level** — per-row chain vs per-segment digest? Per-segment is
  cheaper and likely sufficient for the personal-cloud threat model.
- OQ-6: **Advisory threshold** — if `context_status` surfaces an archival suggestion, at what
  row count / byte size? (Production baseline: ~390 rows/week single-user stdio.)

## Tracking

- GitHub issue: #578 (design driver). Feature issue to be created in Session 1.
- Related: vnc-014 ADR-005 (entry #4359), ADR-004 (entry #4358), PR #577 FINDING 5,
  crt-041 S8 watermark (entry #4062), crt-036 retention, ASS-047 write ceiling,
  #679 / vnc-026 (F3 — must not be blocked).
