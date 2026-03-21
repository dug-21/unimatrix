# SPECIFICATION: col-023 — W1-5 Observation Pipeline Generalization

## Objective

Replace the Claude Code-hardwired observation pipeline with a domain-agnostic event
processing framework. The `HookType` enum and the 21 Claude Code-specific detection
rules are replaced by a configurable domain pack system that loads at startup from TOML
and preserves identical retrospective behavior for all existing Claude Code sessions.
This unblocks W3-1 (GNN training signal) which requires a functioning retrospective
pipeline for any domain, not just Claude Code.

---

## Functional Requirements

### FR-01: ObservationRecord Type Replacement

**FR-01.1** `ObservationRecord` in `unimatrix-core/src/observation.rs` shall replace
the field `hook: HookType` with two fields: `event_type: String` and
`source_domain: String`.

**FR-01.2** `HookType` enum shall be preserved as a module-level constant set of
well-known string values for the "claude-code" domain pack, but shall no longer be
used as a field type anywhere in the retrospective pipeline hot path.

**FR-01.3** All existing fields of `ObservationRecord` (`ts`, `session_id`, `tool`,
`input`, `response_size`, `response_snippet`) shall be preserved unchanged.

**FR-01.4** `ParsedSession`, `ObservationStats` structs in `unimatrix-core/src/observation.rs`
shall require no structural changes beyond those implied by FR-01.1.

### FR-02: Domain Pack Registry

**FR-02.1** A `DomainPack` configuration type shall be defined with these fields:
- `source_domain: String` — identifies the domain; must match `^[a-z0-9_-]{1,64}$`
- `event_types: Vec<String>` — the known event type strings for this domain
- `categories: Vec<String>` — knowledge categories this domain may store entries under
- Detection rules are registered via Rust `DetectionRule` trait implementations, not
  TOML. External (non-built-in) packs declare rules as data-driven descriptors
  (see FR-04).

**FR-02.2** A `DomainPackRegistry` shall hold the registered packs as an in-memory
structure (`Arc<RwLock<_>>`) loaded at server startup. It is not persisted to the
database.

**FR-02.3** The registry shall be initialized at server startup from the TOML config
section `[observation]`, following the same two-level hierarchy (global + per-project
overrides) as the existing `UnimatrixConfig` / `config.rs` pattern.

**FR-02.4** If the `[observation]` section is absent from the TOML config, the registry
shall default to containing only the "claude-code" domain pack (`#[serde(default)]`
behavior). No config is required for existing deployments.

**FR-02.5** The "claude-code" default domain pack shall be bundled in Rust code and
always present as the registry baseline, regardless of config. A config-supplied
"claude-code" pack entry merges with or replaces the bundled default per the existing
two-level hierarchy replace semantics.

**FR-02.6** The categories declared in a domain pack's `categories` field shall be
registered into the `CategoryAllowlist` at startup via `CategoryAllowlist::from_categories()`
or `add_category()` calls, making them valid targets for `context_store` for that
domain's agents.

**FR-02.7** The domain pack registry shall expose a method to look up the registered
pack for a given `source_domain` string, returning `None` for unregistered domains.

### FR-03: Event Ingest Path

**FR-03.1** `parse_observation_rows` in `unimatrix-server/src/services/observation.rs`
shall no longer match `hook_str` against `HookType` variants and shall no longer use
`_ => continue` to drop unknown event types.

**FR-03.2** Events with `event_type` values not in any registered domain pack's
`event_types` list shall be stored and passed through to the detection pipeline with
`source_domain = "unknown"`. They shall not be dropped.

**FR-03.3** Events arriving via the `unimatrix hook` CLI ingress path shall always be
assigned `source_domain = "claude-code"` server-side. The client does not declare its
domain; domain assignment is inferred from the ingress path.

**FR-03.4** Payload size shall be enforced at ingest: payloads exceeding 64 KB shall
be rejected with a `PayloadTooLarge` error before any further processing.

**FR-03.5** JSON nesting depth shall be enforced at ingest: payloads with nesting
exceeding 10 levels shall be rejected with a `NestingTooDeep` error.

**FR-03.6** `source_domain` values shall be validated at both domain pack registration
and event ingest against the regex `^[a-z0-9_-]{1,64}$`. Invalid values are rejected
with a `InvalidSourceDomain` error.

