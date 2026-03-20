# Agent Report: crt-023-agent-1-architect

## Status: COMPLETE

## Artifacts Produced

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/crt-023/architecture/ARCHITECTURE.md`

### ADR Files

| File | Unimatrix Entry ID |
|------|-------------------|
| `architecture/ADR-001-nli-session-concurrency.md` | #2700 |
| `architecture/ADR-002-nli-score-replacement.md` | #2701 |
| `architecture/ADR-003-model-config-and-hashing.md` | #2702 |
| `architecture/ADR-004-post-store-embedding-handoff.md` | #2703 |
| `architecture/ADR-005-bootstrap-promotion-idempotency.md` | #2704 |
| `architecture/ADR-006-eval-cli-missing-model.md` | #2705 |

## Open Questions Resolved

**OQ-01 (Pool sizing / session concurrency):** Single `Mutex<Session>` for NLI (consistent with nxs-003 ADR-001 entry #67). Pool floor raised to 6 when `nli_enabled=true` (from existing minimum of 4 set by crt-022 ADR-003). At 20 pairs × 200ms worst case = ~4s mutex hold per search call. With 3 concurrent callers serializing, worst-case wait is ~8s, within `MCP_HANDLER_TIMEOUT`. `spawn_with_timeout` provides fallback to cosine on timeout.

**OQ-02 (Embedding handoff):** `Vec<f32>` is moved (not cloned) into the fire-and-forget task immediately after the HNSW insert step (Step 5 of the insert pipeline). No clone. If duplicate detected or embedding failed, no task is spawned. If HNSW insert fails but SQL insert succeeded, task runs but finds 0 neighbors — silent degradation.

**OQ-03 (Bootstrap promotion idempotency):** COUNTERS table string key `bootstrap_nli_promotion_done` = 1 when done. `read_counter` returns 0 for absent rows (absence = not done). `set_counter` uses `INSERT OR REPLACE` — idempotent. Marker set in same transaction as final batch of edge operations. Confirmed correct: `counters.rs` has TEXT PRIMARY KEY + u64 value — exactly fits this use case.

**OQ-04 (Eval CLI missing model):** Skip the profile with a SKIPPED annotation in the eval report. `EvalServiceLayer` waits up to 60s for NLI readiness before beginning scenario execution. If Failed or timeout → profile SKIPPED, eval run continues. Baseline profile (NLI disabled) always runs. No new CLI flags needed.

**OQ-05 (Deberta ONNX availability):** `NliDebertaV3Small` variant implemented unconditionally in the enum. ONNX availability for `cross-encoder/nli-deberta-v3-small` is NOT confirmed — DeBERTa-v3's disentangled attention requires specific ONNX export flags and not all HuggingFace repos include pre-exported files. Implementer MUST verify at download time. If unavailable: 3-profile eval degrades to 2-profile; document in delivery report. `onnx_filename()` returns `"model.onnx"` as best-effort; confirm actual filename at implementation.

## Key Design Decisions

1. **Single `Mutex<Session>` + pool floor 6** — SR-02/SR-03 addressed; work-stealing handles burst; fallback covers worst case
2. **Pure NLI entailment replacement** (D-02) — clean semantics; eval gate validates; `nli_enabled=false` rollback
3. **Config-string model + per-file hash pinning** (D-03) — `nli_model_name = "minilm2"|"deberta"`; one hash per config file
4. **Move semantics for embedding handoff** (SR-09/OQ-02) — zero-copy; hand-off after HNSW insert (Step 5)
5. **COUNTERS key for bootstrap idempotency** (SR-07/OQ-03) — existing table, existing helpers, same transaction as edge ops
6. **Skip-not-fail for eval missing model** (SR-08/OQ-04) — 60s wait window covers cached models; SKIPPED annotation is honest
7. **NLI edge writes via `write_pool_server()` directly** — `AnalyticsWrite::GraphEdge` shed policy prohibits NLI-confirmed writes via analytics queue (SR-02, already documented in analytics.rs)

## New Components

- `unimatrix-embed/src/cross_encoder.rs` — `CrossEncoderProvider` trait, `NliScores`, `NliProvider`
- `unimatrix-server/src/infra/nli_handle.rs` — `NliServiceHandle` state machine
- `unimatrix-server/src/services/nli_detection.rs` — `run_post_store_nli`, `run_bootstrap_promotion`

## Modified Components

- `unimatrix-embed/src/model.rs` — `NliModel` enum
- `unimatrix-embed/src/download.rs` — `ensure_nli_model`
- `unimatrix-server/src/infra/config.rs` — 9 NLI fields + pool floor override
- `unimatrix-server/src/error.rs` — `NliNotReady`, `NliFailed` variants
- `unimatrix-server/src/services/search.rs` — `nli_handle` field, modified pipeline
- `unimatrix-server/src/services/store_ops.rs` — `nli_handle` field, fire-and-forget spawn
- `unimatrix-server/src/services/background.rs` — bootstrap promotion task call
- Server startup wiring + `EvalServiceLayer::from_profile()` stub
