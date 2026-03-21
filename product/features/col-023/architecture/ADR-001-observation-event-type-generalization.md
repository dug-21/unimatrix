## ADR-001: Replace HookType Enum with String-Based ObservationEvent Fields

### Context

`ObservationRecord.hook: HookType` is a closed 4-variant enum (`PreToolUse`, `PostToolUse`,
`SubagentStart`, `SubagentStop`) defined in `unimatrix-core/src/observation.rs` and
re-exported by `unimatrix-observe/src/types.rs`. It is referenced in approximately 25
source files across the workspace (detection rules, metrics computation, extraction rules,
session metrics, observation service, tests).

The enum is the primary coupling point that makes the observation pipeline Claude Code-specific.
Any unknown `event_type` string arriving at `parse_observation_rows` is silently dropped via
`_ => continue`. This means:
- Non-Claude-Code event sources cannot feed the pipeline without a code change
- W3-1 (GNN training signal) requires a domain-neutral pipeline for training labels
- Future domains (SRE, scientific instruments) require forking or recompiling

The DB storage layer is already generic: the `observations.hook` column is `TEXT` with no
enum constraint. The coupling exists only in the Rust type system above the DB.

The `ImplantEvent` wire type in `unimatrix-engine/src/wire.rs` is already generic:
`{ event_type: String, session_id: String, timestamp: u64, payload: JsonValue }`.

### Decision

Replace `ObservationRecord.hook: HookType` with two fields:
- `event_type: String` — the event name (e.g., `"PreToolUse"`, `"incident_opened"`)
- `source_domain: String` — the originating domain (e.g., `"claude-code"`, `"sre"`)

`HookType` is retained as a `pub mod hook_type` constants module in `unimatrix-core` for
backward-compatibility documentation, but it is no longer used in the retrospective
pipeline hot path. Detection rules, metrics computation, and extraction rules all transition
to string comparisons.

`parse_observation_rows` is changed to:
1. Always pass through any `event_type` string without filtering
2. Set `source_domain = "claude-code"` for all records arriving via the hook ingress path
3. Set `source_domain = "unknown"` for records with event types not recognized by any
   registered domain pack (used for metrics only — such records are passed to all rules,
   which must guard on `source_domain` to avoid false findings)

The `source_domain` field is set server-side at the ingest boundary, never client-declared.
Everything arriving via `unimatrix hook` is `source_domain = "claude-code"` — derived from
the ingress path, not from the payload.

The `ObservationRecord` struct in `unimatrix-core/src/observation.rs` becomes:

```rust
pub struct ObservationRecord {
    pub ts: u64,
    pub event_type: String,
    pub source_domain: String,
    pub session_id: String,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub response_size: Option<u64>,
    pub response_snippet: Option<String>,
}
```

All 25 affected callsites are updated in a single PR using wave-based compilation gates
(see ADR-004). No external consumers of `HookType` exist outside the workspace.

### Consequences

**Easier:**
- Any string-keyed event type can flow through the pipeline without a code change
- Non-Claude-Code domains can be registered via TOML config (see ADR-002)
- W3-1 training signal pipeline has a domain-neutral event substrate to build on
- Detection rules become data-driven string comparisons rather than enum match arms

**Harder:**
- All 21 detection rules, 21 metrics computations, 5 extraction rules, and associated
  tests must be updated atomically in one PR — no incremental merge path
- The struct change breaks workspace compilation until all callsites are updated;
  wave-based gating (ADR-004) is required to manage this safely
- String-based event types lose compile-time exhaustiveness checking — domain packs
  must include explicit `source_domain` guards to prevent cross-domain false findings
  (enforced as a spec-level contract per ADR-005)