**FR-03.7** The session lifecycle and synchronous injection paths (`SessionStart`,
`Stop`, `TaskCompleted`, `UserPromptSubmit` → `ContextSearch`, `PreCompact` →
`CompactPayload`) are not observation events and shall not be modified by this feature.
Only the `RecordEvent` / `RecordEvents` ingest path is in scope.

### FR-04: Detection Rule Generalization

**FR-04.1** The `DetectionRule` trait signature in `unimatrix-observe/src/detection/mod.rs`
shall remain `detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>`. The
trait signature does not change — only the `ObservationRecord` type changes (FR-01.1).

**FR-04.2** All 21 existing detection rules shall be rewritten to match on
`event_type: String` and `source_domain: String` rather than `HookType` variants.
For the "claude-code" pack, matching string values shall be the existing hook names
(`"PreToolUse"`, `"PostToolUse"`, `"SubagentStart"`, `"SubagentStop"`), preserving
identical detection behavior.

**FR-04.3** Every detection rule that is domain-specific (i.e., written for a specific
domain) SHALL include an explicit `source_domain` guard as the first filter condition.
A rule written for the "claude-code" domain must not fire on records where
`source_domain != "claude-code"`. This is a mandatory implementation contract, not an
optimization.

**FR-04.4** Rules that operate on domain-neutral fields only (`ts`, `session_id`)
may omit the `source_domain` guard but must be documented as domain-neutral in their
module docstring.

**FR-04.5** The data-driven rule DSL for external domain packs shall support exactly
two operator types:
- **Threshold rule**: `count(event_type = X) > N` — fires when the count of records
  with a given `event_type` exceeds threshold N within the session.
- **Temporal window rule**: `count(event_type = X) > N within T seconds` — fires when
  N occurrences of event type X appear within a sliding window of T seconds.

  The DSL is expressed as TOML rule descriptor structs parsed at registration time.
  Rule evaluation is implemented as a host-side `RuleDslEvaluator` struct that iterates
  over `ObservationRecord` slices — no dynamic code loading, no `eval`, no filesystem
  or environment access. JSON pointer expressions (`serde_json::Value::pointer`) may
  be used for payload field extraction within a rule descriptor.

**FR-04.6** Built-in "claude-code" rules are registered as Rust `DetectionRule`
implementations, not as DSL descriptors. The DSL path is for external domain packs only.

**FR-04.7** `default_rules()` in `unimatrix-observe/src/detection/mod.rs` shall
continue to return a `Vec<Box<dyn DetectionRule>>` of 21 rules for the "claude-code"
domain pack. Domain packs may register additional rules that are appended to this
vec at runtime; they do not replace the claude-code rules.

### FR-05: UniversalMetrics Extension (Option B)

**FR-05.1** `UniversalMetrics` typed struct in `unimatrix-store/src/metrics.rs` shall
remain the canonical source of truth for the 21 Claude Code metric fields. It is NOT
converted to `HashMap<String, f64>` as a storage type. `MetricVector.universal` remains
`UniversalMetrics`.

**FR-05.2** A `domain_metrics_json TEXT` column shall be added to the
`OBSERVATION_METRICS` table as a nullable extension column. This is the only schema
change. The schema version shall bump from v13 to v14.

**FR-05.3** When writing metrics for a "claude-code" session, `domain_metrics_json`
shall be NULL. When writing metrics for any other domain, the domain-specific key-value
pairs shall be serialized as a JSON object into `domain_metrics_json`.

**FR-05.4** When reading `OBSERVATION_METRICS` rows from schema v13 (where the column
does not exist), the `domain_metrics_json` column shall deserialize as NULL, which the
application shall treat as an empty map `{}`. No separate migration logic is required
for existing rows.

**FR-05.5** The `UNIVERSAL_METRICS_FIELDS` const in `unimatrix-store/src/metrics.rs`
shall be updated to include `domain_metrics_json` as the 22nd entry. The structural
test (R-03/C-06) that enforces column-field alignment shall be updated to expect 22
entries without reducing its coverage of the 21 existing fields.

**FR-05.6** `BaselineSet.universal` is already `HashMap<String, BaselineEntry>` (string-keyed).
No change is required to `BaselineSet` or `OUTCOME_INDEX`. The existing serialized
baseline data deserializes into `HashMap` without aliases or migration.

