## ADR-004: Wave-Based Refactor Plan for HookType Blast Radius

### Context

SR-04 (scope risk assessment) identified that `HookType` is referenced in approximately
25 source files across the workspace. Changing `ObservationRecord.hook: HookType` to
`event_type: String` + `source_domain: String` breaks workspace compilation at the
callsite level until all consumers are updated. Without a structured plan, a partial
refactor leaves the workspace uncompilable mid-PR.

The blast radius spans four crates:
- `unimatrix-core` — defines `HookType` and `ObservationRecord` (origin point)
- `unimatrix-observe` — re-exports `HookType`, all 21 detection rules, metrics, extraction
  rules, session_metrics, report, lib, tests (heaviest consumer)
- `unimatrix-server` — `services/observation.rs`, `uds/listener.rs`, `background.rs`
- Test files — `unimatrix-observe/tests/extraction_pipeline.rs` and observation service tests

An incremental merge strategy is not possible because `ObservationRecord` is a shared
type that must be consistent across all crates simultaneously. The workspace either
compiles with the old type or the new type — there is no partial state.

### Decision

The refactor is organized into four compilation waves within a single PR. Each wave is a
checkpoint where `cargo check --workspace` passes before proceeding to the next.

**Wave 1: Core type update (unimatrix-core)**
- Replace `HookType` enum with string constants module in `unimatrix-core/src/observation.rs`
- Update `ObservationRecord` struct: remove `hook: HookType`, add `event_type: String` and
  `source_domain: String`
- `unimatrix-core` compiles clean
- All downstream crates break here — expected. Do NOT attempt compilation of workspace yet.

**Wave 2: Observe crate foundation (unimatrix-observe types + helpers)**
- Update `unimatrix-observe/src/types.rs`: update `ObservationRecord` re-export, remove
  `HookType` re-export, add `ObservationEvent` type alias if needed for clarity
- Update helper functions in `detection/mod.rs`: `make_pre()`, `make_post()`, and test
  fixtures that construct `ObservationRecord` with `HookType`
- Update `session_metrics.rs`, `attribution.rs`, `metrics.rs`, `report.rs`
- `cargo check -p unimatrix-observe` passes (excluding tests — they are Wave 4)

**Wave 3: Detection rules and metrics (unimatrix-observe internals)**
- Rewrite all 21 detection rules in `detection/{agent,friction,session,scope}.rs` to use
  `record.event_type` and `record.source_domain` string comparisons instead of
  `HookType` match arms
- Rewrite `metrics.rs` `compute_universal()` — replace `r.hook == HookType::PreToolUse`
  etc. with `r.event_type == "PreToolUse" && r.source_domain == "claude-code"` guards
- Update extraction rules: `extraction/{recurring_friction,knowledge_gap,
  implicit_convention,file_dependency,dead_knowledge}.rs`
- `cargo check -p unimatrix-observe` passes (excluding tests)

**Wave 4: Server + tests**
- Update `unimatrix-server/src/services/observation.rs` (`parse_observation_rows`):
  remove `HookType` match, set `source_domain = "claude-code"` for all hook-path records,
  construct `ObservationRecord` with new fields
- Update `unimatrix-server/src/uds/listener.rs` and `background.rs` if they reference
  `HookType` directly
- Update `unimatrix-observe/tests/extraction_pipeline.rs` test fixtures
- Update observation service tests (mock `ObservationRecord` constructors)
- `cargo test --workspace` passes — this is the merge gate

**Field mapping for claude-code backward compatibility:**
```
Old: hook = HookType::PreToolUse   → New: event_type = "PreToolUse", source_domain = "claude-code"
Old: hook = HookType::PostToolUse  → New: event_type = "PostToolUse", source_domain = "claude-code"
Old: hook = HookType::SubagentStart → New: event_type = "SubagentStart", source_domain = "claude-code"
Old: hook = HookType::SubagentStop  → New: event_type = "SubagentStop", source_domain = "claude-code"
```

Detection rules maintain identical behavior for `source_domain = "claude-code"` events
because the string values of `event_type` are identical to the previous `HookType` variant
names.

### Consequences

**Easier:**
- Each wave has a clear compilation checkpoint — implementors know when to proceed
- No incremental merge is needed: the single-PR approach avoids branch divergence
- The wave boundary at Wave 2/3 allows the observer crate to be structurally correct
  before the rule rewrite, making logic errors easier to isolate

**Harder:**
- The entire refactor must land in one PR — no cherry-picking individual waves
- Any wave that introduces a compilation error blocks all subsequent waves; the implementor
  must resolve the error before proceeding
- Integration tests that construct `ObservationRecord` directly (bypassing the hook path)
  must be updated to supply both `event_type` and `source_domain`; these are enumerated
  in the test files referenced above
