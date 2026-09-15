# SPECIFICATION — vnc-049: OpenCode observation harness (C18)

Source: `product/features/vnc-049/SCOPE.md` (APPROVED). Grounding:
`product/features/vnc-049/SCOPE-RISK-ASSESSMENT.md`,
`product/research/ass-106/ass-106-findings.md`.
Downstream consumers: architect, pseudocode, risk-strategist, tester.

## Objective

Deliver OpenCode as the **fourth Unimatrix observation harness** (after claude-code,
gemini-cli, codex-cli) by adding a TypeScript in-process plugin shim that maps OpenCode's
typed hooks and event bus to Claude-shaped `HookInput` frames and invokes
`unimatrix hook <EVENT> --provider opencode`, reusing the existing Rust ingestion pipeline
unchanged. Events must land with correct `provider` **and** `source_domain` attribution,
subagent telemetry must align to the owning feature/cycle, local-model activity must be
queryable as distinct from cloud-model activity, and the C17 installer must provision the
plugin without regressing OpenCode's existing `mcp.unimatrix` retrieval (C10).

## Ubiquitous Language (read this first)

These terms are used precisely throughout. Downstream agents must not conflate them.

| Term | Definition |
|---|---|
| **Harness** | A coding-agent runtime observed by Unimatrix (claude-code, gemini-cli, codex-cli, opencode). OpenCode is the fourth. |
| **provider** | `ImplantEvent.provider` / `HookInput.provider` — the harness identity. For this feature: the literal string `"opencode"`. Set by the `--provider opencode` CLI flag and the normalizer arm. Identifies *which harness emitted the event*. |
| **source_domain** | A server-**derived** attribution field, `^[a-z0-9_-]{1,64}$`, resolved from the event type via `DomainPackRegistry::resolve_source_domain(event_type)` (`crates/unimatrix-observe/src/domain/mod.rs:180`). The hook ingress path forces `DEFAULT_HOOK_SOURCE_DOMAIN = "claude-code"` (`crates/unimatrix-server/src/services/observation.rs:572`). **provider != source_domain**: setting `--provider opencode` does NOT by itself yield `source_domain=opencode`. |
| **model-id / backend-model** | The concrete LLM behind an OpenCode session, exposed in-process as `model:{providerID, modelID}` (e.g. `ollama`/`qwen3-coder`). No carrier for this exists on the wire today; AC-06 adds one. Distinct from `provider` (which stays `"opencode"` regardless of backend model). |
| **The 7 canonical events** | The vnc-013 canon: SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, PreCompact, Stop. |
| **Reachable / bus-derived / unreachable** | *Reachable*: fires OOB via a typed hook (UserPromptSubmit, PreToolUse, PostToolUse, PreCompact). *Bus-derived (degraded)*: reconstructed plugin-side from observe-only bus events (SessionStart from `session.created`, Stop from `session.idle`, SubagentStart from child `session.created`+`parentID`). *Unreachable*: SubagentStart **retrieval-injection** — cannot be delivered by OpenCode's architecture. |
| **Parity corpus** | The cross-language regression fixture (`crates/unimatrix-server/src/uds/parity_corpus_uds.rs`) asserting the Rust normalizer (`hook.rs`) and the JS normalizer (`packages/unimatrix/lib/hook-client/normalize.js`) canonicalize identically. Guards the col-022 split-brain. |
| **Split-brain (col-022)** | The known defect class where `hook.rs` and `normalize.js` — mirrored normalizers — are edited on only one side. |
| **Measured parity gap** | Legs OpenCode cannot reach by architecture, recorded honestly as observed-not-supported, NOT as failures or silent losses. |

## Domain Models

### Entities and relationships

- **OpenCode plugin shim** (net-new TS artifact, `.opencode/plugins/`): consumes OpenCode's
  `PluginInput` (`client`, `project`, `directory`, `worktree`, `$` Bun shell) and returns a
  `Hooks` object. It is the **normalization boundary** — OpenCode has no stdin command
  contract, so the plugin, not a stdin schema, constructs the Claude-shaped frame. For each
  observed event it builds a `HookInput` JSON and invokes `unimatrix hook <EVENT> --provider opencode`.
