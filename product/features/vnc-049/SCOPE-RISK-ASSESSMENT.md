# Scope Risk Assessment: vnc-049

Grounding: Unimatrix #5737 (harness = 3 coupled touchpoints), #4177 (tautology caught at gate), #4876 (gate-integrity claims verified empirically, not structurally), #4974 (ceremonial N=1 seam), #4298 (normalize at ingest boundary).

## User-Facing Entry Points & Behavioral Outcomes

Path-independent contract the architecture AND tests must both satisfy. No architecture exists yet — outcomes are what the user/agent OBSERVES, never how it is delivered.

| Entry point (how the user/agent actually invokes it) | Path-independent outcome they must observe |
|---|---|
| User runs an OpenCode coding session with the Unimatrix plugin installed | All reachable-by-architecture canonical events land as queryable records; unreachable legs recorded as measured parity gap, not silent loss |
| User runs OpenCode against a LOCAL model (e.g. ollama/qwen3-coder) | That activity is stored and QUERYABLE as distinct from a cloud-model event — through the real assembled path (AC-06 end-path) |
| Any OpenCode event is emitted | The stored/queryable record shows `source_domain=opencode` and `provider=opencode`, NOT the `claude-code` default (AC-03) |
| User/agent spawns a subagent inside OpenCode | The subagent's telemetry is observable and aligned to the owning feature/cycle (AC-04) |
| User runs the C17 installer (`unimatrix init`) in an OpenCode repo | Plugin is provisioned AND existing `context_*` retrieval still returns results (C10 preserved) |

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **AC-06 proven-but-holed cap.** A "model-carrier field is populated" assertion is a proxy/tautology (#4177): field can be set while local events never distinctly LAND or become QUERYABLE via the assembled path. Ships C18 green with a hole. | High | High | AC-06 MUST assert a local-model event lands and is queryable as distinct from a cloud event through the real path — no proxy/injected-dependency/tautological assertion. Verify empirically (#4876). **C18 ledger guardrail: not `proven` until this end-path is behaviorally demonstrated; `partial` otherwise.** |
| SR-02 | **No model-id carrier exists.** `ImplantEvent` carries `provider` but no `model_id`; AC-06 needs a new field plumbed plugin→wire→attribution (#5737). New wire field = blast radius. | High | Med | Architect: decide carrier (payload vs typed field) with parity-corpus coverage; if E2 fast-follows, C18 stays `partial` per guardrail. |
| SR-03 | **PreCompact rests on experimental `session.compacting`.** Unstable upstream API; leg may break silently. | Med | Med | Treat as absorbed by the parity-gap posture — document as degraded/experimental leg, gate or flag it; do not let its instability fail the cap. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **`session.idle`→Stop over-count / bus-derived degradation.** `session.idle` may fire on every idle transition; Stop/SessionStart `duration`/`outcome` computed plugin-side (data-quality risk on individual events). | Med | Med | Absorb into parity-gap posture: PoC-measure idle semantics; document Stop/SessionStart as degraded-derived, not full-fidelity. Do not silently over-count. |
| SR-05 | **500 code-line cap vs C18-touched regions** of background.rs/listener.rs/hook.rs — raw counts are NOT code-line counts. | Low | Med | Architect re-measures in code-lines before judging any targeted carve; no requirement to bring whole modules under cap. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **Split-brain drift (col-022).** Rust normalizer (hook.rs) mirrored by normalize.js; editing one side only is a known defect class (#5737). | High | High | Land the `opencode` arm in BOTH hook.rs and normalize.js together; extend `parity_corpus_uds.rs` to cover opencode and make it green (AC-02). |
| SR-07 | **Silent attribution default to `claude-code` (AC-03).** `source_domain` is event-derived server-side and forced to `claude-code` on the hook path; `--provider opencode` alone is insufficient — events land mis-attributed but green. | High | High | Require an explicit `opencode` `source_domain`/domain-pack resolution path, validated by a test asserting the stored domain is `opencode`, not the default. |
| SR-08 | **C10 retrieval regression during installer work (AC-05).** Installer touching `opencode.json` could clobber `mcp.unimatrix`/local STDIO. | High | Med | Strictly additive install delta; regression test asserts retrieval still returns after provisioning; preserve `mcp.unimatrix` + provider block byte-for-byte. |
| SR-09 | **Subagent alignment (AC-04 show-stopper).** Subagent spawn is only a bus-derived `session.created`+`parentID`; if correlation to feature/cycle fails, telemetry lands unaligned/orphaned. | High | Med | Require validated `session.agent` capture + explicit parent→feature/cycle alignment, tested. Do not fold validated `session.agent` into spoofable tool args (protects AC-07 seam). |

## Path-Divergence Risks

The "works on path A, user invokes path B" class the closed design→test→gate loop cannot catch alone.

| Risk ID | Entry point | Divergence (path that works vs. path the user invokes) | Recommendation |
|---|---|---|---|
| SR-10 | Local-model session | Model carrier proven at an internal seam / injected dependency, while the real plugin→wire→attribution path a user's ollama session travels drops or homogenizes the model — cap green, user's local activity indistinguishable. | AC-06 asserted from the user's actual local-model session end-to-end; distinctness proven on the assembled path, not a seam (#4974). |
| SR-11 | Any event emission | Provider stamped `opencode` but `source_domain` still resolves `claude-code` (accepted-but-inert on the invoked path). | Attribution asserted on the stored record from a real opencode event; accepted-but-inert domain must fail loud, never silently default. |
| SR-12 | AC-07 external_identity seam | Seam kept "viable" but ceremonial (always `None`, N=1) — future E1 consumer finds it carries no value. | Keep seam viable without building enforcement; do not add an N=1 test that gives false confidence (#4974). Not this cycle's delivery. |

## Assumptions

- **Pipeline reuses unchanged** (SCOPE Background/Proposed Approach): plugin shim → `unimatrix hook --provider opencode` reuses UDS/queue/fail-open. If the plugin cannot reliably shell/connect per-event, the whole ingestion premise fails. References Background Research + ass-106 §C(b).
- **Reachable events actually fire OOB** (AC-01 / ass-106 §B): SubagentStart injection and full-fidelity Stop are architecturally unreachable — treated as measured parity gap, not defects (Capability Mapping). If more legs are unreachable than measured, AC-01 coverage shrinks.
- **`opencode.json`/`.opencode/` presence reliably signals OpenCode** (AC-05): installer detection assumes these markers; hand-authored today.

## Design Recommendations

1. **AC-06 is the cap's raison d'être, not an optional E2 (SR-01, SR-02, SR-10).** Whether E2 lands now or fast-follows, C18 stays `partial` until the local-vs-cloud distinctness is behaviorally demonstrated end-path. Carry this BINDING directive into the architecture-risk phase and the test plan.
2. **Treat harness onboarding as three coupled touchpoints** (#5737): provider arm (both normalizers, SR-06), explicit `source_domain` resolution (SR-07), and model carrier (SR-02) — none is a single flag.
3. **Assert outcomes from the user's real entry point** (SR-10, SR-11): attribution and model-distinctness proven on stored/queryable records from real opencode sessions, not internal seams.
4. **Installer delta strictly additive with a retrieval regression sentinel** (SR-08).
5. **Absorb bus-derived data-quality risks into the parity-gap posture** (SR-03, SR-04): document degraded legs honestly; do not let experimental/over-count legs either fail the cap or silently corrupt data.
