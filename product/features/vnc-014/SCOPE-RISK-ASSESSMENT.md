# Scope Risk Assessment: vnc-014

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `client_type_map: Arc<Mutex<HashMap<String,String>>>` is a global lock shared across all concurrent HTTP sessions. Under high concurrency (many sessions initializing simultaneously), this is a contention point on the write path. | Med | Low | Architect should evaluate whether `DashMap` or per-session state is warranted, or document the acceptable concurrency bound. |
| SR-02 | Four-column `ALTER TABLE audit_log` migration is not idempotent without pragma_table_info guards. A crash between the first ALTER and the schema version bump leaves a partially-migrated table; re-run will fail on the already-added column. (Historical evidence: entry #4092 — multi-column ordering rule confirmed in crt-043.) | High | Med | Architect must apply pragma_table_info existence check before each ALTER, and run all four checks before any ALTER executes. |
| SR-03 | rmcp `ServerHandler::initialize` override signature must exactly match the trait definition in rmcp 0.16.0. A version drift or trait signature mismatch produces a compile error that blocks the entire feature. SCOPE.md §Background confirms the override is a provided method — but the Future return type and lifetime bounds are fragile. | Med | Low | Confirm the override compiles against the exact pinned rmcp version before committing the function signature in spec. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `build_context_with_external_identity()` is scoped as a stub (always `None` for `external_identity`), but 12 tool handlers must each be migrated from `build_context()` to the new overload. This is an O(n) mechanical change across all of `tools.rs` with no functional change per site — regression risk from missed or incorrectly migrated call sites is high. | High | Med | Spec must enumerate all call sites explicitly and require a compile-time check (e.g., remove or rename `build_context()` after migration so any missed call site fails to compile rather than silently calling the old path). |
| SR-05 | `capability_used` field (AC-05) requires each tool handler to supply a string capability name constant. The scope implies each tool "knows which capability it checks" — but if the constant is free-form string rather than tied to the `Capability` enum, values will diverge across tools over time. | Med | Med | Spec should define the canonical string values (tied to or derived from the `Capability` enum) and forbid ad-hoc strings at tool sites. |
| SR-06 | Stdio transport uses `""` as the `client_type_map` key (single entry, server lifetime). This creates a correctness assumption: only one stdio session exists per server lifetime. If the server is ever used with multiple sequential stdio clients in a single process lifetime (unlikely but possible in tests), the map key will overwrite silently without error. | Low | Low | Architect should document the single-stdio-session invariant and add a debug assertion or log warning when the `""` key is overwritten. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Append-only DDL triggers (`BEFORE UPDATE` / `BEFORE DELETE` on `audit_log`) will break any existing test fixtures or test helpers that DELETE from `audit_log` for test teardown or setup. This is a structural incompatibility that must be identified before delivery begins. | High | High | Spec must enumerate all test sites that write to or truncate `audit_log` directly, and define the migration path (test DB recreation, not DELETE). |
| SR-08 | `agent_attribution` is defined as non-spoofable (§Goals Goal 3) — populated from the connection layer only, not from tool parameters. But the existing `agent_id` field is agent-declared and spoofable. The coexistence of both fields in `AuditEvent` creates a semantic ambiguity that downstream consumers (audit queries, compliance tooling in W2-3) must not conflate. | Med | Med | Architect should document the two-field attribution model clearly: `agent_id` = agent-declared identity (spoofable, for routing); `agent_attribution` = transport-attested identity (non-spoofable, for compliance). |

## Assumptions

- **§Background "rmcp 0.16.0 — initialize Hook"**: Assumes `ServerHandler::initialize` in rmcp 0.16.0 is overridable and that the default `get_info()` return is the only semantic behavior. If rmcp has additional side effects in the default implementation (session registration, capability negotiation), overriding without calling `super` could silently break session setup.
- **§Proposed Approach Step 2**: Assumes `Mcp-Session-Id` header is always present and stable for the duration of an HTTP session. If rmcp regenerates the session ID mid-session or uses a different extension type on some request paths (e.g., SSE vs POST), the header lookup falls back to `""` silently — misattributing HTTP sessions as stdio.
- **§Non-Goals**: Explicitly defers `cycle_events` gap for Codex. If ASS-050 audit attribution becomes a compliance requirement before `cycle_events` is fixed, the non-goal may be contested during delivery review.

## Design Recommendations

- **SR-02 + entry #4092**: Mandate pragma_table_info guard pattern for all four ALTER TABLE statements. Run all four column-existence checks before executing any ALTER. Do not split the migration — all four columns in one version bump (already stated in constraints).
- **SR-04**: Remove `build_context()` (or make it `#[deprecated]`) after migration so the compiler enforces complete migration. Include this as an explicit AC in the spec.
- **SR-07**: Audit all test files in `unimatrix-store/` and `unimatrix-server/` for `audit_log` DELETE usage before architecture is finalized. The trigger installation may require test infrastructure changes that affect delivery estimate.
- **SR-01**: Accept current `Mutex` for vnc-014 scope (STDIO is single-session; HTTP concurrency is low in current deployments) but document the known contention bound so W2-2 HTTP transport design can revisit.