- **`HookInput`** (`crates/unimatrix-engine/src/wire.rs`): flat JSON
  `{ hook_event_name, session_id, cwd, transcript_path, prompt, provider, mcp_context,
  extra{tool_name, tool_input, tool_response, agent_type, exit_code} }`, all
  `#[serde(default)]`. The shim populates it from OpenCode's differently-shaped inputs.
- **`ImplantEvent`** (`wire.rs`): the internal event carrying `provider`. It carries **no
  model-id/backend-model field today** — AC-06 requires a carrier (payload field or new typed
  field) plumbed plugin → `HookInput` → `ImplantEvent` → attribution.
- **Normalizer arm** (`hook.rs` `map_to_canonical`/`normalize_event_name`,
  `KNOWN_PROVIDERS` at `hook.rs:158`) and its **JS mirror** (`normalize.js`): canonicalize
  event names and stamp provider. Adding a harness = a coupled edit to BOTH plus the parity
  corpus (per pattern #5737: three coupled touchpoints, not one flag).
- **`DomainPackRegistry` / source_domain resolution** (`domain/mod.rs`): maps event → domain.
  An explicit `opencode` resolution/domain-pack path is required for AC-03.
- **C17 installer** (`packages/unimatrix/lib/init.js`): today writes only
  `.mcp.json` + `.claude/settings.json`. Gains a net-new additive OpenCode branch.

### The 7 canonical events → OpenCode mapping (from ass-106 §B)

| Canonical | OpenCode source | Kind | Reachability |
|---|---|---|---|
| SessionStart | `session.created` (+`session.updated`) | bus | bus-derived (degraded) |
| UserPromptSubmit | `chat.message` | typed, mutable | reachable |
| PreToolUse | `tool.execute.before` | typed, mutable | reachable |
| PostToolUse | `tool.execute.after` | typed, mutable | reachable |
| SubagentStart | child `session.created` + `parentID` + `session.agent` | bus | observation bus-derived; **injection unreachable** |
| PreCompact | `experimental.session.compacting` | typed, mutable, EXPERIMENTAL | reachable (unstable API) |
| Stop | `session.idle` | bus | bus-derived (degraded; over-count risk) |

## Functional Requirements

Each FR is testable. FR→AC traceability is given per requirement.

- **FR-01** — The plugin shim SHALL map each of the 7 canonical events from OpenCode's typed
  hooks / event bus to a Claude-shaped `HookInput` and deliver it via
  `unimatrix hook <EVENT> --provider opencode`. Reachable and bus-derived events SHALL fire
  and land as stored records. *(AC-01)*
- **FR-02** — For SessionStart (`session.created`) and Stop (`session.idle`), the shim SHALL
  compute the fields absent from the bus event (e.g. `cwd`/`worktree` from `PluginInput`,
  `duration`/`outcome` for Stop) plugin-side, and SHALL NOT over-count Stop when
  `session.idle` fires on repeated idle transitions (SR-04). These legs SHALL be documented
  as degraded/bus-derived, not full-fidelity. *(AC-01)*
- **FR-03** — SubagentStart observation SHALL be derived from a child `session.created` whose
  `parentID` is non-null, carrying the validated `session.agent`. SubagentStart
  retrieval-**injection** SHALL NOT be attempted and SHALL be recorded as a measured parity
  gap. *(AC-01, AC-04)*
- **FR-04** — `"opencode"` SHALL be added to `KNOWN_PROVIDERS` in `hook.rs` AND a matching
  normalizer arm SHALL be added to `normalize.js`, landed together. The parity corpus test
  (`parity_corpus_uds.rs`) SHALL be extended to cover opencode and SHALL be green. *(AC-02)*
- **FR-05** — Stored records from OpenCode events SHALL carry `provider = "opencode"`. *(AC-02)*
- **FR-06** — An explicit `opencode` `source_domain` resolution/domain-pack path SHALL be
  added such that a stored OpenCode event record resolves `source_domain = "opencode"`, NOT
  the `claude-code` hook-path default. The system SHALL NOT silently fall back to
  `claude-code` for OpenCode events. *(AC-03; SR-07, SR-11)*
- **FR-07** — Subagent telemetry SHALL capture the validated `session.agent` and
  `parentID`, and SHALL align the child session's telemetry to the owning feature/cycle on
  the stored record. The validated `session.agent` SHALL NOT be folded into spoofable tool
  args (preserves the AC-07 seam). *(AC-04; SR-09)*
- **FR-08** — A backend-model carrier SHALL be plumbed from the plugin's in-process
  `model:{providerID, modelID}` through `HookInput` → `ImplantEvent` → attribution, such that
  a local-model event (e.g. `ollama`/`qwen3-coder`) LANDS and is QUERYABLE as distinct from a
  cloud-model event through the real assembled path. *(AC-06; SR-01, SR-02, SR-10)*
- **FR-09** — The C17 installer SHALL detect OpenCode by presence of `opencode.json` or
  `.opencode/`, and SHALL provision the plugin non-clobbering: drop the shim into
  `.opencode/plugins/` and add its dep to `.opencode/package.json`, and/or append to the
  `plugin:[]` array in `opencode.json`. *(AC-05)*
- **FR-10** — The installer delta SHALL be strictly additive: the existing `mcp.unimatrix`
  retrieval entry, the local STDIO command, the local Ollama `provider` block, permissions,
  and all non-Unimatrix keys SHALL be preserved byte-for-byte. *(AC-05; SR-08)*
- **FR-11** — The delivery SHALL keep the `external_identity` seam
  (`build_context_with_external_identity`, currently always `None`) viable and SHALL NOT
  foreclose a per-call MCP delivery-channel option. No E1 enforcement is built. *(AC-07; SR-12)*
- **FR-12** — Every unreachable-by-architecture leg (SubagentStart injection) and every
  degraded bus-derived leg (Stop/SessionStart derivation, PreCompact experimental
  dependency) SHALL be documented as a measured parity gap, and SHALL NOT be reported as a
  passing full-fidelity leg nor allowed to silently corrupt or drop data. *(AC-01; SR-03, SR-04)*

## Non-Functional Requirements

- **NFR-01 (attribution integrity)** — Attribution correctness is verified on the **stored
  record** from a real OpenCode event, never on the flag passed or an internal seam. Applies
  to `provider` (FR-05), `source_domain` (FR-06), and backend-model distinctness (FR-08).
- **NFR-02 (fail-open transport preserved)** — The plugin reuses the existing Rust pipeline
  (UDS transport, queue, fail-open, drop-detector) with zero pipeline rewrite; the only Rust
  deltas are the provider arm and the source_domain resolution (plus the AC-06 carrier). A
  failure to emit an event MUST NOT block or crash the user's OpenCode session.
- **NFR-03 (retrieval non-regression)** — After installer provisioning, OpenCode
  `context_*` retrieval (C10) MUST still return results; a regression test asserts this.
- **NFR-04 (split-brain parity)** — Rust and JS normalizers MUST canonicalize opencode
  identically, enforced by the parity corpus test as a permanent regression sentinel.
- **NFR-05 (file line cap)** — C18-touched regions of `background.rs` / `listener.rs` /
  `hook.rs` are judged against the **500 code-line cap (tests excluded)**. Raw line counts
  (background.rs 5075, listener.rs 10131, hook.rs 4403) are raw-line figures and MUST be
  re-measured in code-lines before any carve is judged. No requirement to bring whole modules
  under the cap. Whether to take a targeted carve is an architect decision.
- **NFR-06 (installer idempotence)** — Running the installer repeatedly on an OpenCode repo
  MUST be idempotent (no duplicate plugin entries, no drift in preserved keys).

## Acceptance Criteria (verification methods, mapped to AC-01..07)

Verification methods are stated so the tester can root them in the Risk Strategy. Where a
risk demands an end-path/stored-record assertion, that is called out explicitly.

- **AC-01** — 7-event mapping. **Verify:** integration test drives OpenCode-shaped inputs
  for each canonical event through the plugin shim → `unimatrix hook --provider opencode`;
  assert each reachable and bus-derived event lands as a stored record. Assert
  unreachable-by-architecture legs (SubagentStart injection) are documented as the measured
  parity gap and produce no false-pass. *(FR-01, FR-02, FR-03, FR-12)*
- **AC-02** — provider attribution + split-brain parity. **Verify:** (a) unit assertion that
  `"opencode"` is in `KNOWN_PROVIDERS`; (b) the extended `parity_corpus_uds.rs` covers
  opencode and is green (Rust ↔ JS canonicalization identical); (c) a stored OpenCode record
  shows `provider = "opencode"`. *(FR-04, FR-05; SR-06)*
- **AC-03** — source_domain attribution (SR-07 silent-default class). **Verify:** the test
  MUST assert the **STORED record resolves `source_domain = "opencode"`** — not merely that
  `--provider opencode` was passed. A test that stops at "the flag was passed" is
  insufficient. Include a negative assertion that the record is NOT `source_domain =
  "claude-code"`. *(FR-06; SR-07, SR-11)*
- **AC-04** (show-stopper) — subagent alignment. **Verify:** drive a child
  `session.created` with non-null `parentID` and a validated `session.agent`; assert the
  **stored subagent telemetry record is aligned to the owning feature/cycle** (parent→cycle
  correlation resolved), not orphaned/unaligned. Assert `session.agent` is captured from the
  validated harness field and is NOT sourced from spoofable tool args. *(FR-07; SR-09)*
- **AC-05** — non-clobbering install + retrieval preservation. **Verify:** (a) installer run
  on a fixture OpenCode repo provisions the plugin (`.opencode/plugins/` + package dep and/or
  `plugin:[]` append); (b) **regression assertion** that `mcp.unimatrix` + local STDIO command
  + provider block + non-Unimatrix keys are preserved **byte-for-byte**; (c) a retrieval
  regression test asserts `context_*` still returns results after provisioning; (d)
  idempotence check (re-run produces no duplicate/drift). *(FR-09, FR-10; SR-08)*
- **AC-06** (E2; C18 raison d'être) — local-model distinctness. **BEHAVIORAL END-PATH
  assertion, authoritative regardless of when E2 lands:** a local-model event (e.g.
  `ollama`/`qwen3-coder`) SHALL be shown to LAND and be QUERYABLE as **distinct from a
  cloud-model event** through the **real assembled plugin → wire → attribution → store →
  query path**. It is NOT acceptable to assert only that the carrier field is populated, nor
  to prove distinctness at an internal seam or via an injected dependency (SR-01
  proven-but-holed; SR-10 path-divergence; #907/#918/#930 lesson). The test drives a real
  OpenCode-shaped local-model event end to end and queries back a stored record
  distinguishable from a cloud-model record. *(FR-08; SR-01, SR-02, SR-10)*
- **AC-07** (forward-compat constraint, E1) — seam viability. **Verify:** static/structural
  assertion that `build_context_with_external_identity` remains callable with an
  `external_identity` value (seam not foreclosed), that a per-call MCP delivery-channel option
  remains open, and that validated `session.agent` is NOT written into spoofable tool args. No
  N=1 ceremonial test giving false confidence (SR-12); no enforcement built. *(FR-11; SR-12)*

**C18 ledger guardrail (authoritative):** AC-06 governs C18 status independent of E2 sizing.
C18 does NOT reach `proven` on AC-01..05 alone. Until AC-06's local-vs-cloud distinctness is
behaviorally demonstrated end-path, C18 stays `partial` (honest-visible-partial), including
the interim if E2 fast-follows. The E2-sizing latitude (in-cycle vs fast-follow) is the
architect's call; the AC-06 criterion above is authoritative regardless of when it lands.

## User Workflows / Use Cases

- **UW-1 — Plugin install via the C17 installer.** A user runs `unimatrix init` in an
  OpenCode repo. The installer detects `opencode.json`/`.opencode/`, provisions the shim
  plugin non-clobbering, and leaves existing `mcp.unimatrix` retrieval working. Observed
  outcome: the plugin is installed AND `context_*` retrieval still returns results (AC-05).
- **UW-2 — Event emission (7 canonical events).** A user runs an OpenCode coding session with
  the plugin installed. Every reachable-by-architecture canonical event lands as a queryable
  record with `provider=opencode` and `source_domain=opencode`; unreachable legs are recorded
  as the measured parity gap, not silent loss (AC-01, AC-02, AC-03).
- **UW-3 — Subagent telemetry alignment.** A user/agent spawns a subagent inside OpenCode.
  The subagent's telemetry is observable and aligned to the owning feature/cycle via the
  validated `session.agent` + `parentID` (AC-04).
- **UW-4 — Local-model attribution query.** A user runs OpenCode against a local model
  (e.g. `ollama`/`qwen3-coder`). That activity is stored and queryable as distinct from a
  cloud-model event through the real assembled path (AC-06).

## Constraints

- **C-1 (split-brain, col-022)** — Any provider/normalizer change lands in `hook.rs` AND
  `normalize.js` together, guarded by the extended parity corpus test. *(SR-06)*
- **C-2 (C17 regression sentinel, #5582)** — `mcp.unimatrix` retrieval / local STDIO must
  not regress; installer delta strictly additive. *(SR-08)*
- **C-3 (source_domain is event-derived server-side)** — Defaults to `claude-code` on the
  hook path; `--provider opencode` alone is insufficient for attribution. An explicit
  opencode resolution path is required and must fail loud, never silently default. *(SR-07)*
- **C-4 (no stdin command contract in OpenCode)** — Every event must pass through the plugin
  shim; the `--provider opencode` arm is necessary but not sufficient. (ass-106 §A/C.)
- **C-5 (500 code-line cap, tests excluded)** — Judged in code-lines, not the raw counts
  above; targeted carve of `background.rs`/`listener.rs`/`hook.rs` regions is an open
  architect decision, not a requirement. *(SR-05)*
- **C-6 (PreCompact experimental dependency)** — Rests on `experimental.session.compacting`,
  flagged unstable upstream. Absorbed into the parity-gap posture: document as
  degraded/experimental, gate or flag it; its instability must not fail the cap. *(SR-03)*
- **C-7 (E1 forward-compat only)** — Demand-pulled under trusted-identity #5734; keep the
  seam viable, build no enforcement. *(SR-12)*
- **C-8 (C10 preserved)** — Remote/local retrieval is proven and is the regression sentinel;
  it is preserved, not modified.

## Dependencies

- **C17 installer provisioning (#5582)** — prerequisite for the install leg (AC-05). The
  net-new OpenCode installer branch extends C17; the nan-004 (#1201) prefix-match non-clobber
  merge is the *principle* to reuse (surface differs: plugin dir + `plugin[]` array, not
  per-event matcher groups).
- **C10 remote/local retrieval (#5547)** — proven; must be preserved (regression sentinel).
- **Existing Rust pipeline (reused unchanged)** — `crates/unimatrix-engine/src/wire.rs`
  (`HookInput`/`ImplantEvent`); `crates/unimatrix-server/src/uds/hook.rs`
  (CLI hook path, `KNOWN_PROVIDERS` @158, normalizer); `crates/unimatrix-observe/src/domain/mod.rs`
  (`resolve_source_domain` @180); `crates/unimatrix-server/src/services/observation.rs`
  (`DEFAULT_HOOK_SOURCE_DOMAIN` @572); `crates/unimatrix-server/src/uds/parity_corpus_uds.rs`.
- **JS edge client** — `packages/unimatrix/lib/hook-client/normalize.js` (mirror normalizer),
  `packages/unimatrix/lib/init.js` (installer).
- **OpenCode plugin API** — `@opencode-ai/plugin@1.18.31` (already the sole dep in
  `.opencode/package.json`; no plugin code exists yet).
- **Prior conventions** — provider-field ADR (Unimatrix #4306, vnc-013); harness-onboarding
  three-touchpoints pattern (Unimatrix #5737).

## NOT in Scope (explicit exclusions)

- **SubagentStart retrieval-injection** — architecturally unreachable in OpenCode; observation
  of the spawn is in scope, injection is not. Accepted parity gap, not a defect.
- **Full E1 trusted-identity design/delivery** — harness-attested identity → capability
  enforcement is demand-pulled under #5734. Only the AC-07 forward-compat constraint applies;
  no enforcement, no PL-4 fail-closed flip, no MCP-proxy shim built here.
- **Command-hook mirror (`.codex/hooks.json` model)** — architecturally impossible; OpenCode
  has no command-hook mechanism (ass-106 finding A/C).
- **Bringing whole `background.rs`/`listener.rs`/`hook.rs` under the 500 code-line cap** — a
  targeted carve is an open architect decision, not a requirement.
- **Active-governance / policy surfaces** — mutable `chat.*` injection and `permission.ask`
  as a policy decision point (ass-106 out-of-scope discoveries) are beyond C18 observation.

## Open Questions (for architect / user)

- **OQ-1 (E2 sizing — architect's call)** — Take per-model attribution (AC-06) inside this
  cycle, or fast-follow? Either way, the AC-06 end-path criterion and the C18 `partial`-until-
  AC-06 ledger guardrail are authoritative.
- **OQ-2 (targeted carve — architect's call)** — Take a targeted carve of the C18-touched
  `background.rs` (#966) region under the 500 code-line cap? Same for `listener.rs` (#965)
  only if it is edited (not reused-unchanged). Re-measure in code-lines first.
- **OQ-3 (`session.idle` → Stop semantics)** — Fires once per Stop with derivable
  `duration`/`outcome`, or on every idle transition (over-count risk)? Needs PoC measurement
  (ass-106 unanswered Q; feeds FR-02/SR-04).
- **OQ-4 (source_domain resolution mechanism)** — Note for architect: entry #4306 shows
  listener Site A deriving `source_domain` from `event.provider.unwrap_or("claude-code")`,
  while #5737 / observation.rs force the `claude-code` default on the hook path via
  `DomainPackRegistry`. The architect must reconcile which path AC-03 flows through so the
  stored record resolves `source_domain=opencode`; the requirement (FR-06) is on the stored
  outcome, not the mechanism.
- **OQ-5 (AC-06 model carrier shape)** — payload field vs new typed `ImplantEvent` field?
  New wire field = blast radius (SR-02); must gain parity-corpus coverage.
- **OQ-6 (subagent MCP connection)** — Do OpenCode child sessions share the parent's MCP
  client or get their own? Gates AC-07 seam viability (channel (i) vs proxy (ii)); does not
  gate this cycle's delivery.
- **OQ-7 (PreCompact experimental stability)** — Depend on it now, or gate/flag it (C-6)?

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned harness-onboarding pattern #5737
  (three coupled touchpoints: provider arm both sides, split-brain parity corpus, explicit
  source_domain resolution; per-model needs a model carrier) and provider-field ADR #4306
  (vnc-013 explicit `provider` on HookInput/ImplantEvent; listener derives source_domain from
  provider with claude-code fallback). Both incorporated into Domain Models, FR-04/06/08, and
  OQ-4/OQ-5. Read-only tier; no storage.
