# vnc-047 — `context_cycle` tags: opaque cycle labels (EntryRecord-tag parity)

> Status: SCOPED — open questions resolved by human 2026-07-09 (folded in as decisions below).
> Tracks GH #940. Research inputs: vnc-045 (`context_tag`, #928) is the direct mechanism
> precedent; col-025 (goal on cycle_events, #3396) is the cycle-attribute precedent.

## Problem Statement

A `context_cycle` run cannot be labeled today. A run has a **run identity** worth recording —
general opaque labels such as workflow version, run mode, confidence-required, experiment arm, etc.
(e.g. `workflow:v1.3`, `mode:batch`, `arm:A`) — so that later, externally, a deployment can join
those labels against the per-cycle metrics `cycle_review` already produces (A/B improvement analysis
is one motivating use, not the only one). There is no place to put such labels: `context_cycle`
carries only `goal`, and the only per-cycle read surface (`context_cycle_review`) shows metrics but
no run labels. The enabler that is missing is **tag storage + surfacing** on a cycle, using the
*same* opaque-tag model entries already have (one tag model, not a bespoke cycle dialect). The tags
are a general run-identity label list, not a workflow-versioning-only field.

## Goals

1. A cycle carries **`tags`**: a general opaque run-identity label array (workflow version, run
   mode, confidence-required, arm, etc.), **engine-uninterpreted** (value-opacity, parity with
   EntryRecord tags / vnc-045 SD-8). The engine stores and returns tags it never interprets — no
   vocabulary, no allow-list, non-empty is the only check.
2. **Settable ONLY at cycle start, WHOLE-SET-ONCE** via an additive `tags` param on `context_cycle`,
   flowing through the hook path exactly as `goal` does (col-025) — extracted in
   `build_cycle_event_or_fallthrough`, carried on `RecordEvent` → `handle_cycle_event`, and
   persisted in the same transaction as the `cycle_start` event insert. The **first tag-bearing**
   `cycle_start` locks the entire tag set for that `feature_cycle`; every later start (same, subset,
   superset, or different tags) is a whole-set no-op. A **tagless** start does NOT lock — first tags
   win.
3. **Durable, in a queryable substrate** — tags are the source of truth in a new **`cycle_tags`
   junction table**, GC-protected so they survive the cycle-based telemetry purge for long-run
   analysis. The junction (indexed on `tag`) is the queryable substrate the deferred
   filter/learn-by-tag direction will build on.
4. **Surfaced in `context_cycle_review`** so a run shows its labels (markdown + JSON): review reads
   from `cycle_tags` and mirrors into `summary_json` for display, following the `goal` surfacing
   path end to end. `cycle_review` is the authoritative confirmation of what was recorded.
5. **Prefix-convention-friendly** (`workflow:v1.3`, `arm:A`) — by convention, not enforced; a
   `namespace` prefix (substring before the first `:`) carries meaning only to the reader. The
   engine NEVER parses prefixes (value-opacity).
6. **One tag model** — reuse the entry-tag opacity semantics and the `add_tag`-style
   `INSERT … ON CONFLICT DO NOTHING` primitive for the per-row start-time insert. The whole-set-once
   lock is enforced by an EXISTS-guard on `cycle_tags` for the `feature_cycle` inside the
   `cycle_start` transaction — a row-existence check, never a value/namespace parse (value-opacity
   preserved). Parity with the entry tag model is honored by **reserving the existing `context_tag`
   tool as the future mutation home** (see Non-Goals / deferred), not by building a mutation surface
   now.

## Non-Goals

1. **Tag mutation (add / remove / replace) on a cycle.** Tags are **whole-set-once at cycle start**
   (first tag-bearing start locks the set); there is no in-place cycle-tag mutation, no `replace`
   verb, no post-start editing in this feature. **Deferred future-mutation path (decided):** if
   mutation is ever needed it becomes an additive option on the **existing `context_tag` tool** to
   target cycle tags, defaulting to entry-targeting so the current interface is unchanged —
   explicitly **NOT** a new `context_cycle_tag` tool.
2. **Per-key / per-namespace write-once (REJECTED).** Locking is whole-set, not per-tag or
   per-namespace. Per-key write-once was explicitly rejected because deciding "is this namespace
   already set?" would force the engine to parse namespaces out of tag values, breaking
   value-opacity. The lock is a single row-existence check for the `feature_cycle`.
3. **A new MCP op or any change to the `context_cycle` interface beyond the additive `tags` param.**
4. **Cross-cycle QUERY / SEARCH by tag.** No cycle-list/search MCP surface exists today; query
   parity with entry tags is blocked on that surface and is deferred (future companion issue). The
   `cycle_tags` junction is built as the substrate that unblocks it later.
5. **Cross-run comparison / diffing / A/B aggregation.** The cross-run join is done externally;
   this feature only makes the labels storable and readable per run.
   - Note: **mid-run / outcome (`cycle_stop`) labels**, **post-start mutation**, and **cross-cycle
     read / aggregation** are all part of the *same* deferred future discussion (a later
     cycle-tag-query + richer-labeling feature), not separate tracks.
6. **Vocabulary, allow-list, length/charset validation, or prefix enforcement** — value-opacity
   (parity with vnc-045 SD-8; no `protected_tags` policy).
7. **A pre-review cycle-tag read surface.** Read-back is via `context_cycle_review` (post-review)
   only (decided — OQ-4); no lightweight in-run tag read is added. The frozen-skip (whole-set no-op)
   outcome is NOT caller-returnable (see AC-08); `cycle_review` remains the authoritative confirmation.
8. **Modifying entry `context_tag` / `context_correct`** — the entry path is left unchanged.
9. **Trust-level / identity authorization** — `Capability::Write` only; `agent_id` is audit-only
   (parity with vnc-045 SD-9).

## Background Research (grounded integration points at HEAD)

### Entry-tag model (the parity target — vnc-045 / nxs-008)
- `entry_tags` is a **junction table** `(entry_id, tag)` PK, `FK ON DELETE CASCADE`, indexed on
  `(tag)`, `(entry_id)`, `(tag, entry_id)` — `db.rs:573-607`, `migration.rs:1688-1700`. Pattern
  #373: junction for tags that are (or will be) queried by element; JSON TEXT only for Vec fields
  never queried by element.
- `EntryRecord.tags: Vec<String>` (`schema.rs:55`); loaded via batch junction query
  `load_tags_for_entries` + `apply_tags` (`read.rs:111-158`).
- `context_tag(id, action, tag)` handler `tools.rs:1542-1633`; `TagParams` `tools.rs:364-376`
  (`id`, `action ∈ {add,remove,replace}`, opaque `tag`, audit-only `agent_id`). Gated by a single
  `require_cap(Capability::Write)` (`tools.rs:1562`; `require_capability` `infra/registry.rs:92`).
  There is no `Capability::Tag` — one Write gate covers all three verbs.
- Store primitives (`write.rs`): `add_tag` (INSERT … ON CONFLICT DO NOTHING, idempotent, :281),
  `remove_tag` (single-row DELETE, :310), `replace_tag(entry_id, namespace, new_tag)` (one txn:
  read prior via `LIKE 'ns:%' ESCAPE`, scoped DELETE, INSERT; returns evicted prior, :350).
  `like_escape` neutralizes LIKE metacharacters (:419).
- **Value-opacity is explicit**: only non-empty is checked (`tools.rs:1576-1584`); "NO value-hygiene,
  no allow-list" (`store_tag.rs:108`). `namespace = tag[..tag.find(':')]` (`tools.rs:651`), derived,
  never validated. Two comment-only retrofit seams (no stub): `tools.rs:1558-1561`, `1609-1614`.
- **Audit (vnc-045 SD-7 shape)**: `StoreTagService::tag` (`store_tag.rs:94-193`) runs
  `check_write_rate`, dispatches the verb, then fire-and-forget emits one `operation="context_tag"`
  audit event with `metadata {action, namespace, tag, prior_value, new_value}`; `prior_value`
  mandatory on remove/replace.

### Cycle storage — TWO paths, and a correction to the issue's premise
- **`context_cycle` MCP handler is session-unaware and persists NOTHING** (`tools.rs:4062`,
  comment `tools.rs:4128`). `CycleParams` (`tools.rs:515-542`): `type`, `topic` (= `cycle_id` /
  `feature_cycle`), `phase`, `outcome`, `next_phase`, `goal` (start-only, ≤`MAX_GOAL_BYTES=1024`),
  `agent_id`, `format`. The actual event write is the **hook path**:
  `hook.rs:769 build_cycle_event_or_fallthrough` (extracts `goal` at `:839`) → `RecordEvent` →
  `listener.rs handle_cycle_event` (fire-and-forget spawn) → `db.insert_cycle_event(...)`
  (`db.rs:320`, direct write pool, never the analytics drain). **Implication:** a "set at start"
  tag must ride this hook path exactly as `goal` does — a bare MCP `context_cycle` call does not
  persist it.
- **`cycle_events`** (`migration.rs:497-506`, +`goal TEXT` v15→v16 `:561`): structural audit trail
  keyed `(cycle_id, seq, event_type)`; `goal` lives only on the `cycle_start` row (col-025 ADR-001
  #3396). Read via `get_cycle_start_goal` (`db.rs:371`).
- **`cycle_review_index`** (`migration.rs:617-623`): durable per-cycle aggregate, PK `feature_cycle`,
  `summary_json` holds the full serialized `RetrospectiveReport`, plus v5 aggregate columns.
  Single writer `store_cycle_review()` (`cycle_review_index.rs:301`, `write_pool_server`,
  TOCTOU-safe INSERT-or-UPDATE). Written **only at review time** — the row does not exist during
  the run. `SUMMARY_SCHEMA_VERSION = 5` (`cycle_review_index.rs:54`); a change to
  `RetrospectiveReport` round-trip fidelity requires a bump (→6) across three paths + the pinned
  test (#4178, #5051).
- **Telemetry GC** (`retention.rs`, crt-036): a cycle is purgeable only once it has a
  `cycle_review_index` row; GC deletes **observations → query_log → injection_log → sessions**
  (`gc_cycle_activity` :116). **`cycle_events`, `cycle_review_index`, `entries`,
  `observation_phase_metrics` are protected** and asserted unchanged by
  `test_gc_protected_tables_regression` (:521-643).
  - **KEY CORRECTION to issue #940:** the issue treats `cycle_events` as "ephemeral" and concludes
    tags "must ride the durable cycle_review aggregate row." In the code, **`cycle_events` is NOT
    purged** — it is a protected table that survives GC, exactly like `cycle_review_index`. The
    genuinely purgeable per-cycle store is **`sessions` (and its `keywords` column)**. So durability
    does **not** force `cycle_review_index`; it forces "anything except `sessions`." Both
    `cycle_events` and a new dedicated table satisfy the durability requirement.
- **JSON-array precedent**: `sessions.keywords TEXT` holds a JSON array string (col-022,
  `sessions.rs:39`) — but it is on the **purgeable** `sessions` table and is now inert (crt-025 WA-1,
  #2987). Not a durability-safe model to copy.
- Migration template: `CURRENT_SCHEMA_VERSION = 30` (`migration.rs:26`); additive `ALTER TABLE ADD
  COLUMN` guarded by `pragma_table_info` COUNT pre-check (canonical `migration.rs:314-343`; batch
  form `:1500-1571`).

### `context_cycle_review` surfacing (the `goal` end-to-end precedent)
- Handler `tools.rs:2409+`; output type `RetrospectiveReport`
  (`unimatrix-observe/src/types.rs:382-472`, `goal` at :436). `goal` flow: read
  `get_cycle_start_goal` → set `report.goal` (`tools.rs:3409-3428`) → serialized whole into
  `summary_json` by `build_cycle_review_record` (`tools.rs:4554`) → JSON output automatic
  (`briefing.rs:105`), markdown via `render_goal_section` (`retrospective.rs:203-217`, called :49);
  header meta line at `retrospective.rs:157-168`.
- **A new cycle-level `tags` field follows this exact path**: read tags from the durable store →
  populate `report.tags` → it rides `summary_json` (so it survives even the stale-purged memo path,
  where `raw_signals_available=0`) → JSON automatic, markdown via a new section or header-meta
  entry. Adding a serde field to `RetrospectiveReport` requires the `SUMMARY_SCHEMA_VERSION` bump.

## Proposed Approach

Set-once at start, stored in a durable cycle-keyed junction, surfaced through `cycle_review`.

**Storage (source of truth): a new `cycle_tags` junction table** keyed by `feature_cycle`
(`cycle_tags(feature_cycle TEXT, tag TEXT, PK(feature_cycle, tag))`), indexed on `(tag)`, added to
the retention protected set. Rationale:
- **Physical + semantic parity with `entry_tags`** — the issue's north star ("one tag model");
  the `add_tag`-style `INSERT … ON CONFLICT DO NOTHING` primitive ports 1:1, re-keyed from
  `entry_id` to `feature_cycle`.
- **Queryable substrate** — the `(tag)` index is what the deferred filter/learn-by-tag and
  cross-cycle query direction will build on (pattern #373: junction, not JSON, when the column will
  be queried by element).
- **Durable** — as a protected table it survives GC (parity with `cycle_events` /
  `cycle_review_index`; sessions is the purgeable store).

**Write path (whole-set-once, at start only):** `CycleParams` gains `tags: Option<Vec<String>>`.
It is extracted in `build_cycle_event_or_fallthrough` (hook.rs) beside `goal`, carried on
`RecordEvent` → `handle_cycle_event`, and written via a new `insert_cycle_tags` into `cycle_tags`
**in the same transaction as the `cycle_start` event insert**. The transaction first runs an
**EXISTS-guard** — `SELECT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle = ?)`: if any tag
already exists for the `feature_cycle`, the whole incoming set is skipped (frozen-skip no-op),
regardless of whether it is the same, a subset, a superset, or different; if none exist and the
incoming set is non-empty, every tag is inserted (per-row `INSERT … ON CONFLICT(feature_cycle, tag)
DO NOTHING`). A **tagless** start does not write and does not lock — the first tag-bearing start
wins. The guard is a row-existence check only; it never parses tag values or namespaces
(value-opacity). Tags are honored only on the `cycle_start` event (parity with how `goal` is
start-only). There is **no** MCP mutation op and **no** interface change beyond the additive `tags`
param.

**Ack echo + tracing (best-effort, non-gating):** the EXISTING `context_cycle` ack string carries a
best-effort echo — a start-with-tags call appends "N labels accepted for recording…" (accept, not a
durability guarantee, since the hook write is fire-and-forget downstream of the ack); a
non-start-with-tags call appends "tags ignored — recorded only at cycle start". The listener emits a
tracing line distinguishing wrote-set vs frozen-skip. No new MCP interface, no read-back API; the
frozen-skip outcome is not returnable to the caller — `cycle_review` is the authoritative
confirmation. (Cross-ref ADR-007.)

**Read / surface path:** `context_cycle_review` reads tags from `cycle_tags` (a new
`get_cycle_tags(feature_cycle)` store getter, parity with `get_cycle_start_goal`), populates a new
`RetrospectiveReport.tags` field, which serializes into `summary_json` (so it survives the
stale-purged memo path) and renders in markdown (new section or header-meta entry) and JSON
(automatic). `cycle_tags` is the source of truth; the `summary_json` copy is display-only.

**Deferred future mutation (decided, not built):** should in-place editing ever be needed, it is an
additive option on the **existing `context_tag` tool** to target cycle tags, defaulting to
entry-targeting so the current `context_tag` interface is unchanged — **not** a new
`context_cycle_tag` tool. This reservation is how the "one tag model" parity is honored without
building mutation now.

## Acceptance Criteria

- **AC-01 (value-opacity):** A cycle can carry `tags`, an opaque string array. Any non-empty value
  is accepted (`workflow:v1.3`, `arm:A`, free-form `foo` alike); the engine applies no
  vocabulary/allow-list/length/charset check beyond non-empty. Tags are stored and returned verbatim.
- **AC-02 (whole-set-once at start via hook path):** Tags supplied on a `context_cycle` **start**
  (the additive `tags` param) flow through `build_cycle_event_or_fallthrough` → `RecordEvent` →
  `handle_cycle_event` into `cycle_tags`, associated with that `feature_cycle`, written in the same
  transaction as the `cycle_start` event insert. An EXISTS-guard on `cycle_tags` for the
  `feature_cycle` enforces whole-set-once: the first tag-bearing start writes the full set; every
  later start (same / subset / superset / different) is a whole-set no-op (frozen-skip); a tagless
  start does not lock (first tags win). The guard is a row-existence check only — it never parses tag
  values or namespaces (value-opacity). Tags on a non-start event are ignored. There is no post-start
  mutation path.
- **AC-03 (storage substrate):** Tags are stored in a new `cycle_tags(feature_cycle, tag)`
  junction (PK `(feature_cycle, tag)`, indexed on `(tag)`), added via an additive migration
  (`CURRENT_SCHEMA_VERSION` 30→31, `pragma_table_info`/`sqlite_master`-guarded, three-path
  fresh-create + migration hygiene). This junction is the source of truth.
- **AC-04 (durability):** Cycle tags survive the cycle-based telemetry GC (`gc_cycle_activity`) —
  `cycle_tags` is registered in the protected-table set, never purged (contrast `sessions`). A
  regression test asserts cycle tags are unchanged after a full GC pass (parity with
  `test_gc_protected_tables_regression`).
- **AC-05 (surfaced in review):** `context_cycle_review` shows a run's tags in both JSON (a `tags`
  field on `RetrospectiveReport`) and markdown (a dedicated section or header-meta entry), populated
  by reading `cycle_tags` (via a `get_cycle_tags` getter) and folding into the report so it rides
  `summary_json` (surviving the stale-purged memo path). `SUMMARY_SCHEMA_VERSION` is bumped (5→6)
  for the new field across the three schema paths + the pinned-version test.
- **AC-06 (interface stability):** The only external interface change is the additive `tags` param
  on `context_cycle`. No new MCP tool is added; the entry-targeting `context_tag` and
  `context_correct` are unchanged.
- **AC-07 (prefix convention, not enforced):** A `namespace` prefix (`workflow:`, `arm:`) is
  supported by convention only — no prefix is required, derived, or validated by the engine at the
  write path; prefixes carry meaning only to the human/analyst reading `cycle_review`.
- **AC-08 (ack echo + tracing — BEST-EFFORT, NON-GATING):** As a nice-to-have that MUST NOT block
  any gate, the existing `context_cycle` ack string SHOULD echo tag intake — start-with-tags →
  "N labels accepted for recording…" (accept, not a durability guarantee); non-start-with-tags →
  "tags ignored — recorded only at cycle start" — and the listener SHOULD emit a tracing line
  distinguishing wrote-set vs frozen-skip. No new MCP interface and no read-back API are added; the
  frozen-skip outcome is not caller-returnable, and `cycle_review` remains the authoritative
  confirmation. Absence or imperfection of this echo never fails a gate. (Cross-ref ADR-007.)

## Constraints

- **One tag model** — reuse entry-tag opacity semantics and the `add_tag`-style `INSERT … ON
  CONFLICT DO NOTHING` primitive; do not fork a bespoke cycle tag dialect. The future mutation home
  is reserved on the existing `context_tag` tool (issue north star).
- **Whole-set-once** — tags are honored only on the `cycle_start` event; the first tag-bearing start
  locks the entire set (later starts are whole-set no-ops; a tagless start does not lock). Enforced
  by an EXISTS-guard on `cycle_tags` for the `feature_cycle` inside the `cycle_start` transaction
  (row-existence check + per-row `INSERT … ON CONFLICT DO NOTHING`). No in-place mutation, no
  `replace`/`remove` verb ships. **Per-key / per-namespace write-once is rejected** — it would force
  the engine to parse namespaces and break value-opacity.
- **Value-opacity** — no vocabulary/allow-list/validation beyond non-empty (parity with vnc-045
  SD-8); no `protected_tags` policy; no engine-side prefix derivation or enforcement; the
  whole-set-once guard is a row-existence check, never a value parse.
- **`context_cycle` MCP handler is session-unaware and persists nothing** — the `tags` set at
  start MUST ride the `hook.rs`/`handle_cycle_event` path (like `goal`); a bare MCP `context_cycle`
  call will not persist tags. Write them in the same transaction as the `cycle_start` insert.
- **Durability** — tags MUST NOT live on `sessions` (purgeable). `cycle_tags` must be a GC-protected
  table (register it in `retention.rs`'s protected set).
- **`cycle_review_index` is written only at review time** via the single writer
  `store_cycle_review()`; tags are surfaced via `RetrospectiveReport` (riding `summary_json`),
  read from `cycle_tags` each review — the aggregate row is display-only, not the source of truth
  (it cannot be: it does not exist during the run).
- **Schema-version discipline** — the new `RetrospectiveReport.tags` field changes round-trip
  fidelity ⇒ bump `SUMMARY_SCHEMA_VERSION` (5→6) across fresh-create + migration + pinned test
  (#4178, #5051). The new `cycle_tags` table ⇒ `CURRENT_SCHEMA_VERSION` 30→31, additive, with the
  `pragma_table_info`/`sqlite_master` existence guard.
- Rust workspace rules: file-size limits; extend existing fixtures/helpers; Grep/Glob not Bash.

## Open Questions

None — all four open questions were resolved by the human (2026-07-09) and folded into the
decisions above. Recorded here for traceability:

- **OQ-1 (schema placement) → RESOLVED:** new **`cycle_tags(feature_cycle, tag)` junction**, indexed
  on `(tag)`, GC-protected — the source of truth and the queryable substrate for the future
  filter/learn-by-tag direction. `cycle_review` reads it and mirrors into `summary_json` for display
  only. (See Proposed Approach / AC-03/04/05.) The durability correction stands: `cycle_events` is
  GC-protected and survives purge; `sessions` is the purgeable per-cycle store — so durability never
  forced `cycle_review_index`.
- **OQ-2 (mutable vs set-once) → RESOLVED: WHOLE-SET-ONCE.** Tags are settable only at cycle start
  via the hook path; the first tag-bearing start locks the entire set (later starts are whole-set
  no-ops; a tagless start does not lock). Enforced by a row-existence EXISTS-guard, never a value
  parse. Per-key / per-namespace write-once was explicitly rejected (it would break value-opacity by
  forcing namespace parsing). No mutation ships. (See Goals #2/#6, Non-Goals #1/#2, AC-02.)
- **OQ-3 (op surface) → RESOLVED:** no new MCP op and no `context_cycle` interface change beyond the
  additive `tags` param. The deferred future-mutation path is an additive, entry-defaulting option
  on the **existing `context_tag` tool** — not a new `context_cycle_tag`. (See Non-Goals #1/#2,
  AC-06.)
- **OQ-4 (read-back) → RESOLVED:** read-back is via `context_cycle_review` (post-review) only; no
  pre-review cycle-tag read surface. (See Non-Goals #6.)

## Dependencies

- **vnc-045** (`context_tag`, #928) — the tag-model precedent: value-opacity (SD-8), the
  `add_tag`-style `INSERT … ON CONFLICT DO NOTHING` primitive reused for the start-time insert
  (`write.rs:281`), and the tool reserved as the future cycle-tag mutation home. (The `remove`/
  `replace` verbs and audit-mutation shape are NOT used — no mutation ships here.)
- **col-025** (#3396) — cycle-attribute precedent: attribute set at start via the hook path and
  surfaced in `cycle_review` (`goal` end to end).
- **crt-036 retention** (`retention.rs`) — the GC-protected-table set the new store must join.
- **crt-055 / #4178** — `SUMMARY_SCHEMA_VERSION` bump discipline (three paths + pinned test) for
  the new `RetrospectiveReport` field.
- **nxs-008 / #360 / #373** — `entry_tags` junction model and the junction-vs-JSON rule.

## Tracking

- GH Issue: **#940** — https://github.com/dug-21/unimatrix/issues/940 (design complete, Session 1).
- Future companion (deferred, out of scope): cross-cycle tag QUERY/SEARCH + cross-run
  comparison/A-B aggregation, blocked on a cycle-list/search MCP surface that does not exist today.
