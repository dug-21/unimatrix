# Pseudocode Overview: bugfix-523 — Server Hardening Batch

## Components Involved

| Component File | Item(s) | Source File |
|---------------|---------|-------------|
| `nli-tick-gate.md` | Item 1 | `services/nli_detection_tick.rs` |
| `log-downgrade.md` | Item 2 | `services/nli_detection_tick.rs` |
| `nan-guards.md` | Item 3 | `infra/config.rs` |
| `session-sanitization.md` | Item 4 | `uds/listener.rs` |

Items 1 and 2 share `nli_detection_tick.rs` and MUST be assigned to the same implementation
agent (C-08 / SR-06). They are described in separate pseudocode files for clarity but land
in one diff.

## Data Flow Between Components

None. All four items are independent. No data flows between them at runtime.

```
background.rs
  └─ run_graph_inference_tick()          [nli-tick-gate.md: Item 1 gate here]
       ├─ Phase A: Informs write loop    [unconditional — unchanged]
       ├─ Path C: run_cosine_supports_path()  [unconditional — log-downgrade.md: Item 2 here]
       └─ Path B: [NEW gate] → get_provider() → rayon dispatch  [skipped when nli_enabled=false]

server startup: ServerConfig::validate()
  └─ InferenceConfig::validate()        [nan-guards.md: Item 3 — 19 guards added]

UDS socket → dispatch_request()
  └─ RecordEvent { post_tool_use_rework_candidate } arm
       ├─ capability check              [unchanged]
       ├─ sanitize_session_id()         [session-sanitization.md: Item 4 — NEW guard]
       ├─ payload extraction            [unchanged]
       └─ record_rework_event()         [unchanged]
```

## Shared Types (All Existing — None New)

| Type | Used By | Source |
|------|---------|--------|
| `InferenceConfig` | Items 1, 3 | `infra/config.rs` — `nli_enabled: bool` (Item 1), float fields (Item 3) |
| `ConfigError::NliFieldOutOfRange { path, field, value, reason }` | Item 3 | `infra/config.rs` |
| `HookResponse::Error { code: i64, message: String }` | Item 4 | `uds/listener.rs` |
| `HookEvent` (field: `session_id: String`) | Item 4 | `uds/listener.rs` |
| `ERR_INVALID_PAYLOAD` (i64 constant) | Item 4 | `uds/listener.rs` |

No new types. No new error variants. No schema changes. No Cargo.toml changes.

## Sequencing Constraints

Items 1 and 2 must be implemented in the same agent wave to avoid merge conflicts on
`nli_detection_tick.rs`. Items 3 and 4 are fully independent and can be implemented in
any order or in parallel.

## Structural Landmarks (insertion site anchors)

| Item | File | Landmark | Action |
|------|------|----------|--------|
| 1 | `nli_detection_tick.rs` | Line 555: closing `}` of `candidate_pairs.is_empty()` block; line 560: `let provider = match nli_handle.get_provider().await` | Insert new gate block between these two lines |
| 2 | `nli_detection_tick.rs` | Lines 796 and 806: `tracing::warn!` in `category_map.get(src_id)` None arm and `category_map.get(tgt_id)` None arm inside `run_cosine_supports_path` | Change `warn!` to `debug!` at both sites only |
| 3 | `infra/config.rs` | Lines 1028–1309: individual field guard `if self.<field>` statements; lines 1160–1168: `fusion_weight_checks` loop body; lines 1178–1186: `phase_weight_checks` loop body | Prefix each guard with `!v.is_finite() ||` (Group A) or `!value.is_finite() ||` (Groups B/C) |
| 4 | `uds/listener.rs` | Line 665: closing `}` of capability check block; line 666: `let tool_name = event.payload.get(...)` | Insert `sanitize_session_id` guard block between these two lines |

## Comment Update (Item 1 Side Effect)

The comment at lines 557–563 (the `get_provider()` call site) currently reads:
`"Expected when nli_enabled=false (production default)."` This must be updated to remove
the `nli_enabled=false` rationale because the explicit gate now handles that case before
reaching `get_provider()`. The comment should reflect that Err here is a transient
provider-not-ready condition only.
