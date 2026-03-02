# col-009-researcher Agent Report

## SCOPE.md

Written to: `product/features/col-009/SCOPE.md`

## Key Findings

### Table count correction
The product vision references 13 tables; the actual codebase has **14** (OBSERVATION_METRICS was added in col-002 — confirmed in `crates/unimatrix-store/src/schema.rs`). SIGNAL_QUEUE will be the **15th** table.

### col-008 SessionRegistry is complete and ready to consume
`crates/unimatrix-server/src/session.rs` fully implements `InjectionRecord`, `SessionState`, and `SessionRegistry`. col-009 can read `injection_history` directly without any structural changes to col-008's data model. The only addition needed is a `signaled_entries: HashSet<u64>` field on `SessionState` for dedup.

### SessionClose.outcome is currently None
In `hook.rs` `build_request()`, the `Stop` arm sets `outcome: None`. col-009 needs to change this to `outcome: Some("success".to_string())` for clean Stop events, with server-side rework state upgrading to "rework" when thresholds are crossed.

### PostToolUse is currently a RecordEvent passthrough
PostToolUse falls through to the default `RecordEvent` arm in `build_request()`. col-009 needs a dedicated `PostToolUse` arm that extracts rework indicators (Bash exit codes, repeated file edits) and sends structured rework-candidate events to the server.

### RetrospectiveReport extension is clean
`RetrospectiveReport` in `crates/unimatrix-observe/src/types.rs` has a predictable structure. The `entries_analysis: Option<Vec<EntryAnalysis>>` addition with `#[serde(default)]` is straightforward and backward compatible.

### Asymmetric design is non-negotiable
The "auto-positive, flag-negative, never auto-downweight" design rationale is explicit in the PRODUCT-VISION.md and ASS-014 feature-scoping.md. This is a product safety invariant, not a tuning choice. Acceptance criteria AC-06 enforces this.

### Schema migration is simpler than prior migrations
v0→v1 (nxs-004), v1→v2 (crt-001), and v2→v3 (crt-005) all required scan-and-rewrite of existing entries. Schema v4 only needs to create the SIGNAL_QUEUE table and initialize `next_signal_id = 0` — no entry rewriting needed. Simpler migration, lower risk.

## Open Questions

1. **Rework state location**: Server-side in `SessionState.rework_events` (recommended) vs. reconstructing from transcript_path on each PostToolUse hook.

2. **PostToolUse non-Bash rework indicators**: How to detect rework from Edit/Write tool calls (same file edited twice in short window) given these tools don't have exit codes.

3. **SessionClose outcome override strategy**: Whether `Stop` always sends `"success"` and the server downgrades based on rework state, or whether the hook itself should query the server before deciding the outcome field.

4. **PendingEntriesAnalysis location**: In-memory on `McpServer` struct (recommended) vs. a new redb table.

5. **Signal cap overflow policy**: Whether lost signals on overflow are acceptable given Wilson 5-vote minimum guard (recommended: yes).