<!-- FR-06 (Admin Runtime Override) removed: ADR-002 decision, human-confirmed.
     Domain pack registration is config-file-driven only. No runtime re-registration
     in W1-5 scope. -->

---

## Non-Functional Requirements

**NFR-01: Backward Compatibility**
All existing Claude Code sessions must produce identical retrospective output after
this feature ships. The "claude-code" domain pack must map old `HookType` string values
(`"PreToolUse"`, `"PostToolUse"`, `"SubagentStart"`, `"SubagentStop"`) to the new
generic `event_type` strings identically. Zero behavioral regression is required.

**NFR-02: Security — Payload Limits**
Payloads exceeding 64 KB must be rejected at ingest. JSON nesting deeper than 10 levels
must be rejected at ingest. Both checks must run before any deserialization into domain
types. Measured: rejection occurs at ingest, no panics, verified by AC-06.

**NFR-03: Security — Domain Validation**
`source_domain` must match `^[a-z0-9_-]{1,64}$` at both registration and ingest.
Invalid values are rejected with an error; they do not silently truncate, coerce, or
pass through.

**NFR-04: Security — Rule Sandboxing**
Data-driven detection rules (DSL) must be pure data transformations. No filesystem
access, no environment variable access, no Turing-complete evaluation. Rules are
expressed as TOML descriptors using threshold and temporal window operators only.
Implementation uses existing `serde_json::Value::pointer` for payload field extraction
with a custom `RuleDslEvaluator` host for temporal aggregation.

**NFR-05: No New Crate Dependencies**
No new crate dependencies are introduced for the rule DSL implementation. The
`serde_json::Value::pointer` method (already a transitive dependency) plus a new
`RuleDslEvaluator` struct in the workspace are sufficient. `json_pointer` alone is
insufficient for temporal window rules; the evaluator struct is the extension.

**NFR-06: No Wire Protocol Changes**
`ImplantEvent`, `HookRequest`, `HookResponse` in `unimatrix-engine/src/wire.rs` must
not change. The wire layer already has `event_type: String` and `payload: JsonValue`.

**NFR-07: No Observations Table Migration**
The `observations` table `hook` column is already TEXT with no enum constraint. No
migration is required for this table's core columns.

**NFR-08: Schema Version v14**
Only the `OBSERVATION_METRICS` table changes (adds `domain_metrics_json TEXT`). The
schema version bumps from v13 to v14. All other tables are unchanged.

