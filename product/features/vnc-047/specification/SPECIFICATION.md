# vnc-047 — Specification: `context_cycle` opaque cycle tags

> Source: `product/features/vnc-047/SCOPE.md` (SCOPED, all four OQs resolved 2026-07-09).
> Risk inputs: `product/features/vnc-047/SCOPE-RISK-ASSESSMENT.md` (SR-01…SR-10).
> Tracks GH #940. Tag-model precedent: vnc-045 (#928). Cycle-attribute precedent: col-025 (#3396).

## Objective

Let a `context_cycle` run carry **opaque, engine-uninterpreted `tags`** (e.g. `workflow:v1.3`,
`arm:A`) set once at cycle start, stored durably as the source of truth in a new `cycle_tags`
junction table, and surfaced per-run in `context_cycle_review` (markdown + JSON). This uses the
existing entry-tag opacity model (one tag model, not a bespoke cycle dialect) and adds no new MCP
tool — only an additive `tags` param on `context_cycle`. The tags become an externally joinable
label substrate for later A/B / improvement analysis; the cross-run join itself is out of scope.

## Domain Models & Ubiquitous Language

- **Cycle tag** — an opaque, non-empty UTF-8 string associated with one cycle run. The engine
  stores and returns it verbatim; it never parses, validates, or interprets its content (parity
  with `EntryRecord.tags` / vnc-045 SD-8).
- **`feature_cycle`** — the cycle-run identity key. It is the `topic` field of `CycleParams`
  (a.k.a. `cycle_id`). It is the junction key for `cycle_tags` (re-keyed from `entry_tags`'
  `entry_id`).
- **Namespace-by-convention** — the substring of a tag before its first `:` (e.g. `workflow` in
  `workflow:v1.3`). It carries meaning only to the human/analyst reading the review. It is NOT
  derived, required, or validated at the cycle-tag write path. (Contrast: the entry `context_tag`
  path derives `namespace` for its `replace`/audit semantics — those are not used here.)
- **Set-once** — cycle tags are honored only on the `cycle_start` event and written exactly once
  per `(feature_cycle, tag)`. There is no mutation verb, no post-start editing. A re-issued start
  is idempotent (first write wins; see FR-6 / AC-02).
- **Source-of-truth junction vs. display-only mirror** — `cycle_tags` is the durable source of
  truth. `context_cycle_review` reads it each review and folds the tags into
  `RetrospectiveReport`, which serializes into `summary_json`. That `summary_json` copy is a
  **display-only mirror**, not a second source of truth (it cannot be: the review row does not
  exist during the run and is written only at review time).
- **Hook path** — the only persistence route for cycle events:
  `context_cycle` MCP → `build_cycle_event_or_fallthrough` (hook.rs) → `RecordEvent` →
  `handle_cycle_event` (listener.rs) → `db.insert_cycle_event`. The bare MCP `context_cycle`
  handler is session-unaware and persists nothing.

## Functional Requirements

Each requirement is testable; verification is tied to an AC below.

- **FR-1 (additive param).** `CycleParams` gains `tags: Option<Vec<String>>`. Its absence is
  indistinguishable from prior behavior; all existing `context_cycle` fields and behavior are
  unchanged. [AC-06]
- **FR-2 (value-opacity).** Each supplied tag is accepted iff non-empty. No vocabulary, allow-list,
  length, charset, or prefix check is applied. Tags are stored and returned byte-for-byte as
  supplied. An empty-string tag is rejected (parity with `tools.rs:1576-1584`). [AC-01]
- **FR-3 (set-once, start-only, hook-path persistence).** Tags are persisted only when supplied on
  a `context_cycle` **start** event, and only via the hook path: extracted in
  `build_cycle_event_or_fallthrough` beside `goal`, carried on `RecordEvent`, written by
  `handle_cycle_event` into `cycle_tags` **in the same transaction as the `cycle_start` event
  insert**. There is no second persistence route. [AC-02, SR-03]
- **FR-4 (tags ignored on non-start events).** Tags supplied on any non-start cycle event
  (`phase`, `outcome`, `next_phase`, etc.) are ignored — not persisted, not an error. [AC-02]
