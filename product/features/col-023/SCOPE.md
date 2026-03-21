# W1-5: Observation Pipeline Generalization

## Problem Statement

The observation pipeline is hardwired to Claude Code's hook model. `HookType` is a closed
4-variant enum (`PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop`) defined in
`unimatrix-core`. The `UniversalMetrics` struct has 21 hardcoded fields that are all
Claude Code-specific (e.g., `bash_for_search_count`, `coordinator_respawn_count`,
`sleep_workaround_count`). The 21 detection rules in `unimatrix-observe` match directly
on `HookType` variants and tool names like `"Bash"`, `"Read"`, `"Write"`, `"Edit"`,
`"Grep"`, `"TaskUpdate"`.

This coupling means any non-Claude-Code event source — SRE incident streams, environmental
sensors, scientific instrument readings — cannot feed Unimatrix's retrospective intelligence
without code changes and a recompile. It also blocks W3-1 (GNN training signal pipeline),
which depends on a functioning retrospective pipeline for any domain.

Affected: any operator deploying Unimatrix outside a Claude Code workflow, and any Wave 3
work that assumes domain-neutral training labels.

## Goals

1. Replace `HookType` enum with a generic `ObservationEvent` type: `event_type: String`,
   `source_domain: String`, `payload: JsonValue`, `session_id: String`, `ts: u64`.
2. Generalize `UniversalMetrics` so dev-specific metrics become the "claude-code" domain
   pack's metrics, not hardcoded struct fields. The retrospective metric vector becomes
   configurable per-domain.
3. Rewrite all 21 detection rules to operate on the generic event schema rather than
   pattern-matching on `HookType` variants and Claude Code tool names.
4. Implement config-file-driven domain pack registration (TOML at startup) with a
   "claude-code" default pack pre-bundled so existing behavior is preserved.
5. Expose runtime re-registration for Admin callers as an override mechanism, with the
   same security constraints as other Admin-only tools.
6. Enforce security constraints on all untyped external input: payload max 64 KB,
   nesting depth ≤ 10 levels, `source_domain` validated `[a-z0-9_-]` max 64 chars,
   extraction rules sandboxed (no filesystem/env references).

## Non-Goals

- Changing the observations table schema or adding a schema migration. The `hook` column
  is already TEXT with no enum constraint in SQLite — the DB layer is already generic.
  No migration is required.
- Removing backward compatibility with existing Claude Code hook events. The "claude-code"
  default domain pack must preserve identical behavior for all existing sessions.
- Implementing domain packs for specific non-Claude-Code domains (SRE, environmental
  monitoring). This feature defines the framework and ships the "claude-code" pack only.
- Changing the wire protocol (`HookInput`, `ImplantEvent`, `HookRequest` in
  `unimatrix-engine/src/wire.rs`). `ImplantEvent` already has `event_type: String` and
  `payload: JsonValue` — the wire layer is already generic. Only the server-side processing
  and `unimatrix-observe` types need changing.
- Changing the MCP interface (`context_cycle_review`). The
  RetrospectiveReport shape is unchanged; only the internal computation becomes domain-aware.
- Implementing GNN training label generation (W3-1). This feature enables it but does not
  build it.
- Runtime domain pack hot-reload without server restart (startup config only; Admin
  override is the runtime path).

## Background Research

### Current Architecture

**HookType location**: `unimatrix-core/src/observation.rs`. Re-exported by
`unimatrix-observe/src/types.rs`. Referenced in 59 files across the workspace.

**ObservationRecord** (unimatrix-core): has `hook: HookType` as a typed field. This is
the type flowing through the entire retrospective pipeline.

**Storage layer is already generic**: The `observations` table stores `hook` as a `TEXT`
column with no enum constraint. `insert_observation` takes `hook: &str`. The DB layer
requires no changes — coupling is purely in the Rust type system above the DB.

**UniversalMetrics**: 21-field struct in `unimatrix-store/src/metrics.rs` with a
`UNIVERSAL_METRICS_FIELDS: &[&str]` const used by a structural test (R-03/C-06) that
enforces column-field alignment. The OBSERVATION_METRICS table stores each as an explicit
column. Making metrics configurable must address this test.

**Detection rule coupling**: All 4 detection rule modules (`agent.rs`, `friction.rs`,
`session.rs`, `scope.rs`) import `HookType` and match on it explicitly. Every rule is
written against Claude Code tool names and event semantics. The `DetectionRule` trait
interface (`detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>`) takes
the typed `ObservationRecord` slice — changing `ObservationRecord.hook` from `HookType`
to `String` propagates to all 21 rules.

**ImplantEvent is already generic**: `unimatrix-engine/src/wire.rs` defines
`ImplantEvent { event_type: String, session_id: String, timestamp: u64, payload: JsonValue }`.
The wire protocol does not need to change.

**parse_observation_rows** (unimatrix-server/src/services/observation.rs): Hardcoded
match converting `hook_str` to `HookType` variants, with `_ => continue` skipping unknown
types. This is the ingest point that will route to domain-appropriate processing.