**NFR-09: Compilation Gate Compliance**
The `HookType` enum is referenced in approximately 59 files. The refactor must proceed
in waves with compilation gates between each wave (pattern entry #377). Every wave must
compile cleanly before the next wave begins. No intermediate non-compiling states may
be committed.

**NFR-10: Detection Rule Thread Safety**
Domain-pack-registered rules inherit the existing rayon thread pool execution context
via `spawn_blocking`. All `DetectionRule` implementations must be `Send` (already
required by the trait). No new synchronization primitives are needed in the rule
implementations themselves.

**NFR-11: Effort Envelope**
Target implementation time is 5-7 days. Detection rule rewrite is identified as the
long tail. Architecture wave planning must bound this within the effort envelope.

---

## Acceptance Criteria

**AC-01** (Verification: compile + grep)
`ObservationRecord.hook: HookType` is replaced with `event_type: String` and
`source_domain: String`. `HookType` as a field type no longer appears anywhere in
the retrospective pipeline hot path. Verified by: `grep -r "hook: HookType"` returns
no matches outside deprecated/const contexts; workspace compiles cleanly.

**AC-02** (Verification: existing test suite passes unchanged)
All 21 detection rules compile and produce identical findings for Claude Code event
streams after the type change. No existing rule test is deleted or weakened. Verified
by: the full test suite passes with no regressions. The test count for
`unimatrix-observe` does not decrease.

**AC-03** (Verification: integration test with absent `[observation]` section)
A server started with a config that has no `[observation]` section loads the
"claude-code" default domain pack automatically. `serde` default deserialization applies.
Verified by: integration test starts server without `[observation]` config and confirms
"claude-code" pack is registered and events are processed.

**AC-04** (Verification: end-to-end retrospective regression test)
An empty config (no `[observation]` section) produces identical `RetrospectiveReport`
output for a fixed Claude Code session fixture as the current codebase. Verified by:
snapshot test comparing report output before and after the feature ships.

**AC-05** (Verification: synthetic multi-domain integration test)
A synthetic event stream with `source_domain = "sre"` and `event_type = "incident_opened"`
can be ingested via the pipeline without the server rejecting it or panicking. A
domain-specific detection rule registered for "sre" fires on synthetic "sre" events
and produces a `HotspotFinding`. "claude-code" rules do not fire on "sre" events
(source_domain guard verified). This test is sufficient for W3-1's unblocking gate —
W3-1 requires only that the pipeline accepts multi-domain events and that detection
rules gate correctly on `source_domain`; W3-1 does not require multi-domain production
rules to be pre-built.

**AC-06** (Verification: unit tests for ingest validation)
Payloads exceeding 64 KB are rejected at ingest with a `PayloadTooLarge` error.
JSON nesting deeper than 10 levels is rejected with a `NestingTooDeep` error.
Verified by: unit tests covering both boundary conditions (exactly 64 KB passes,
64 KB + 1 byte rejects; depth 10 passes, depth 11 rejects).

**AC-07** (Verification: unit tests for domain validation)
`source_domain` values not matching `^[a-z0-9_-]{1,64}$` are rejected at both
domain pack registration and event ingest with `InvalidSourceDomain`. Test cases:
empty string, string with uppercase letters, string with spaces, string exceeding
64 characters, string with special characters outside the allowed set.

<!-- AC-08 removed: FR-06 removed from scope (ADR-002, human-confirmed config-only). -->

**AC-09** (Verification: schema migration test + read-back test)
The `OBSERVATION_METRICS` table has a `domain_metrics_json TEXT` column after startup
with a fresh database (schema v14). Rows inserted at schema v13 (simulated by omitting
the column in a test fixture) read back with NULL for `domain_metrics_json`, which
deserializes as an empty map. Verified by: migration test in
`unimatrix-store/src/migration.rs` test suite.

**AC-10** (Verification: structural test updated and passing)
The structural test (R-03/C-06) enforcing column-field alignment for `UniversalMetrics`
is updated to expect 22 entries (21 original + `domain_metrics_json`). The test
continues to verify each of the original 21 column names. Verified by: test passes;
`UNIVERSAL_METRICS_FIELDS` has 22 entries.

**AC-11** (Verification: integration test for unknown event passthrough)
The `parse_observation_rows` function no longer drops unknown event types. Records with
`event_type` values not in any registered pack pass through with `source_domain = "unknown"`.
Verified by: integration test inserts a record with an unregistered `event_type` string
and confirms it appears in the pipeline output with `source_domain = "unknown"`.

---

## Domain Models

**ObservationRecord**
A normalized event from any domain. Carries `event_type: String` (what happened),
`source_domain: String` (which domain produced it), `session_id: String` (groups events
into sessions), `ts: u64` (epoch milliseconds), plus optional tool-call fields
(`tool`, `input`, `response_size`, `response_snippet`) that are Claude Code-specific
and may be `None` for non-Claude-Code domains.

**DomainPack**
The complete description of how one domain uses Unimatrix's observation pipeline.
Contains: `source_domain` (identity), `event_types` (known events), `categories`
(knowledge categories for this domain's agents), and detection rules (Rust
`DetectionRule` implementations for built-in packs; DSL descriptors for external packs).
Registered at startup; may be overridden at runtime by an Admin caller.

**DomainPackRegistry**
In-memory `Arc<RwLock<HashMap<String, DomainPack>>>` keyed by `source_domain`. Populated
at server startup from TOML config. Contains the "claude-code" built-in pack always.
Never persisted to the database.

**DetectionRule**
Trait object (`Box<dyn DetectionRule + Send>`) that inspects a `&[ObservationRecord]`
slice and returns `Vec<HotspotFinding>`. Domain-specific rules guard on `source_domain`
as their first filter. Domain-neutral rules operate only on `ts` and `session_id`.

**RuleDslEvaluator**
Host struct that evaluates a TOML-defined rule descriptor against an `ObservationRecord`
slice. Supports threshold operator (count > N) and temporal window operator
(count > N within T seconds). Implements `DetectionRule` trait. No dynamic code loading.

**HookType Well-Known Values**
The four existing variant names (`"PreToolUse"`, `"PostToolUse"`, `"SubagentStart"`,
`"SubagentStop"`) are preserved as `pub const` string values in `unimatrix-core` for use
by the "claude-code" domain pack rules. They are not an enum type; they are string
constants.

**event_type**
A free-form string identifying what happened within a domain session. For "claude-code":
`"PreToolUse"`, `"PostToolUse"`, `"SubagentStart"`, `"SubagentStop"`. For other domains:
domain-defined strings (e.g., `"incident_opened"`, `"sensor_reading"`, `"review_complete"`).

**source_domain**
A string matching `^[a-z0-9_-]{1,64}$` that identifies the origin domain of an
observation event. Set server-side at ingest from the ingress path, not declared by
the client. The string `"unknown"` is reserved for events whose `event_type` does not
match any registered pack.

---

## User Workflows

### Workflow 1: Existing Claude Code Operator (Zero Config Change)
1. Operator has a running Unimatrix server with no `[observation]` config.
2. Claude Code hooks fire `PreToolUse`, `PostToolUse`, etc. as today.
3. Server assigns `source_domain = "claude-code"` from the hook ingress path.
4. `parse_observation_rows` passes records through as `ObservationRecord` with
   `event_type = "PreToolUse"` etc. and `source_domain = "claude-code"`.
5. All 21 "claude-code" rules run, gate on `source_domain`, and fire identically to today.
6. `context_cycle_review` returns an identical `RetrospectiveReport` as before the feature.

### Workflow 2: Operator Adding a New Domain Pack
1. Operator adds `[[observation.domain_packs]]` stanza to TOML config with
   `source_domain = "sre"`, `event_types = ["incident_opened", ...]`,
   `categories = ["runbook", "post-mortem"]`, and a path to a rule descriptor file.
2. Server restarts and loads the "sre" domain pack into the `DomainPackRegistry`.
3. `CategoryAllowlist` is updated to include `"runbook"` and `"post-mortem"`.
4. Incoming "sre" events are ingested, stored, and processed by the "sre" detection rules.
5. "claude-code" rules do not fire on "sre" events.

<!-- Workflow 3 (Admin Runtime Pack Override) removed: FR-06 removed from scope. -->

---

## Constraints

**C-01** No wire protocol changes. `ImplantEvent`, `HookRequest`, `HookResponse` in
`unimatrix-engine/src/wire.rs` are frozen for this feature.

**C-02** No `observations` table migration. The `hook` column is already TEXT with no
constraint. Only `OBSERVATION_METRICS` gains a new nullable column.

**C-03** `UniversalMetrics` typed struct is the canonical representation (Option A
from scope risk SR-02 is rejected here). Two live representations of the 21 fields
must not be created. `MetricVector.universal` stays `UniversalMetrics`. Domain
extension metrics use `domain_metrics_json` as a side-channel only.

**C-04** No runtime domain pack registration. Domain pack changes require a server
restart with an updated config file.

**C-05** Extraction rule DSL is not Turing-complete. Only threshold and temporal window
operators are in scope (SR-01 resolution). No `eval`, no script files, no dynamic
loading. The constraint is: if a rule cannot be expressed as threshold or temporal
window over `event_type` counts (with optional JSON pointer payload extraction), it
must be implemented as a Rust `DetectionRule`, not as a DSL descriptor.

**C-06** All 21 rule implementations plus tests in `unimatrix-observe/tests/` must be
updated in the same PR. No external consumers of `DetectionRule` exist outside the
workspace. The `DetectionRule` trait change is a workspace-internal breaking change.

**C-07** Detection rules for domain X must never fire on events from domain Y. The
`source_domain` guard is an explicit requirement, not an optimization. Cross-domain
false findings are silent and difficult to diagnose post-merge (SR-07).

**C-08** `serde_json::Value::pointer` is the only new JSON extraction mechanism
permitted. No additional crate dependencies for the DSL evaluator.

**C-09** The `DomainPackRegistry` write path (runtime Admin override) writes to
in-memory state only. No new database write contention is introduced. SQLite
single-writer constraint is unaffected.

**C-10** The "claude-code" domain pack categories shall include all 8 existing
`INITIAL_CATEGORIES` from `CategoryAllowlist` to ensure zero regression in
knowledge storage for existing Claude Code sessions.

---

## Dependencies

**Crates (internal)**
- `unimatrix-core` — `ObservationRecord`, `ParsedSession` (modified in FR-01)
- `unimatrix-store` — `UniversalMetrics`, `MetricVector`, `UNIVERSAL_METRICS_FIELDS`,
  migration system (modified in FR-05)
- `unimatrix-observe` — `DetectionRule` trait, all 21 rule modules, `default_rules()`
  (modified in FR-04)
- `unimatrix-server` — `parse_observation_rows`, `config.rs`, `categories.rs`,
  `DomainPackRegistry` host (modified in FR-02, FR-03)

**Crates (external, existing transitive dependencies — no new additions)**
- `serde` / `serde_json` — TOML deserialization for domain pack config; JSON pointer
  extraction for DSL rules
- `toml` — already used by `config.rs`
- `tokio` — async server runtime (unchanged)
- `rayon` — detection rule execution context (unchanged)

**External Services**
None. This feature is entirely server-side.

**Existing Components Referenced**
- `CategoryAllowlist::from_categories()` / `add_category()` — domain pack category
  registration (dsn-001 pattern, no changes to `CategoryAllowlist` itself)
- `context_enroll` Admin trust level check — reference pattern (not used in this feature)
- `UnimatrixConfig` two-level hierarchy — model for domain pack config loading
- `OUTCOME_INDEX` / `BaselineSet.universal: HashMap<String, BaselineEntry>` — no
  changes required (FR-05.6)

**Blocked Features**
- W3-1 (GNN training signal pipeline) — requires this feature's AC-05 to be satisfied.
  Specifically: W3-1 requires that the pipeline accepts multi-domain events and that
  detection rules gate correctly on `source_domain`. W3-1 does NOT require pre-built
  multi-domain production detection rules.

---

## NOT in Scope

- Changing `ImplantEvent`, `HookRequest`, `HookResponse` in `unimatrix-engine`.
- Removing or migrating the `observations` table `hook` column.
- Implementing domain packs for specific non-Claude-Code domains (SRE, environmental
  monitoring, scientific instruments). Only the "claude-code" built-in pack ships.
- Adding a new 13th MCP tool for domain pack registration. Runtime override extends
  an existing tool.
- `RetrospectiveReport` wire shape changes. The `context_cycle_review` MCP tool
  interface is unchanged.
- W3-1 GNN training label generation. This feature enables it but does not build it.
- Runtime domain pack hot-reload without server restart (config path requires restart).
- Changing session lifecycle paths (`SessionStart`, `Stop`, `TaskCompleted`) or
  synchronous injection paths (`UserPromptSubmit`, `PreCompact`).
- Database migrations for `OUTCOME_INDEX` or `BaselineSet`.
- Confidence system changes (lambda, weights, Wilson score).
- Multiple concurrent schema migrations. Schema v14 covers only `OBSERVATION_METRICS`.

---

## Open Questions

<!-- OQ-01 resolved: FR-06 removed from scope (ADR-002, human-confirmed config-only).
     No Admin runtime override tool needed in W1-5. -->

**OQ-02 (Architect)** Wave partitioning for the HookType refactor blast radius
(NFR-09): Entry #2843 identifies ~59 files. The architect must enumerate all callsites,
partition into compilation-gated waves, and confirm every callsite is within the PR
boundary before wave 1 begins.

**OQ-03 (Architect)** Confirm that `BaselineSet.universal` serialized key strings in
existing OUTCOME_INDEX rows match the expected HashMap deserialization keys. If any
deployment has baseline data written with the typed struct field names as keys, the
deserialization will work (field names = HashMap keys for serde) — but the architect
should verify this assumption against a test database row before committing to no
migration (SR-06).

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for observation pipeline domain generalization — found
  entry #2843 (HookType blast radius, 59 files), entry #2844 (UniversalMetrics migration
  options), entry #2902 (DSL expressiveness gap pattern), entry #377 (wave-based
  refactoring with compilation gates). These materially shaped the constraint and
  NFR sections.
- Queried: /uni-query-patterns for detection rule source domain guard — found
  entry #261 (AuditSource-driven behavior differentiation) confirming the cross-domain
  guard as a security pattern. Shaped AC-05 and C-07.
- Queried: /uni-query-patterns for UniversalMetrics representation — found entry #632
  (ADR-001: MetricVector types), entry #2844 (migration complexity). Confirmed Option B
  (retain typed struct + extension column) is the only viable path; resolved SR-02 in
  C-03.