- **FR-5 (junction storage, source of truth).** Tags are written to a new
  `cycle_tags(feature_cycle TEXT, tag TEXT, PRIMARY KEY(feature_cycle, tag))` junction indexed on
  `(tag)`, via a `add_tag`-style `INSERT … ON CONFLICT(feature_cycle, tag) DO NOTHING` primitive
  ported (re-keyed) from `entry_tags`' `add_tag` (`write.rs:281`). This junction is the source of
  truth. [AC-03, SR-06]
- **FR-6 (whole-set-once, idempotent re-issue).** The first **tag-bearing** `cycle_start` locks
  the **entire** tag set for that `feature_cycle`. Every later start — whether it supplies the same
  set, a subset, a superset, or a wholly different set — is a **no-op on tags**: the whole tag
  write is skipped when any `cycle_tags` row already exists for that `feature_cycle`. There is **no
  accumulation and no per-key/per-row behavior** — it is all-or-nothing per cycle. A **tagless**
  start does NOT lock (first *tags* win, not first start). Re-issue never duplicates rows and never
  errors. Enforced by an **EXISTS-guard inside the `cycle_start` transaction** (`SELECT 1 FROM
  cycle_tags WHERE feature_cycle = ?` → skip the whole insert if present), NOT by namespace parsing
  or per-tag conflict resolution — value-opacity is preserved. [AC-02, AC-02a, SR-05]
- **FR-7 (authorization + no per-tag audit event).** The write is gated by a single
  `require_cap(Capability::Write)` (parity with `context_tag`, `tools.rs:1562`). There is no
  `Capability::Tag`. `agent_id` is **audit-only** — it does not authorize or scope the write
  (parity with vnc-045 SD-9 / ADR-008). **No per-tag audit event is emitted** — unlike the entry
  `context_tag` mutation path (which emits one `operation="context_tag"` event per call), the
  set-once cycle-tag write rides the fire-and-forget `cycle_start` insert and emits no dedicated
  tag audit event; the `cycle_start` event itself is the trail. [AC-06]
- **FR-8 (durability across GC — protection by omission).** Cycle tags survive the cycle-based
  telemetry GC because `gc_cycle_activity` deletes only an **enumerated set**
  (observations → query_log → injection_log → sessions); `cycle_tags` is **not in that delete
  enumeration**, so it is protected **by omission** (parity with `cycle_events` /
  `cycle_review_index`; contrast the purgeable `sessions`). The load-bearing, easy-to-miss step is
  adding `cycle_tags` to the **regression test's** protected-table assertion set so a future GC
  change that adds it to the delete list is caught. [AC-04, SR-09]
- **FR-9 (surfaced in review — JSON).** `RetrospectiveReport` gains a `tags` field.
  `context_cycle_review` reads tags via a new `get_cycle_tags(feature_cycle)` store getter (parity
  with `get_cycle_start_goal`), populates `report.tags`, which serializes automatically into JSON
  output and into `summary_json` (so tags survive the stale-purged memo path where
  `raw_signals_available=0`). [AC-05]
- **FR-10 (surfaced in review — markdown).** `context_cycle_review` markdown renders the run's tags
  in a dedicated section or header-meta entry (parity with `render_goal_section` /
  `retrospective.rs`). A cycle with no tags renders no spurious section. [AC-05]
- **FR-11 (interface stability).** No new MCP tool is added. The entry-targeting `context_tag` and
  `context_correct` handlers are unchanged. The only external surface change is the additive `tags`
  param on `context_cycle` and the additive `tags` field on the `context_cycle_review` output.
  [AC-06]