**config.rs precedent**: The existing `UnimatrixConfig` / TOML loader pattern in
`unimatrix-server/src/infra/config.rs` is the exact model for domain pack config loading.
Two-level hierarchy (global + per-project), `#[serde(default)]`, replace semantics for
per-project overrides. Domain pack config should follow this exact pattern.

**ASS-022 research** confirms: the observations pipeline is the most domain-coupled
component in the system. The 21 rules and metrics were noted as
"Claude Code-specific confirmed" in ass-022/01-domain-agnosticism-assessment.md.

**DetectionRule trait** has a clean extensibility story: it is already a trait object
(`Box<dyn DetectionRule>`), and `default_rules()` returns a `Vec<Box<dyn DetectionRule>>`.
Domain packs can register their rules into this vec without changing the framework.

**Metrics storage complexity**: The OBSERVATION_METRICS table has 21 explicit typed columns
matching `UniversalMetrics` fields, enforced by a structural test. Three migration options
exist (see constraint entry #2844). The least-invasive approach is Option B: retain the
21 columns as "claude-code" defaults, add an extension JSON column for domain-specific
metrics. This preserves backward compatibility and the structural test.

**Schema version**: Currently v13 (incremented by crt-021/W1-1). This feature may need
a v14 bump only if the OBSERVATION_METRICS table is altered (Option B).

### Existing Extensibility Points

- `DetectionRule` trait is already `Box<dyn DetectionRule>` — domain rules plug in cleanly
- `ObservationSource` trait (ADR-002 col-012) keeps `unimatrix-observe` independent of store
- `config.rs` TOML loader is the precedent for startup config (two-level hierarchy, serde default)
- `UNIVERSAL_METRICS_FIELDS` const is a single-source-of-truth that must be updated if fields change

## Proposed Approach

### Phase 1: Generalize the core type

Replace `ObservationRecord.hook: HookType` with `event_type: String` and add
`source_domain: String`. Keep all other fields. Deprecate `HookType` enum (or preserve it
as a const set of well-known values for the claude-code domain pack). Update
`parse_observation_rows` to no longer filter unknown types — pass through all `event_type`
strings.

### Phase 2: Domain pack registry

Define `DomainPack` as a config-time registration:
```toml
[[observation.domain_packs]]
source_domain = "claude-code"
event_types = ["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]
categories = ["outcome", "lesson-learned", "decision", "convention",
              "pattern", "procedure", "duties", "reference"]
# extraction rules are built-in (Rust code); external packs specify rule file path
```

At startup, load domain packs from TOML config following the same two-level hierarchy as
`UnimatrixConfig`. A "claude-code" pack is always loaded as the baseline default.
Admin-level MCP call for runtime re-registration follows the same security model as
`context_enroll` (Admin capability required).

### Phase 3: Rewrite detection rules

The 21 rules are rewritten to match on `event_type: String` and `source_domain: String`
rather than `HookType` variants. For the "claude-code" domain pack, the matching values
are the existing hook names as strings — behavior is identical. The `DetectionRule` trait
signature changes from `detect(&self, records: &[ObservationRecord])` to take the
generalized record. Domain pack rules can be loaded from config-specified rule descriptors
(data-driven extraction rules) or registered as Rust `DetectionRule` implementations.

### Phase 4: Generalize UniversalMetrics

Adopt Option B: retain 21 columns as "claude-code" domain defaults, add a nullable
`domain_metrics_json TEXT` column to OBSERVATION_METRICS for extension metric key-value
pairs. `MetricVector.universal` becomes a `HashMap<String, f64>` at the logical level
(with serde compat aliases for the 21 existing field names). Schema version bumps to v14.

### Security

- Payload size check at ingest: reject payloads > 64 KB with `PayloadTooLarge` error
- Depth limit at ingest: walk JSON tree and reject nesting > 10 levels
- `source_domain` regex validation: `^[a-z0-9_-]{1,64}$` at registration and ingest
- Extraction rule sandboxing: rules are pure data transformations expressed as JSON path
  expressions, no Turing-complete evaluation, no env/fs access
- Runtime re-registration requires Admin capability (same pattern as `context_enroll`)

## Acceptance Criteria

- AC-01: `ObservationRecord.hook` field is replaced with `event_type: String` and
  `source_domain: String`. `HookType` enum is no longer used in the retrospective
  pipeline hot path.
- AC-02: All 21 detection rules compile and produce identical findings for Claude Code
  event streams after the type change (no behavioral regression). Test coverage on
  existing rule logic is preserved.
- AC-03: A domain pack registry is loaded from TOML at startup. An absent
  `[observation]` section uses the "claude-code" default pack (serde default behavior).
- AC-04: The "claude-code" default domain pack is bundled and active with no config
  required. An empty config produces identical retrospective behavior to today.
- AC-05: A synthetic non-Claude-Code event stream (e.g., `source_domain = "sre"`,
  `event_type = "incident_opened"`) can be ingested and processed through the pipeline
  without the server rejecting it or panicking. A domain-specific detection rule
  registered for "sre" fires on synthetic "sre" events.
- AC-06: Payloads exceeding 64 KB are rejected at ingest with a descriptive error.
  JSON nesting deeper than 10 levels is rejected at ingest.
- AC-07: `source_domain` values not matching `[a-z0-9_-]` max 64 chars are rejected
  at both domain pack registration and event ingest.
- AC-08: Admin-only MCP runtime re-registration of a domain pack is callable by an
  Admin agent and rejected (with `PermissionDenied`) by a non-Admin agent.
- AC-09: The `OBSERVATION_METRICS` table gains a `domain_metrics_json` column (schema
  v14). Existing rows (schema v13 and below) read back with NULL for this column, which
  deserializes as an empty map.
- AC-10: The structural test enforcing column-field alignment for `UniversalMetrics`
  (R-03/C-06) is updated to reflect the new schema without reducing coverage.
- AC-11: The `parse_observation_rows` function no longer silently drops unknown event
  types with `_ => continue`. Unknown `event_type` values from unregistered domains are
  stored and passed through with `source_domain = "unknown"`.

## Constraints

- **No wire protocol changes**: `ImplantEvent`, `HookRequest`, `HookResponse` in
  `unimatrix-engine` must not change. The wire layer is already generic.
- **No observations table migration** for the core columns: `hook` is already TEXT. Only
  OBSERVATION_METRICS gains a new nullable column (schema v14).
- **Backward compatibility**: All existing Claude Code sessions must produce identical
  retrospective output after this feature ships. The "claude-code" domain pack maps
  old `HookType` string values to the new generic `event_type` strings identically.
- **DetectionRule trait signature change is a breaking change** within the workspace:
  all 21 rule implementations plus tests in `unimatrix-observe/tests/extraction_pipeline.rs`
  must be updated in the same PR. No external consumers of this trait exist outside the
  workspace.
- **SQLite single-writer**: Domain pack loading at startup is read-only config. Runtime
  re-registration writes to an in-memory registry, not the DB. No new write contention.
- **Extraction rule sandboxing cannot use `eval` or dynamic code loading**: Rules must
  be expressed as JSON path + transform specs parsed at registration time. Turing-complete
  rule DSLs are out of scope.
- **Rayon pool**: Detection rule execution already runs on the rayon thread pool via
  `spawn_blocking`. Domain-pack-registered rules inherit the same execution context.
- **No new crate dependencies** for the sandboxed rule DSL: use `serde_json::Value`
  pointer expressions (`json_pointer`) which are already a transitive dependency.

## Resolved Design Decisions

1. **Detection rule format for external domain packs**: Both threshold rules (count of
   event type X > N) and temporal window rules (N events within T seconds) are in scope.
   The rule DSL must support both.

2. **Domain pack = event_types + categories (TOML only, no new MCP tool)**: A domain
   pack is the complete description of how a domain uses Unimatrix: what events it
   produces AND what knowledge categories it stores entries under. Both are declared in
   the same TOML stanza. `CategoryAllowlist::from_categories()` and
   `config.knowledge.categories` are already config-driven (dsn-001); adding
   `categories = [...]` to a domain pack stanza is one extra Vec<String> field at
   startup. No new MCP tool for runtime registration — config-file-driven is simpler,
   reproducible, and version-controllable. Admin runtime override (if needed) can extend
   an existing tool; a dedicated 13th tool is not warranted.

3. **`source_domain` is set server-side on the hook path**: Everything arriving via
   `unimatrix hook` is `source_domain = "claude-code"` — implicit, derived from the
   ingress path, never declared by the client. Non-Claude-Code domains feed events
   through a different future ingress (out of scope for W1-5). The hook CLI's event
   string (e.g., `"PostToolUse"`) becomes the `event_type`; the source domain is inferred
   from context.

   **Critical scope clarification**: `build_request()` routes hook events into five
   categories. Only `RecordEvent`/`RecordEvents` enter the observation pipeline.
   Session lifecycle (`SessionStart`, `Stop`, `TaskCompleted`) and synchronous injection
   (`UserPromptSubmit` → `ContextSearch`, `PreCompact` → `CompactPayload`) are
   Claude Code-specific protocol — they write responses back to stdout for Claude Code to
   consume and are **not** observation events. W1-5 generalizes only the `RecordEvent`
   path. The injection and lifecycle paths are explicitly out of scope.

4. **No OUTCOME_INDEX migration required**: `BaselineSet.universal` is already
   `HashMap<String, BaselineEntry>` — string-keyed today. Existing serialized baseline
   data in OUTCOME_INDEX naturally deserializes into a HashMap without aliases or
   migration. `MetricVector.universal: UniversalMetrics` stays as a typed struct under
   Option B (retain 21 columns + add `domain_metrics_json` extension column); no
   `MetricVector` deserialization breakage occurs.

## Tracking

https://github.com/dug-21/unimatrix/issues/331
