# nan-021 Researcher Report

**Agent:** nan-021-researcher | **Date:** 2026-06-24

## Deliverable
- SCOPE.md: `/workspaces/unimatrix/product/features/nan-021/SCOPE.md` (7 ACs, all sections present).

## Key Findings (grounded in code)
- **"infra-001" is two instruments** under `product/test/infra-001/`: (a) Python pytest MCP harness
  (`harness/client.py` spawns `serve --stdio`; drives `context_cycle`/`context_cycle_review`; also has
  `UnimatrixUdsClient` + `UnimatrixHookClient` = the local-UDS parity baseline), and (b) the Docker
  HTTPS smoke (`scripts/docker-http-posture-smoke.sh` + `release-gate-lib.sh`) which ALREADY stands up
  the shipped HTTPS image, registers a slug, and does a cert-pinned `POST /v1/{slug}/observe`. (b) is
  the cumulative extension point; it currently does NOT drive a full cycle, the MCP bridge, behavioral
  observe, or cycle_review.
- **The stdio-only harness is the real gap.** `project_routing_integration.rs`'s own header: it
  *"cannot reach the `/v1/{slug}/` HTTP edge (it spawns single-project stdio)"* and excludes `/observe`.
- **Full cloud path mapped:** server `serve` HTTP-on + registered slug (`http/listener.rs`,
  `http_provision.rs`); self-signed leaf at `{data_dir}/tls/cert.pem` (`cert_provisioner.rs`); pin via
  `cert-pin.js`; bridge `lib/hook-client/mcp-bridge.js` (`node mcp-bridge.js <projectHash>`, reads
  `credstore.js`, SSE-forced per #5129); `compose_route_urls` → `mcp_url`/`observe_url`; `init --bundle`
  writes credstore + token-free `.mcp.json`.
- **"Seeded topic_signal" (Goal 3) is real and located.** Python `_seed_observation_sql_lifecycle`
  (`test_lifecycle.py:1253`, SQL-injects feature_ids) and Rust `make_stamped_event(..., topic_signal:
  Option<String>)` (`uds/listener/tests/stamp_read.rs:28`). Real derivation: `attribution.rs:
  extract_topic_signal` → `uds/listener.rs:enrich_topic_signal_with_source`. **#832's own commit
  message confirms** the harness was stdio-only and BA-1 "drove closest achievable parity at the
  topic_signal=cycle_id join" — exactly the gap nan-021 closes.
- **Parity surface (Goal 4):** `MetricVector` (`unimatrix-store/src/metrics.rs`) = `UniversalMetrics`
  (21 fields) + `phases` BTreeMap + `domain_metrics`; derived from content-opaque durable streams
  (`cycle_aggregates.rs`) → transport-agnostic, so field-for-field HTTPS-vs-UDS equality is the clean
  C0 proof.
- **Docker VERIFIED present:** `docker version` → Engine 29.5.2, Compose v2.40.3, arm64, responsive.
  A containerized HTTPS fixture (mirroring nan-019) is viable.
- **C0 (#5191) is now a measurement feature** — C5/C10/C11 all proven; only C0's own done_when (a
  measured remote-vs-local parity run) remains. nan-021 is its named evidence artifact.

## Open Questions for Human (full text in SCOPE.md OQ-1..OQ-6)
- OQ-1: primary substrate — Docker container vs in-process Rust TLS vs Python-driven HTTPS serve
  (likely hybrid: container for HTTPS, Python harness for UDS baseline + MetricVector compare).
- OQ-2: harness must spawn `mcp-bridge.js` directly and drive the cycle *through* it (keep bridge in path).
- OQ-3: exact CI lane (release workflow_dispatch/tag vs dedicated Docker PR lane).
- OQ-4: naming/ownership — nan-021 (Nanoprobes test-infra/gate) vs vnc-042; recommend nan-021 owns the
  fixture, proves C0 #5191 (capability credit stays personal-cloud).
- OQ-5: parity tolerance (exact equality vs documented exclusion set for wall-clock fields).
- OQ-6: UDS baseline live-vs-live (stronger) vs golden fixture (simpler, can drift).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced C0 #5191, C11 #5153, vnc-039 ADR-001 #5115,
  edge-client bridge pattern #5129, Layer-2 harness slug+pinned-HTTPS trap #5098 (all directly used).
- Stored: entry #5285 "Cloud-path parity tests must DERIVE topic_signal over the wire, not SQL/struct-seed
  the attribution join" via context_store (generalizable test pattern; feature-specific scope lives in SCOPE.md).