- **FR-12 (ack echo — best-effort SHOULD, non-gating).** On the **existing** `context_cycle` MCP
  ack string (no new interface, no read-back API), the handler SHOULD echo tag intake:
  - Start-with-tags → `"N tags accepted at cycle start: [...]"`. This is **accept-for-recording,
    explicitly NOT a durability guarantee** — `context_cycle_review` remains the authoritative
    confirmation (parity with the `goal` fire-and-forget ack).
  - Non-start-with-tags → `"tags ignored — only recorded at cycle start."`
  - A listener-side tracing log SHOULD distinguish **wrote-set vs. frozen-skip** (the FR-6
    EXISTS-guard skip) for operator visibility. The frozen-skip case **cannot** be returned to the
    caller without a new interface — do NOT spec a caller-visible frozen-skip signal.

  This entire requirement is **best-effort**: it MUST NOT block a gate and MUST NOT become a
  gate-critical MUST. [AC-09, non-gating]

## Non-Functional Requirements

- **NFR-1 (schema-version discipline — `cycle_tags` table).** Adding `cycle_tags` requires
  `CURRENT_SCHEMA_VERSION` 30→31. This is an **independent cascade** whose per-path acceptance
  line-items are enumerated in AC-03. Additive `CREATE TABLE`/index guarded by
  `sqlite_master`/`pragma_table_info` existence pre-check (canonical `migration.rs:314-343`).
  [SR-01]
