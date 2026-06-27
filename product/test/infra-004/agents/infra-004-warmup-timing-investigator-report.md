# Bug Investigation Report: infra-004-warmup-timing-investigator

## Bug Summary
infra-004's warmup barrier writes a throwaway OBSERVE marker and polls
`write_then_barrier observe` for durability within `WARMUP_DEADLINE_SECS` (default
180, derived from #767). On AC-11 cold-model dispatch run **28296294623** the
warmup observe write was accepted (204) but the own marker was **not present within
180s ⇒ INFRA**. A prior run (28292626314) had the same observe warmup write become
PRESENT in <1s. The task was to measure cold-model readiness, decide whether 180s
is too tight, and decide whether observe-durability is the right readiness signal.

## TL;DR — the deadline was never the problem
1. **Cold model load is ~3–4s, MCP round trip ready ~5s from boot — nowhere near
   180s.** The 180s bound has enormous margin.
2. **The warmup observe write CANNOT exercise the embedding model at all** — the
   observe path has zero embed dependency (fire-and-forget SQL insert). So
   observe-durability is the *wrong* proxy for "embedding warm".
3. **The real cause of run 28296294623 is a marker-shape regression, not timing.**
   The #859 PII-safe nonce removed the all-digit hyphen-segment that the server's
   intentional `looks_like_feature_id` filter (bugfix-832) requires to persist
   `topic_signal`. The marker is silently dropped to `NULL`, so the observe
   read-as-barrier can never find it → deterministic INFRA timeout at *any*
   deadline. Raising 180s fixes nothing.

## Measurement Method & Environment
- Image: **`ghcr.io/dug-21/unimatrix:latest-arm64`** (built 2026-06-27T15:00:58Z).
  The sandbox is `aarch64`, so the amd64 image cannot run here; arm64 is the
  byte-equivalent multi-arch sibling (same approach the prior infra-004 investigator
  used). Docker 29.5.2, 23Gi RAM (9.6Gi free), HF + GHCR reachable.
- Reproduced the smoke's cold path: fresh data + **fresh `/shared` volume per run**
  (no warm model cache), `project register arch-research`, boot with
  `UNIMATRIX_HTTP_ENABLED=true`, `UNIMATRIX_MODEL_CACHE=/shared/models`.
- Two clean cold boots; plus a live-container marker-shape matrix and a focused
  source/code-path trace (observe vs MCP vs embed) and `git`-backed confirmation
  of the `looks_like_feature_id` filter.

## Measured Numbers (cold boot, no model cache)
| Signal | Cold run 1 | Cold run 2 |
|---|---|---|
| HTTP transport active | +1s | +1s |
| **Embedding model downloaded + loaded** (`embedding model loaded successfully`) | **+3.4s** | **+4s** |
| **MCP `context_store`→`context_search` round trip ready** | works (model up ~3.4s) | **+5s from boot** |
| Observe-write durability via `topic_signal` (current #859 marker) | **never (NULL, deterministic)** | **never (NULL)** |
| Model files on `/shared` | `model.onnx` 90,405,214 B + `tokenizer.json` 466,247 B | same |

Model was genuinely **cold-downloaded** to `/shared/models` (fresh volume, 90 MB
ONNX) — not baked into the image. Cold download+load = ~3–4s on this sandbox's fast
HF link. (#767 budgets a 10s/20s/40s ≈70s retry/backoff window for a throttled CI
HF link; even fully retried that is <<180s.)

## Root Cause Analysis

### Why observe-durability is the wrong signal (semantic defect)
The observe write path has **zero embedding dependency** and returns 204 *before*
the row is durable:
```
POST /v1/{slug}/observe → route_observe (http/router/handlers.rs:40)
  → dispatch_request (handlers.rs:150) → HookRequest::RecordEvent (uds/listener.rs:999)
  → apply_stamp_to_row → spawn_blocking_fire_and_forget (listener.rs:1157-1158)
  → insert_observation (listener.rs:3330)  [raw SQL INSERT, NO embed call]
  → HookResponse::Ack → HTTP 204 (http/router/observe.rs:65-72)
```
`context_store` (the MCP path) is the only write that requires the model
(`store_ops.rs:131-133` `embed_service.get_adapter().await`, returns
`EmbedNotReady`/`EmbedFailed` while loading). So an observe barrier proves SQLite
durability of the `observations` table — **it does not prove "embedding path warm"**,
which is the warmup barrier's stated purpose (ADR-001 #5349).

### Why run 28296294623 timed out (mechanical defect — the actual cause)
The server persists `topic_signal` only if it passes `looks_like_feature_id`
(intentional, bugfix-832):
```
fn looks_like_feature_id(value: &str) -> bool {   // uds/listener.rs:289-303
    // splits on '-'; requires >=1 ALL-DIGIT segment AND >=1 alpha segment
}
```
Reached for both HTTP and UDS via `enrich_topic_signal_with_source`
(listener.rs:196-244, gate at :233) ← `apply_stamp_to_row` (:355-356). A marker
that fails this is set to `NULL` (`topic_source=NULL`).

The #859 construction-safe nonce (`isolation-probe-lib.sh:_default_nonce`) is
`<b36(pid)>x<b36(epoch)>` — joined with the **letter `x`**, no hyphen, so the marker
`infra003-warmup-<nonce>` (e.g. `infra003-warmup-1424xthaxt2`) has **no all-digit
hyphen-segment** → `looks_like_feature_id = false` → `topic_signal` dropped → the
observe read-as-barrier (`read_marker ... WHERE topic_signal='<marker>'`) returns 0
rows forever → `WTB=INFRA` at `WARMUP_DEADLINE_SECS`.

The OLD nonce `$$-$(date +%s)` (e.g. `18530-1782573915`) DID contain all-digit
segments → passed the filter → observe persisted → **that is why run 28292626314's
observe legs passed**. The #859 PII fix (which removed phone/SSN-shaped digit runs)
inadvertently removed the very digit-segment the server filter needs. The two
server constraints conflict for the marker: PII content-scan wants *no* long digit
runs; `looks_like_feature_id` wants *an* all-digit segment.

### Empirical confirmation (live cold container)
| marker | persisted `topic_signal` |
|---|---|
| `infra003-warmup-1424xthaxt2` (current #859 shape) | **NULL** |
| `infra003-warmup-18530-1782573915` (old `$$-epoch` shape) | persisted (`extracted`) |
| `infra003-warmup-1-2` (minimal feature-id shape) | persisted |
| `col-024` | persisted |

Note: `session_id` persists verbatim on every variant (stored prefixed `http-…`);
only `topic_signal` is filtered.

## Affected Files and Functions
| File | Function | Role |
|---|---|---|
| product/test/infra-001/scripts/isolation-probe-lib.sh | `_default_nonce` / `observe_write` | #859 nonce yields markers with no all-digit segment; observe marker rides `topic_signal` |
| product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh | `warmup_barrier`, `derive_markers`, `run_cells`, `read_marker` | Warmup + matrix observe legs read back `topic_signal` that is never persisted |
| crates/unimatrix-server/src/uds/listener.rs:289-303 | `looks_like_feature_id` | Intentional filter; drops non-feature-id `topic_signal` |
| crates/unimatrix-server/src/uds/listener.rs:196-244 | `enrich_topic_signal_with_source` | Applies the filter (bugfix-832) on HTTP+UDS |
| crates/unimatrix-server/src/uds/listener.rs:3330 | `insert_observation` | Persists the (already-nulled) `topic_signal` |
| crates/unimatrix-server/src/services/store_ops.rs:131-140 | `StoreService::insert` | The MCP path that DOES require the embedding model |

## DELIVERABLES

### (a) Warmup readiness SIGNAL — switch to the MCP `context_store` round trip
**Recommendation: replace the observe-durability warmup probe with the MCP
`context_store`→`context_search` round trip (the #767 signal); keep observe out of
the warmup entirely.** Rationale:
- It is the only path that actually exercises the embedding model — the real cold
  cost the barrier exists to absorb (observe has zero embed dependency).
- It is the exact surface the load-bearing matrix's MCP writes use, so warming it
  warms the right path **and** yields a clean readiness signal — this resolves
  **OQ-WB-1 affirmatively** (warm the MCP path; don't warm observe).
- It is what #767 calibrated 180s on, making the bound's provenance coherent.
- It sidesteps the `looks_like_feature_id` marker trap (the MCP probe asserts on the
  JSON-RPC result, not on a `topic_signal` read-back).
- Marker constraint for the MCP probe: keep it PII-shape-safe (per the prior
  investigator / #5355) — `context_store` content-scans; observe does not.

### (b) Evidence-based `WARMUP_DEADLINE_SECS`
**Keep 180s** (for the MCP signal). The value was never the problem.
- Measured cold model load: **3–4s**; cold MCP round trip ready: **~5s** (this
  sandbox, fast HF).
- #767 retry/backoff floor for a throttled CI HF link: **~70s** (10+20+40).
- 180s = ~2.5× the 70s backoff floor and ~36× the measured cold-ready ⇒ comfortable
  margin covering a fully-retried slow cold download. **Do not lower below ~120s**
  (CI HF throttling variance). Recommended: `WARMUP_DEADLINE_SECS=180` unchanged,
  env-overridable for slow runners. *(Caveat: my HF link is fast, so the absolute
  3–5s is a lower bound; the 70s #767 budget is the conservative anchor.)*

### (c) Scope: in-feature warmup tuning vs deeper issue
- **Switching the warmup signal to the MCP round trip is IN-SCOPE infra-004
  warmup-barrier tuning** — the barrier is the only permitted gate-script change
  (ADR-001) and this makes it deterministically-GREEN-when-healthy.
- **This is NOT a server bug.** `looks_like_feature_id` is intentional (bugfix-832);
  the fix belongs in the test marker contract, not `crates/`.
- **There is a larger, related defect beyond warmup that must also be fixed:** the
  infra-003 isolation MATRIX's observe positive controls (C5 `POS_OBS_A`/`POS_OBS_B`
  via `write_then_barrier observe` → read `topic_signal`) are broken by the SAME
  filter + #859 markers. Switching warmup to MCP does **not** rescue the matrix's
  observe half — it will time out INFRA too. Fix options (gate-script, no `crates/`):
  1. Make the marker contract satisfy BOTH server constraints — a SHORT all-digit
     hyphen-segment (e.g. `infra003-3-<b36nonce>`: the `3` passes
     `looks_like_feature_id`; too short to form a phone/SSN shape; keep R-12/R-18),
     **or**
  2. Key the observe read-as-barrier off a column that persists verbatim
     (`session_id`, which is stored unfiltered) instead of `topic_signal`.
  Option 1 also unbreaks the matrix observe half. File this as a GH issue against the
  infra-003 isolation gate (marker/observe contract); it interacts with infra-004's
  warmup. Per project rule, the concrete defect is a GH issue, not a Unimatrix lesson.

## Risk Assessment
- **Blast radius**: the marker contract is shared by warmup, C3 observe, C4 MCP, and
  C5/C6 read-as-barrier. Any marker reshape must preserve R-12 (`[a-z0-9-]`), R-18
  (pairwise non-substring), the PII-shape canary (#859), AND now
  `looks_like_feature_id`, plus the sqlite predicate matching.
- **Regression risk**: switching warmup to MCP is low risk (reuses a proven probe);
  the marker reshape must thread two server filters — covered by adding a
  `looks_like_feature_id`-shape self-check alongside the existing R-12/R-18/PII
  checks.
- **Confidence**: **High.** Cold timing measured twice; observe/embed independence
  confirmed in source; `topic_signal` drop reproduced live across four marker shapes;
  exact filter cited (listener.rs:289-303) and tied to the #859 nonce change.

## Missing Test
A deterministic off-Docker check (via the existing `SMOKE_*` stub seam or a tiny
assertion) that every derived marker that will be read back through `topic_signal`
ALSO satisfies `looks_like_feature_id` (≥1 all-digit hyphen-segment AND ≥1 alpha
segment) — mirroring the existing R-12/R-18/PII canaries. This would have caught the
#859 nonce change at gate-logic-test time instead of on a CI dispatch. (Equivalently,
a test asserting the observe read-as-barrier keys off a verbatim-persisted column.)

## Reproduction Scenario (deterministic)
1. Boot `ghcr.io/dug-21/unimatrix:latest-{arch}` cold (fresh `/shared`), register
   `arch-research`.
2. `POST /v1/arch-research/observe` with
   `{"type":"RecordEvent","event_type":"tool_use","session_id":"x","payload":{},"topic_signal":"infra003-warmup-1424xthaxt2"}`
   → 204, but the `observations` row has `topic_signal=NULL`.
3. Poll `SELECT count(*) FROM observations WHERE topic_signal='infra003-warmup-1424xthaxt2'`
   → 0 forever ⇒ warmup `WTB=INFRA` at `WARMUP_DEADLINE_SECS`.
4. Repeat step 2 with `topic_signal:"infra003-warmup-18530-1782573915"` (or any
   feature-id-shaped value) → row persists; barrier would pass. The cold model is
   ready in ~3–5s regardless, so timing is never the gating factor.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (infra-004 warmup) + `context_get`
  #5349 (ADR-001 warmup bound) and #5352 (ADR-004 cold-model proof). Read the prior
  `infra-004-mcp-infra-investigator-report.md` (run 28292626314 = a *different*
  failure: MCP marker phone-PII; already fixed by #859 — which this report shows
  introduced the present observe-side regression).
- Stored: nothing. Per project rule "bugs are GH issues, not lessons" this concrete
  gate-script/marker-contract defect goes to a GH issue, not Unimatrix. The
  generalizable trap (isolation markers must satisfy BOTH the PII content-scan AND
  the `looks_like_feature_id` filter, which pull in opposite directions on digit
  runs) is already partially captured by lesson #5355 (PII-shape-safe markers) and
  should be extended there by the fixing agent if desired — not duplicated here.
