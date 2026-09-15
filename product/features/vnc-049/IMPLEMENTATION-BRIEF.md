# vnc-049 Implementation Brief — OpenCode observation harness (C18)

OpenCode as the fourth Unimatrix observation harness: behavioral-signal parity + local-model
attribution, with attribution persisted at ingest. Compiled from the approved Session 1 design.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-049/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-049/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-049/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-049/architecture/ARCHITECTURE.md |
| Risk / Test Strategy | product/features/vnc-049/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-049/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-049/ACCEPTANCE-MAP.md |
| Research grounding | product/research/ass-106/ass-106-findings.md (GH #982) |

## Goal

Deliver OpenCode as the fourth observation harness (after claude-code, gemini-cli, codex-cli) by
adding a TypeScript in-process plugin shim that maps OpenCode's typed hooks + event bus to
Claude-shaped `HookInput` frames and invokes `unimatrix hook <EVENT> --provider opencode --model <...>`,
reusing the existing Rust ingestion pipeline unchanged. Events must land with correct `provider` AND
`source_domain` attribution persisted at ingest, subagent telemetry must align to the owning
feature/cycle, local-model activity must be queryable as distinct from cloud-model activity, and the
C17 installer must provision the plugin without regressing OpenCode's existing `mcp.unimatrix`
retrieval (C10).

## Component Map

Components from ARCHITECTURE.md §Component Breakdown (C1..C9). Pseudocode and test-plan file paths
CONFIRMED during Session 2 Stage 3a (all files present, paths verified).

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 OpenCode plugin shim (TS, net-new) | pseudocode/c1-plugin-shim.md | test-plan/c1-plugin-shim.md |
| C2 Provider normalization arm (Rust, hook.rs + new hook/opencode.rs) | pseudocode/c2-provider-arm-rust.md | test-plan/c2-provider-arm-rust.md |
| C3 Provider normalization arm (JS mirror, normalize.js) | pseudocode/c3-provider-arm-js.md | test-plan/c3-provider-arm-js.md |
| C4 Wire carriers (model_id on HookInput + ImplantEvent) | pseudocode/c4-wire-carriers.md | test-plan/c4-wire-carriers.md |
| C5 Ingest attribution persistence (schema + write path) | pseudocode/c5-ingest-persistence.md | test-plan/c5-ingest-persistence.md |
| C6 Read-path attribution (prefer stored, legacy fallback) | pseudocode/c6-read-path.md | test-plan/c6-read-path.md |
| C7 OpenCode domain pack | pseudocode/c7-domain-pack.md | test-plan/c7-domain-pack.md |
| C8 Parity corpus (opencode cases) | pseudocode/c8-parity-corpus.md | test-plan/c8-parity-corpus.md |
| C9 Installer OpenCode branch (init.js + new opencode-install.js) | pseudocode/c9-installer.md | test-plan/c9-installer.md |

### Cross-Cutting Artifacts (confirmed — Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: C1/C3/C9 are JS/TS surfaces (uni-js-dev); C2/C4/C5/C6/C7/C8 are Rust surfaces (uni-rust-dev).
C1+C3+C8 are coupled and must change together (col-022 split-brain — see Constraints).

### Stage 3b Wave Plan (from pseudocode/OVERVIEW.md dependency ordering)

Critical path: C4 → C5 → {C2, C1} → C8. col-022 override: the C2+C3+C8 triad (and C1's event map)
must land together, so the waves sequence *implementation start*, not separate merges for that triad.

- **Wave 1 (foundation):** C4 (wire carriers, Rust), C7 (domain pack, Rust), C5 schema migration (Rust)
- **Wave 2 (ingest + normalize):** C5 write-path bind (Rust), C2 (Rust provider arm)
- **Wave 3 (read + edge + plugin):** C6 (read path, Rust), C3 (JS mirror), C1 (plugin shim, TS)
- **Wave 4 (guards + provisioning):** C8 (parity corpus, Rust), C9 (installer, JS)

Delivery-time corrections flagged by Stage 3a (dev agents resolve live, gate reviews):
- Parity-corpus target file is `uds/parity_corpus_cases*.rs` + `parity_corpus_gen.rs` (drift gate),
  NOT `parity_corpus_uds.rs` — confirm at C8 implementation.
- Schema version: `CURRENT_SCHEMA_VERSION`=31 today → next is 32; resolve live, do NOT hardcode.
- OQ-4 derivation site: pin the ADR-001 ingest site that stamps stored `source_domain` (C5).
- OQ-A: two `ObservationRow` structs (write-path listener.rs vs read observations.rs) — confirm
  which carries `source_domain`/`model_id` and where it is constructed from `ImplantEvent`.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| source_domain attribution mechanism | Persist `source_domain` at ingest, stamped provider-first; fail-loud, never silent claude-code default. Event-derived read alone reads OpenCode back as claude-code. | AC-03, SR-07/SR-11, R-02 | architecture/ADR-001-source-domain-persisted-at-ingest.md |
| model_id carrier | New wire field on HookInput + ImplantEvent + new `model_id` column; plumbed plugin→ImplantEvent→observations, queryable. | AC-06, SR-02, R-10 | architecture/ADR-002-model-id-carrier.md |
| E2 sizing | AC-06 IN this cycle — rides the same migration + plumbing as AC-03; ledger guardrail gives zero credit for deferral. | OQ-1, AC-06 | architecture/ADR-003-e2-sizing-ac06-in-cycle.md |
| Ingestion topology | TS plugin shim + `--provider opencode` arm in BOTH hook.rs and normalize.js + extended parity corpus (three coupled touchpoints, #5737, col-022). | AC-01/AC-02, SR-06, R-03 | architecture/ADR-004-plugin-shim-provider-arm-parity.md |
| Modularity vs 500-line cap | New-module-with-thin-wiring; no ad-hoc monolith split. background.rs NOT carved (production write is in listener.rs; its INSERT is inside the test module). Over-cap touched files get a scheduled decomposition issue. | OQ-2, SR-05, R-17 | architecture/ADR-005-modularity-new-module-thin-wiring.md |
| Installer branch | Non-clobbering additive OpenCode branch; retrieval regression sentinel; byte-for-byte preservation of mcp.unimatrix + Ollama provider block. | AC-05, SR-08, R-08/R-09 | architecture/ADR-006-installer-opencode-branch.md |
| Subagent alignment | Bus-derived child `session.created` + `parentID` + validated `session.agent` on the observe channel (`extra.agent_type`); parent→feature/cycle correlation. Never fold into spoofable tool args. | AC-04, SR-09, R-06/R-07 | architecture/ADR-007-subagent-alignment-observe-channel.md |
| AC-07 forward-compat | Keep `external_identity` seam + per-call MCP delivery-channel viable; no enforcement, no N=1 ceremonial test. | AC-07, SR-12, R-07 | architecture/ADR-008-ac07-external-identity-forward-compat.md |
| Degraded/experimental legs | PreCompact experimental API + `session.idle`→Stop over-count absorbed into the parity-gap posture; fail-safe, documented, non-cap-failing. | AC-01, SR-03/SR-04, R-11/R-12 | architecture/ADR-009-degraded-legs-parity-gap-posture.md |

**Ingest-persistence reframe (carry into delivery):** ADR-001 changes the delivery shape from the
SCOPE's "provider flag + domain pack" framing. The `observations` table has no `source_domain`,
`provider`, or `model` column today; `source_domain` is derived at read time from `event_type` with a
`claude-code` fallback. Because OpenCode emits the same canonical event names as claude-code, a domain
pack alone cannot distinguish them — event-derived resolution reads OpenCode records back as
claude-code, and `--provider opencode` sets only the in-flight `ImplantEvent.provider`, which is then
dropped (no column). AC-03 and AC-06 are therefore achievable only by persisting attribution at
ingest via schema migration. Outcome is unchanged from SCOPE; only the mechanism deepened.

## Files to Create / Modify

### New
- `packages/unimatrix/opencode-plugin/` — C1 TS plugin shim; maps 7 canonical events, carries `--provider opencode` + `--model`, derives subagent alignment, shells to `unimatrix hook`.
- `crates/unimatrix-server/src/uds/hook/opencode.rs` — C2 opencode canonicalization module (thin-wiring target keeps hook.rs delta minimal).
- `packages/unimatrix/lib/opencode-install.js` — C9 non-clobbering installer logic module.

### Modify
- `crates/unimatrix-engine/src/wire.rs` — C4: add `model_id: Option<String>` (`#[serde(default)]`) to `HookInput` and `ImplantEvent`; regenerate ts-rs bindings.
- `crates/unimatrix-server/src/uds/hook.rs` — C2: add `"opencode"` to `KNOWN_PROVIDERS` (:158); normalizer arm delegates to `hook/opencode.rs` (over-cap: minimal wiring only).
- `packages/unimatrix/lib/hook-client/normalize.js` — C3: mirror the opencode arm (col-022).
- `crates/unimatrix-store/src/db.rs` — C5: add `source_domain TEXT`, `model_id TEXT` (nullable) to `observations` CREATE.
- `crates/unimatrix-store/src/migration.rs` — C5: additive ALTER for the two columns; bump to the next sequential schema version, resolved against `CURRENT_SCHEMA_VERSION` at delivery (32 as of migration.rs:26 today; migration is version-independent — do NOT hardcode, a concurrent PR may advance it before vnc-049 lands).
- `crates/unimatrix-server/src/uds/listener.rs` — C5: bind `source_domain`, `model_id` at `insert_observation` (:3383) and `insert_observations_batch` (:3416) (over-cap: minimal wiring only).
- `crates/unimatrix-server/src/services/observation.rs` — C6: `parse_observation_rows` (:576) SELECT new cols, prefer stored `source_domain`, surface `model_id`; `DEFAULT_HOOK_SOURCE_DOMAIN` (:572) becomes legacy-row fallback only.
- `crates/unimatrix-store/src/observations.rs` — C6: SELECT new cols in `fetch_observations_since` (:44), `load_observations_for_sessions` (:124), `load_observation_session_stats` (:173).
- `crates/unimatrix-observe/src/domain/mod.rs` — C7: add opencode builtin/config `DomainPack` (used for legacy NULL rows; `resolve_source_domain` @180 otherwise unchanged).
- `crates/unimatrix-server/src/uds/parity_corpus_uds.rs` — C8: add opencode cases across the 7 events incl. provider, model, subagent.
- `packages/unimatrix/lib/init.js` — C9: add OpenCode detection branch delegating to `opencode-install.js`.

### Untouched (preserved as sentinels)
- `opencode.json` `mcp.unimatrix` + Ollama `provider` block — preserved byte-for-byte (C10, AC-05).
- `mcp/server.rs` `build_context_with_external_identity` — kept viable, not modified (AC-07).

## Data Structures

- **HookInput** (`wire.rs`): flat JSON `{ hook_event_name, session_id, cwd, transcript_path, prompt, provider, model_id (new), mcp_context, extra{tool_name, tool_input, tool_response, agent_type, exit_code} }`, all `#[serde(default)]`.
- **ImplantEvent** (`wire.rs`): carries `provider: Option<String>`; add `model_id: Option<String>` (same attrs — `#[serde(default, skip_serializing_if="Option::is_none")]`).
- **observations row**: existing cols `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal, phase, topic_source`; add `source_domain TEXT`, `model_id TEXT` (nullable).
- **DomainPack** (`domain/mod.rs`): `{ source_domain: "opencode", event_types, categories, rules }`; `source_domain` format `^[a-z0-9_-]{1,64}$`.
- **PluginInput** (OpenCode, `@opencode-ai/plugin@1.18.31`): `{ client, project, directory, worktree, $ (Bun shell) }`; plugin returns a `Hooks` object.

## Function Signatures

- `const KNOWN_PROVIDERS: &[&str] = &["claude-code","gemini-cli","codex-cli"]` → add `"opencode"` (`hook.rs:158`).
- `normalize_event_name` / `map_to_canonical` (event-name → canonical, inferred_provider) — add opencode arm (`hook.rs:66-105`).
- `insert_observation(...)` (`listener.rs:3383`), `insert_observations_batch(...)` (`listener.rs:3416`) — bind `source_domain`, `model_id`.
- `parse_observation_rows(...)` (`observation.rs:576`) — SELECT + prefer stored `source_domain`, surface `model_id`.
- `resolve_source_domain(&self, event_type: &str) -> String` (`domain/mod.rs:180`) — unchanged; legacy NULL rows only.
- `build_context_with_external_identity(..., external_identity: Option<&ResolvedIdentity>)` (`mcp/server.rs`) — untouched, kept callable with a value.
- OpenCode plugin: `Plugin(input, opts) => Promise<Hooks>`.

### OpenCode → canonical event map (C1 shim)

| Canonical | OpenCode source | Reachability |
|---|---|---|
| SessionStart | `session.created` (+`session.updated`) | bus-derived (degraded; derive cwd from PluginInput.directory) |
| UserPromptSubmit | `chat.message` | reachable (carries model) |
| PreToolUse | `tool.execute.before` | reachable |
| PostToolUse | `tool.execute.after` | reachable |
| SubagentStart | child `session.created` + `parentID` + `session.agent` | observe-only; injection unreachable (parity gap) |
| PreCompact | `experimental.session.compacting` | reachable (unstable API — ADR-009) |
| Stop | `session.idle` | bus-derived (degraded; over-count guard — ADR-009) |

## Constraints

- **C-1 Split-brain (col-022):** the opencode arm lands in `hook.rs`/`hook/opencode.rs` AND `normalize.js` together, guarded by the extended parity corpus. C1 (plugin) + C3 (normalize.js) + C8 (corpus) change together; editing one side is a known defect class.
- **C-2 C17 regression sentinel (#5582):** `mcp.unimatrix` retrieval / local STDIO must not regress; installer delta strictly additive.
- **C-3 source_domain event-derived server-side:** defaults to `claude-code` on the hook path; `--provider opencode` alone is insufficient — an explicit provider-first ingest stamp is required and must fail loud, never silently default.
- **C-4 No stdin command contract in OpenCode:** every event passes through the plugin shim; the `--provider opencode` arm is necessary but not sufficient.
- **C-5 500 code-line cap (tests excluded):** judged in code-lines, not raw lines. New-module-with-thin-wiring; over-cap touched files (`hook.rs`, `listener.rs`, `observation.rs`, `db.rs`, `migration.rs`) get only minimal wiring plus a scheduled decomposition issue. New modules ship at/under cap.
- **C-6 PreCompact experimental dependency:** rests on `experimental.session.compacting` (flagged unstable); document as degraded/experimental, gate or flag it; instability must not fail the cap.
- **C-7 E1 forward-compat only:** keep the seam viable, build no enforcement.
- **C-8 C10 preserved:** remote/local retrieval is the regression sentinel; preserved, not modified.
- **NFR-02 fail-open:** a failure to emit an event must not block or crash the OpenCode session.
- **NFR-06 installer idempotence:** re-running `unimatrix init` produces no duplicate entries / no key drift.
- **Lesson #5670:** hook-frame changes (`model_id`) must land at Rust hook.rs (extract+insert), JS hook-client port, a parity-corpus case, and one client→listener→DB crossing test.

## Dependencies

- **C17 installer provisioning (#5582)** — prerequisite for AC-05. Reuse the nan-004 (#1201) prefix-match non-clobber merge principle (surface differs: plugin dir + `plugin[]` array).
- **C10 remote/local retrieval (#5547)** — proven; preserved as regression sentinel.
- **Existing Rust pipeline (reused unchanged)** — UDS transport, queue, fail-open, drop-detector, listener, storage.
- **OpenCode plugin API** — `@opencode-ai/plugin@1.18.31` (sole dep in `.opencode/package.json`; no plugin code exists yet).
- **Prior conventions** — provider-field ADR (#4306, vnc-013); harness-onboarding three-touchpoints pattern (#5737); parity corpus (#4751, vnc-026); ts-rs binding drift gate (vnc-024 #4726).

## NOT in Scope

- SubagentStart retrieval-injection — architecturally unreachable in OpenCode; observation in scope, injection is an accepted parity gap.
- Full E1 trusted-identity design/delivery — demand-pulled under #5734; only the AC-07 forward-compat constraint applies. No enforcement, no PL-4 fail-closed flip, no MCP-proxy shim.
- Command-hook mirror (`.codex/hooks.json` model) — architecturally impossible (ass-106 A/C).
- Bringing whole `background.rs`/`listener.rs`/`hook.rs` under the 500 code-line cap — targeted carve deferred to a scheduled decomposition issue.
- Active-governance / policy surfaces (mutable `chat.*` injection, `permission.ask` as a policy decision point).

## Alignment Status

Vision guardian (2026-09-15): 5 PASS, 1 WARN, no FAIL/hard variance.

- **PASS** — Vision Alignment, Milestone Fit, Scope Gaps, Architecture Consistency, Risk Completeness. C18 is the named next client for personal-cloud (#5731); attribution-at-ingest strengthens Knowledge Integrity (#5681); AC-07 builds forward-compat seam only (correct milestone discipline for #5734).
- **DECIDED (2026-09-15 human ruling) — opencode-only stamp.** The write-time source_domain stamp applies only to `provider == "opencode"`; `claude-code`/`gemini-cli`/`codex-cli` rows stay NULL and read back via today's read-derived resolution — existing behavior preserved byte-for-byte, **zero T-SEC-12/13 change**. `model_id` (ADR-002) is naturally opencode-only. Generalizing to all harnesses is a different outcome (integrity #5681 / C11 / T-SEC re-baseline), not C18's last mile; it earns its own cycle if ever wanted. Resolves OQ-2 / R-05 / vision WARN.

## C18 Ledger Guardrail (authoritative)

C18 does NOT reach `proven` on AC-01..05 alone. Until AC-06's local-vs-cloud distinctness is
behaviorally demonstrated on the real assembled plugin→wire→ingest-persist→store→query path (R-01
gating test), C18 stays `partial` (honest-visible-partial), including the interim if E2 fast-follows.
The E2-sizing latitude is the architect's call (resolved: IN this cycle, ADR-003); the AC-06 criterion
is authoritative regardless of when it lands.

## Open Questions Carried to Delivery

- **OQ-2 / R-05 — RESOLVED (opencode-only, 2026-09-15):** write-time source_domain stamp applies only to opencode rows; non-opencode rows stay NULL → read-derived fallback → T-SEC-12/13 unchanged. No longer open. Delivery must assert the stamp is opencode-only and that T-SEC-12/13 stay green unchanged.
- **OQ-3 / R-11 (`session.idle`→Stop):** fires once per Stop with derivable `duration`/`outcome`, or on every idle transition (over-count)? Needs a delivery-time PoC; ADR-009 mandates an over-count guard either way.
- **OQ-4 / R-02 (derivation-site reconciliation):** two conflicting sites (#4306 listener Site A `provider.unwrap_or("claude-code")` vs `DomainPackRegistry` DEFAULT on the hook path). Pin WHICH site produces the stored `source_domain` so a future edit to the other cannot silently reintroduce the default.
- **OQ-5 (model carrier shape):** payload field vs new typed `ImplantEvent` field — resolved to new typed field (ADR-002); confirm parity-corpus coverage.
- **Schema version (resolved):** the next sequential schema version, resolved against `CURRENT_SCHEMA_VERSION` at delivery — 32 as of migration.rs:26 today; do NOT hardcode (a concurrent PR may advance it before vnc-049 lands).
- **AC-04 ordering race (R-06):** child `session.created` may arrive before parent registration — correlation must still resolve (no lost parent).
- **OQ-6 (subagent MCP connection):** do OpenCode child sessions share the parent's MCP client or get their own? Gates AC-07 delivery-channel (i) vs proxy (ii). Does NOT gate this cycle's delivery.
- **OQ-7 (PreCompact stability):** gate behind a plugin flag now, or defer the leg? ADR-009 flags it degraded; delivery decides the flag default.