- **NFR-2 (schema-version discipline — `RetrospectiveReport.tags` field).** Adding the serde
  `tags` field changes `RetrospectiveReport` round-trip fidelity ⇒ `SUMMARY_SCHEMA_VERSION` 5→6.
  This is a **second, independent cascade** whose per-path acceptance line-items are enumerated in
  AC-05. It is distinct from NFR-1 and must not be lumped with it. [SR-01, crt-055/#4178]
- **NFR-3 (version-collision re-verification).** Both `CURRENT_SCHEMA_VERSION = 31` and
  `SUMMARY_SCHEMA_VERSION = 6` must be re-verified as the next free numbers against HEAD **at
  implementation start**, not only at design time. If a parallel-merged feature has claimed either,
  flag for renumber before proceeding. [SR-02, assumption A2]
- **NFR-4 (interface stability / backward compatibility).** No new MCP tool; the additive `tags`
  param is `Option`-typed so omission preserves existing wire behavior. Existing `context_cycle`,
  `context_tag`, and `context_correct` callers are unaffected. [SR-06]
- **NFR-5 (one tag model).** Reuse entry-tag opacity semantics and port the `add_tag`-style
  `INSERT … ON CONFLICT DO NOTHING` primitive re-keyed to `feature_cycle`; do not fork a bespoke
  cycle-tag dialect. Cite vnc-045 / nxs-008 as source. [SR-06]
- **NFR-6 (silent-failure containment on the fire-and-forget path).** The hook path has a
  documented history of silently dropping cycle data when the SM session is absent/evicted or
  `feature_cycle` is NULL (#4140, #981). Cycle tags MUST persist even when the session is
  absent/evicted (parity with the #4136 pre-register fix). A dropped/absent session must not
  silently lose a start's tags. There is no "did my tags persist?" API — persistence is
  set-and-forget by design. [SR-03, SR-07, assumption A1]
- **NFR-7 (queryable substrate shape).** The `(tag)` index must be shaped so the deferred
  cross-cycle filter/learn-by-tag / query direction needs no re-migration (pattern #373: junction,
  not JSON, when the column will be queried by element). [SR-04]
- **NFR-8 (no back-fill).** SUMMARY staleness is advisory-only; pre-existing v5 cached reviews do
  not recompute to v6. Only cycles started after deployment carry tags; historical cycles show no
  tags **by design**. This is documented expectation, not a defect. [SR-10]
- **NFR-9 (workspace rules).** Respect file-size limits; extend existing fixtures/helpers rather
  than create isolated scaffolding; use Grep/Glob not Bash for search.

## Acceptance Criteria

Each AC carries a verification method. ACs marked **[assembled-path]** MUST be proven by driving
the assembled `context_cycle`-start → hook → listener → `context_cycle_review` chain, NOT a
store-only structural test (SR-08; documented recurring gate miss — `proven_by` must cite an
assembled-path test).

- **AC-01 (value-opacity).** A cycle accepts `tags` as an opaque string array; any non-empty value
  is stored and returned verbatim (`workflow:v1.3`, `arm:A`, free-form `foo` alike); an empty
  string is rejected; no vocabulary/allow-list/length/charset/prefix check is applied.
  *Verify:* unit + assembled test supplying mixed tags incl. one empty (rejected) and one
  colon-prefixed and one bare (both stored verbatim).

- **AC-02 (whole-set-once at start via the hook path).** [assembled-path] Tags on a
  `context_cycle` **start** flow `build_cycle_event_or_fallthrough` → `RecordEvent` →
  `handle_cycle_event` into `cycle_tags` for that `feature_cycle`, written in the same transaction
  as the `cycle_start` insert. The first tag-bearing start locks the whole set via the FR-6
  EXISTS-guard. Tags on a non-start event are ignored. A re-issued start does not duplicate or
  error. No post-start mutation path exists.
  *Verify:* drive an assembled cycle start with tags, then a non-start event with tags (asserted
  absent), then a duplicate start (asserted no dup rows, no error). `proven_by` must cite the
  assembled-path test.

- **AC-02a (whole-set-once is intended: later starts are a whole-set no-op).** A re-issued start
  supplying a same/subset/superset/different tag set does not overwrite, remove, or accumulate
  tags, and does not error. The whole later write is skipped once any `cycle_tags` row exists for
  the `feature_cycle`.
  *Verify:* assembled tests —
  (a) start with `{arm:A}`, then re-start with `{arm:B}`; assert the stored set is **exactly
      `{arm:A}`** (`arm:B` ignored, no error);
  (b) start with `{A, B}`, then re-start with `{C}`; assert the stored set is **exactly `{A, B}`**
      (no accumulation, `C` ignored, no error);
  (c) a **tagless** start followed by a tag-bearing start; assert the later tags **do** lock (first
      *tags* win, not first start). (SR-05 documented behavior.)

- **AC-03 (storage substrate + `CURRENT_SCHEMA_VERSION` 30→31 cascade).** Tags are stored in
  `cycle_tags(feature_cycle, tag)` (PK `(feature_cycle, tag)`, index on `(tag)`) as the source of
  truth, added by an additive migration. This bump is verified as **discrete per-path
  line-items**, all required:
  - **AC-03a:** `CURRENT_SCHEMA_VERSION` constant advanced 30→31 (`migration.rs:26`).
  - **AC-03b:** fresh-create path creates `cycle_tags` + its `(tag)` index.
  - **AC-03c:** migration path creates `cycle_tags` on an existing DB, guarded by
    `sqlite_master`/`pragma_table_info` existence pre-check (idempotent re-run safe).
  - **AC-03d:** any pinned schema-version / migration-hygiene test is updated and passes.
  *Verify:* migration unit tests for fresh-create and upgrade-from-v30; idempotent re-run.

- **AC-04 (durability across GC — protection by omission).** `cycle_tags` is unchanged after a
  full GC pass (`gc_cycle_activity`) because it is **not in the GC delete enumeration**
  (observations → query_log → injection_log → sessions) — protection by omission, contrast the
  purged `sessions`. `cycle_tags` is added to the regression test's protected-table assertion set
  so a future GC change that starts deleting it is caught.
  *Verify:* regression test mirroring `test_gc_protected_tables_regression` — seed cycle tags,
  make the cycle purgeable (has a `cycle_review_index` row), run GC, assert `cycle_tags` rows
  intact. [SR-09]

- **AC-05 (surfaced in review + `SUMMARY_SCHEMA_VERSION` 5→6 cascade).** [assembled-path]
  `context_cycle_review` shows the run's tags in JSON (a `tags` field on `RetrospectiveReport`) and
  markdown (dedicated section or header-meta entry), populated by reading `cycle_tags` via
  `get_cycle_tags` and folded into the report so it rides `summary_json`. The bump is verified as
  **discrete per-path line-items**, all required:
  - **AC-05a:** `SUMMARY_SCHEMA_VERSION` constant advanced 5→6 (`cycle_review_index.rs:54`).
  - **AC-05b:** `RetrospectiveReport.tags` round-trips through `summary_json`
    serialize/deserialize (all three schema paths updated).
  - **AC-05c:** the pinned `SUMMARY_SCHEMA_VERSION` test (#4178, #5051) is updated and passes.
  - **AC-05d:** JSON output includes `tags`; markdown renders the tag section; a tag-less cycle
    renders no spurious section.
  *Verify:* assembled test — start a cycle with tags, run review, assert tags appear in BOTH the
  JSON and markdown outputs of `context_cycle_review` (not just a store getter). `proven_by` must
  cite the assembled-path test. [SR-08]

- **AC-06 (interface stability + authorization).** The only external change is the additive `tags`
  param on `context_cycle` (plus the additive `tags` field on review output). No new MCP tool;
  `context_tag` and `context_correct` unchanged. The write is gated by a single
  `Capability::Write`; `agent_id` is audit-only.
  *Verify:* handler-registry test (no new tool); auth test asserting Write required and agent_id
  not authorizing; diff review confirming `context_tag`/`context_correct` untouched.

- **AC-07 (prefix convention, not enforced).** A `namespace` prefix (`workflow:`, `arm:`) is
  supported by convention only — not required, derived, or validated at the cycle-tag write path;
  prefixes carry meaning only to the reader.
  *Verify:* test storing a colon-prefixed and a colon-free tag; assert both stored identically
  with no prefix-based branching.

- **AC-08 (no back-fill of historical reviews).** A cycle whose review was cached at
  `SUMMARY_SCHEMA_VERSION` 5 shows no tags; only post-deployment runs carry tags. Documented
  expectation, not a bug.
  *Verify:* documented in feature docs; optionally a test confirming a v5 cached review surfaces
  no `tags` without a recompute. [SR-10]

- **AC-09 (ack echo — BEST-EFFORT, NON-GATING).** On the existing `context_cycle` ack string:
  start-with-tags echoes `"N tags accepted at cycle start: [...]"` (accept-for-recording, NOT a
  durability guarantee); non-start-with-tags echoes `"tags ignored — only recorded at cycle
  start."`; a listener tracing log distinguishes wrote-set vs. frozen-skip. The frozen-skip case
  is operator-visible only (no caller-visible signal without a new interface — not specced).
  **This AC is best-effort and MUST NOT block a gate.**
  *Verify (best-effort):* string-assertion on the ack for a start-with-tags and a
  non-start-with-tags call. A miss here does not fail delivery.

## User / Agent Workflows

1. **Label a run at start.** An SM/agent issues `context_cycle(type=start, topic=<feature_cycle>,
   goal=…, tags=["workflow:v1.3","arm:A"], agent_id=…)`. Requires `Capability::Write`. Tags ride
   the hook path and land in `cycle_tags` in the `cycle_start` transaction — the first tag-bearing
   start locks the whole set (FR-6). The ack MAY echo `"N tags accepted at cycle start: [...]"`
   (best-effort accept-for-recording, not a durability guarantee); `cycle_review` is the
   authoritative confirmation (set-and-forget).
2. **Continue the run.** Subsequent `context_cycle` events (phase/outcome/next_phase) may or may
   not carry `tags`; if present, tags are ignored (FR-4) and the ack MAY echo `"tags ignored — only
   recorded at cycle start."` A later start re-supplying tags is also a whole-set no-op (FR-6).
3. **Read the labels.** After review, `context_cycle_review(feature_cycle)` returns the run's tags
   in JSON and markdown, read fresh from `cycle_tags` each review.
4. **External A/B analysis (out of scope).** An analyst joins the per-run labels (from `cycle_tags`
   or the `summary_json` mirror) against `cycle_review` metrics out-of-band.

## Constraints

- **Hook-path only.** The bare MCP `context_cycle` handler persists nothing; tags MUST ride
  `hook.rs` → `handle_cycle_event` (like `goal`) and be written in the `cycle_start` transaction.
  No second persistence route. [SR-03]
- **Durability placement.** Tags MUST NOT live on `sessions` (purgeable) or on the run-time-absent
  `cycle_review_index`. Source of truth is `cycle_tags`, GC-protected. [SR-09]
- **`cycle_review_index` write timing.** Written only at review time via the single writer
  `store_cycle_review()`; tags are surfaced via `RetrospectiveReport` riding `summary_json`, read
  from `cycle_tags` each review — the aggregate row is display-only.
- **Two independent version cascades.** `CURRENT_SCHEMA_VERSION` 30→31 and
  `SUMMARY_SCHEMA_VERSION` 5→6 are separate; each requires its full per-path update + pinned test
  (AC-03, AC-05). This is the recurring gate miss in this codebase (#4153, #4373). [SR-01]
- **Value-opacity.** No vocabulary/allow-list/validation beyond non-empty; no `protected_tags`
  policy; no engine-side prefix derivation or enforcement. [vnc-045 SD-8]
- **One tag model.** Reuse entry-tag opacity + the ported `add_tag`-style primitive; the future
  mutation home is reserved on the existing `context_tag` tool (not built here). [SR-06]

## Dependencies

- **vnc-045** (`context_tag`, #928) — tag-model precedent: value-opacity (SD-8), the `add_tag`-style
  `INSERT … ON CONFLICT DO NOTHING` primitive (`write.rs:281`), Capability::Write gate + agent_id
  audit-only posture (ADR-008 / SD-9). `remove`/`replace` verbs and audit-mutation shape are NOT
  used.
- **col-025** (#3396) — cycle-attribute precedent: `goal` set at start via the hook path and
  surfaced in `cycle_review` end to end (the exact path `tags` follows).
- **crt-036 retention** (`retention.rs`) — the GC-protected-table set the new store must join;
  `test_gc_protected_tables_regression` is the parity target for AC-04.
- **crt-055 / #4178** — `SUMMARY_SCHEMA_VERSION` bump discipline (three paths + pinned test).
- **nxs-008 / #360 / #373** — `entry_tags` junction model and the junction-vs-JSON rule.
- **Code touchpoints:** `unimatrix-*` — `CycleParams` (tools.rs:515-542), `build_cycle_event_or_
  fallthrough` (hook.rs:769), `handle_cycle_event` (listener.rs), `db.insert_cycle_event`
  (db.rs:320), migration (migration.rs:26,314-343), `retention.rs` protected set,
  `RetrospectiveReport` (unimatrix-observe/types.rs:382-472), `cycle_review_index.rs:54,301`,
  `retrospective.rs` (markdown render), a new `get_cycle_tags` getter + `insert_cycle_tags`
  primitive.

## NOT in Scope (explicit exclusions)

1. **Tag mutation on a cycle** — no add/remove/replace after start, no post-start editing. The
   deferred mutation home is the existing `context_tag` tool (additive, entry-defaulting option),
   **not** a new `context_cycle_tag` tool — reserved, not built.
2. **Any new MCP op or `context_cycle` interface change** beyond the additive `tags` param.
3. **Cross-cycle QUERY / SEARCH by tag** — no cycle-list/search MCP surface exists; deferred to a
   future companion issue. `cycle_tags`'s `(tag)` index is the substrate for it.
4. **Cross-run comparison / diffing / A/B aggregation** — done externally; this feature only makes
   labels storable + readable per run.
5. **Vocabulary, allow-list, length/charset validation, prefix enforcement, `protected_tags`
   policy** — value-opacity only.
6. **A pre-review cycle-tag read surface** — read-back via `context_cycle_review` only (OQ-4).
7. **Modifying entry `context_tag` / `context_correct`** — the entry path is unchanged.
8. **Trust-level / identity authorization** — `Capability::Write` only; `agent_id` audit-only.
9. **Back-fill of historical / pre-v6 cached reviews** — no recompute; historical cycles show no
   tags by design (NFR-8).

## Open Questions

None for the architect blocking spec approval. Carry-forward flags for downstream (not spec gaps):

- **For architect (from SR-07/NFR-6):** confirm the concrete mechanism by which cycle-start tags
  persist when the SM session is absent/evicted (parity with the #4136 pre-register fix) — a
  design decision for the ADR, not a spec ambiguity.
- **For implementation start (SR-02/NFR-3):** re-verify v31 and SUMMARY v6 are still the next free
  numbers at HEAD; flag renumber if a parallel feature merged first.
