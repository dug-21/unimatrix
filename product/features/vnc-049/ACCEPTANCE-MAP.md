# vnc-049 Acceptance Criteria Map

Every AC from SCOPE.md mapped to its binding verification. AC-06 (local-model distinctness) and AC-03
(source_domain) carry authoritative, non-proxy end-path / stored-record assertions — a test that stops
at "the flag was passed" or "the field is Some(...)", or proves the outcome at an internal seam / via
an injected dependency, does NOT discharge them.

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | TS plugin shim maps the 7 canonical events → Claude-shaped HookInput → `unimatrix hook --provider opencode`; reachable + bus-derived events land as records; unreachable legs recorded as measured parity gap | test | Integration test drives each of the 7 OpenCode-shaped inputs through the plugin shim → `unimatrix hook`; assert each reachable/bus-derived event is a stored, queryable record; assert SubagentStart injection is recorded as the measured parity gap (no false-pass, no silent drop). Covers FR-01/02/03/12, R-16. | PENDING |
| AC-02 | Events carry correct `provider="opencode"`; split-brain parity (hook.rs + normalize.js + parity corpus) | test | (a) unit assertion `"opencode" ∈ KNOWN_PROVIDERS` (hook.rs:158); (b) extended `parity_corpus_uds.rs` covers opencode across all 7 events (provider/model/subagent) and is green, drift-failing if either arm is edited alone; (c) stored OpenCode record shows `provider="opencode"`. Covers FR-04/05, SR-06, R-03. | PENDING |
| AC-03 | Events resolve to `source_domain="opencode"`, NOT the claude-code hook-path default | test | **Authoritative stored-record assertion (non-proxy):** ingest a real opencode event through the assembled path, SELECT the stored row, assert `source_domain="opencode"` AND a mandatory negative assertion that it is NOT `"claude-code"`. A test that stops at "`--provider opencode` was passed" is insufficient. Pin WHICH derivation site produces the stored value (OQ-4). Fail-loud when the provider-first stamp cannot resolve. Covers FR-06, SR-07/SR-11, R-02. | PENDING |
| AC-04 | Subagent telemetry (child `session.created` + `parentID` + validated `session.agent`) captured and aligned to the owning feature/cycle | test | Drive a child `session.created` with non-null `parentID` + validated `session.agent`; assert the STORED subagent record is aligned to the owning feature/cycle (parent→cycle correlation resolved), not orphaned. Assert `session.agent` is captured from the validated harness field on the observe channel (`extra.agent_type`), NOT from spoofable tool args. Assert ordering: child event arriving before/around parent registration still correlates. Covers FR-07, SR-09, R-06/R-07. | PENDING |
| AC-05 | C17 installer detects OpenCode and provisions the plugin non-clobbering; `mcp.unimatrix` retrieval preserved byte-for-byte | test | (a) installer runs on a fixture OpenCode repo, provisions plugin (`.opencode/plugins/` + package dep and/or `plugin:[]` append); (b) regression assertion `mcp.unimatrix` + local STDIO + Ollama provider block + non-Unimatrix keys preserved byte-for-byte; (c) retrieval regression test asserts `context_*` still returns results after provisioning; (d) idempotence check (re-run: no duplicate/drift). Covers FR-09/10, NFR-03/06, SR-08, R-08/R-09. | PENDING |
| AC-06 | Per-model `source_domain`/model_id attribution: local-model activity queryable as distinct from cloud-model — C18 raison d'être (E2, IN this cycle per ADR-003) | test | **Authoritative behavioral end-path assertion (C18 GATING — R-01):** drive a real OpenCode-shaped LOCAL-model event (`model:{providerID:"ollama",modelID:"qwen3-coder"}`) through the real assembled path plugin→`unimatrix hook --provider opencode --model ...`→UDS→normalizer→ImplantEvent→ingest-persist INSERT→store, then QUERY the stored record back and assert it is distinguishable from a separately-ingested CLOUD-model OpenCode record. Distinctness observed on the queried record, NOT on in-memory ImplantEvent. Negative/anti-tautology: fails if the carrier is dropped/homogenized/NULLed between wire and INSERT; no injected/mocked attribution dependency; not "the `--model` flag was passed" or "the field is Some(...)". Two local models also mutually distinguishable. Covers FR-08, SR-01/SR-02/SR-10. | PENDING |
| AC-07 | Forward-compat (E1): `external_identity` seam kept viable, per-call MCP delivery-channel option open, validated `session.agent` not folded into spoofable tool args; no enforcement built | test | Static/structural assertion that `build_context_with_external_identity` remains callable with an `external_identity` value (seam not foreclosed); a per-call MCP delivery-channel option remains open; validated `session.agent` is NOT written into spoofable tool args (spoof-rejection test). No N=1 ceremonial test giving false confidence; no enforcement built. Covers FR-11, SR-12, R-07. | PENDING |

## Binding / Gating Notes

- **AC-06 is the C18 `proven` gate.** R-01 scenario 1+2 is authoritative regardless of E2 sizing. C18
  stays `partial` (honest-visible-partial) until this end-path distinctness is behaviorally
  demonstrated, including any fast-follow interim. Not a seam, not a populated-field check, not an
  injected dependency.
- **AC-03 stored-record + negative assertion** is authoritative and non-proxy — asserted on the stored
  row from a real opencode event, with the derivation site pinned (OQ-4).
- **AC-02 / AC-03 provider vs source_domain** are distinct axes: `--provider opencode` yields
  `provider="opencode"` but does NOT by itself yield `source_domain="opencode"` (ADR-001 ingest stamp).
- **Degraded / experimental legs (ADR-009):** `session.idle`→Stop over-count guard (R-11) and
  PreCompact experimental-API fail-safe (R-12) are data-quality/parity-gap guards — they must not fail
  the cap and must not silently corrupt data.

## Supporting Risk Scenarios (verified during delivery, not top-level ACs)

| Risk | Scenario | Verification |
|------|----------|--------------|
| R-04 | Migration cascade (next sequential schema version, resolved at delivery — 32 today) | Fresh-create + ALTER-migrate column parity; legacy-NULL read-path fallback (both branches); three-path version bump + idempotency/back-fill. |
| R-05 | Existing-provider source_domain regression | **DECIDED opencode-only (2026-09-15).** Stamp fires only for `provider == "opencode"`; non-opencode rows stay NULL → read-derived fallback. T-SEC-12/13 re-run **green, unchanged**; a test proves the stamp is opencode-only. Resolved — no open decision. |
| R-10 | model_id carrier blast radius | ts-rs binding drift gate green (#4726); serde back-compat (frame without `model_id` deserializes); one client→listener→DB crossing test (#5670). |
| R-13 | Plugin transport fail-open | Session survives an emit failure (binary absent / socket down); no crash/block (NFR-02). |
| R-14 | Bus-derived field synthesis | Derived cwd/worktree correct; absent `transcript_path` handled without a bogus value; malformed bus payload degrades honestly. |
| R-15 | Untrusted input | `model_id`/`source_domain` validated against `^[a-z0-9_-]{1,64}$`; never raw to SQL/paths; installer merge preserves non-Unimatrix keys, follows no injected paths. |
