# col-022: Explicit Feature Cycle Lifecycle -- Pseudocode Overview

## Components

| Component | File | Crate | Modified Source |
|-----------|------|-------|-----------------|
| shared-validation | shared-validation.md | unimatrix-server | `src/infra/validation.rs` |
| schema-migration | schema-migration.md | unimatrix-store | `src/migration.rs`, `src/sessions.rs` |
| mcp-tool | mcp-tool.md | unimatrix-server | `src/mcp/tools.rs` |
| hook-handler | hook-handler.md | unimatrix-server | `src/uds/hook.rs` |
| uds-listener | uds-listener.md | unimatrix-server | `src/uds/listener.rs` |

## Build Order

1. **shared-validation** -- no dependencies on other col-022 components
2. **schema-migration** -- no dependencies on other col-022 components
3. **mcp-tool** -- depends on shared-validation types (`CycleType`, `ValidatedCycleParams`)
4. **hook-handler** -- depends on shared-validation (`validate_cycle_params`)
5. **uds-listener** -- depends on schema-migration (`SessionRecord.keywords`), shared-validation (event_type constants)

Components 1+2 can be built in parallel. Components 3+4 can be built in parallel after 1. Component 5 depends on 1+2.

## Data Flow

```
Agent -> MCP tool (context_cycle) -> validate params -> return acknowledgment
                                            |
Claude Code -> PreToolUse hook -> build_request detects "context_cycle"
                                   -> validate_cycle_params (shared)
                                   -> builds RecordEvent { event_type: "cycle_start", payload: {feature_cycle, keywords}, topic_signal }
                                   -> UDS fire-and-forget
                                            |
                                   Listener dispatch_request
                                   -> "cycle_start": set_feature_force + update_session_feature_cycle + update_session_keywords
                                   -> "cycle_stop": generic observation persistence only
```

## Shared Types

### New types in `unimatrix-server/src/infra/validation.rs`

```
enum CycleType { Start, Stop }

struct ValidatedCycleParams {
    cycle_type: CycleType,
    topic: String,         // sanitized, max 128 chars, valid feature ID
    keywords: Vec<String>, // max 5 items, each max 64 chars
}
```

### New types in `unimatrix-server/src/infra/session.rs`

```
enum SetFeatureResult {
    Set,
    AlreadyMatches,
    Overridden { previous: String },
}
```

### New type in `unimatrix-server/src/mcp/tools.rs`

```
struct CycleParams {
    type: String,                    // "start" or "stop"
    topic: String,                   // feature cycle ID
    keywords: Option<Vec<String>>,   // up to 5 semantic keywords
}
```

### Modified type in `unimatrix-store/src/sessions.rs`

```
struct SessionRecord {
    // ... existing 9 fields unchanged ...
    keywords: Option<String>,  // NEW: JSON array string, nullable
}
```

## Event Type Constants

Both hook-handler and uds-listener must use identical string constants. Define in a shared location (e.g., top of `validation.rs` or a new constants block):

```
const CYCLE_START_EVENT: &str = "cycle_start";
const CYCLE_STOP_EVENT: &str = "cycle_stop";
```

The specification uses "cycle_begin"/"cycle_end" in FR-14. The architecture uses "cycle_start"/"cycle_stop" in ADR-001. ADR-001 is authoritative -- use "cycle_start"/"cycle_stop".

## Cross-Crate Decision: `is_valid_feature_id`

`is_valid_feature_id` is `fn` (private) in `unimatrix-observe::attribution`. `unimatrix-server` already depends on `unimatrix-observe` (Cargo.toml line 23). Two options:

- **Re-export**: make `is_valid_feature_id` `pub` in `unimatrix-observe` and import in validation.rs
- **Duplicate**: copy the 8-line function into `validation.rs`

Recommendation: **duplicate** in `validation.rs`. The function is trivial (8 lines), and promoting a private function to pub changes the observe crate's API surface for a single consumer. The validation module already has length/character checks that overlap. Document the duplication origin.
