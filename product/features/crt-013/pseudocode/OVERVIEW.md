# crt-013 Pseudocode Overview

## Components

| Component | Crates | Purpose |
|-----------|--------|---------|
| coaccess-consolidation | unimatrix-engine, unimatrix-adapt | Remove W_COAC, co_access_affinity(), episodic.rs |
| status-penalty-validation | unimatrix-server | Integration tests for crt-010 penalties |
| briefing-config | unimatrix-server | Configurable semantic_k for BriefingService |
| status-scan-optimization | unimatrix-store, unimatrix-server | SQL aggregation replacing full table scan |

## Data Flow

No new cross-component data flow is introduced. Changes are:

1. **Subtractive** (C1): Dead code removed from engine and adapt crates. No runtime behavior change.
2. **Test-only** (C2): New test module in server crate exercising existing search pipeline.
3. **Configurational** (C3): New `semantic_k` field on `BriefingService`, read from env var at construction.
4. **Optimizational** (C4): New `StatusAggregates` struct in store crate, consumed by `StatusService` in server.

## Shared Types

```
StatusAggregates {
    supersedes_count: u64,
    superseded_by_count: u64,
    total_correction_count: u64,
    trust_source_distribution: BTreeMap<String, u64>,
    unattributed_count: u64,
}
```

Defined in `unimatrix-store`, consumed by `unimatrix-server/src/services/status.rs`.

## Sequencing Constraints

Components are independent. No build-order dependencies between them.

- C1 can be implemented first (simplest: pure deletion).
- C3 can be done independently (field + env var).
- C4 requires the new Store methods before the StatusService consumer change.
- C2 is test-only and can be done last or in parallel.

## Integration Harness Plan

Feature touches store/retrieval behavior and confidence system. Relevant integration suites:
- `smoke` (mandatory gate)
- `tools` (search behavior changes)
- `lifecycle` (status interactions)
- `confidence` (co-access boost, penalty interaction)

No new integration tests needed in infra-001 since penalty validation uses Rust-level integration tests with the full search pipeline. The search pipeline MCP-level behavior is already covered by existing suites.
