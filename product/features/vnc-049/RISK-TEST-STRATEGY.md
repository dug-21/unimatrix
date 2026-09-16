# Risk-Based Test Strategy: vnc-049

OpenCode observation harness — behavioral-signal parity + local-model attribution (C18).

Grounding: SCOPE-RISK-ASSESSMENT.md (SR-01..SR-12), ARCHITECTURE.md (ADR-001..009, C1..C9),
SPECIFICATION.md (FR-01..12, AC-01..07, NFR-01..06). Historical evidence: #4177 (tautology caught
at gate), #4876 (empirical not structural verification), #3548 (test exists but omits the
test-plan assertion), #2758 (grep non-negotiable test names before accepting PASS), #5302
(single-source the full CONTRACT, not just DATA — parity drift), #4373 / #4153 (schema-version
cascade: three paths + parity + back-fill + idempotency), #907/#918/#930 (proven-but-holed).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| **R-01** | **AC-06 GATING.** Local-model distinctness proven at a seam / injected dependency / "carrier field populated" tautology, not on the real assembled plugin→wire→ingest-persist→store→query path. C18 ships green while ollama/qwen3 is indistinguishable from cloud on the user's path. | High | High | **Critical (gating)** |
| **R-02** | **Silent `source_domain=claude-code`.** OQ-4 exposes two conflicting derivation sites (listener Site A `provider.unwrap_or("claude-code")` per #4306 vs `DomainPackRegistry` DEFAULT on the hook path). If write-time provider-first stamp (ADR-001) lands at the wrong site, opencode rows read back `claude-code` — green but mis-attributed. | High | High | **Critical** |
| **R-03** | **Split-brain drift (col-022).** opencode arm added to `hook.rs`/`hook/opencode.rs` but not mirrored in `normalize.js`, or parity corpus not extended — the two normalizers diverge silently (#5302: single-source the contract not just data). | High | High | **Critical** |
| **R-04** | **Migration cascade incomplete.** New `source_domain`/`model_id` columns + schema-version bump (next sequential version, resolved against `CURRENT_SCHEMA_VERSION` at delivery — 32 today; do NOT hardcode) miss one of the three required paths (#4153) or the cascade checklist (#4373: column-count, parity, back-fill/idempotency, existing migration-test assertions). Existing migration tests break or new rows land NULL. | High | High | **Critical** |
| **R-05** | **Existing-provider source_domain regression (T-SEC-12/13).** **RESOLVED (2026-09-15): opencode-only stamp** — the write-time stamp applies only to `provider == "opencode"`; non-opencode rows stay NULL → read-derived fallback → existing behavior byte-for-byte, no semantics change. Residual: a test must assert the stamp is opencode-only and T-SEC-12/13 stay green unchanged. | Low | Low | **Low (resolved)** |
| **R-06** | **Subagent orphaning (AC-04 show-stopper).** `parentID`→parent session→feature/cycle correlation fails; child telemetry lands unaligned/orphaned. Bus-derived `session.created` may arrive before parent is registered (ordering race). | High | Med | **High** |
| **R-07** | **`session.agent` on a spoofable channel.** Validated `session.agent` folded into MCP tool args instead of the observe channel (`extra.agent_type`) — corrupts AC-04 alignment and forecloses the AC-07 identity seam (ADR-007/008). | High | Med | **High** |
| **R-08** | **C10 retrieval regression on install (AC-05).** Installer branch (C9) touching `opencode.json` clobbers `mcp.unimatrix`, the local STDIO command, or the Ollama `provider` block — retrieval stops returning. | High | Med | **High** |
| **R-09** | **Installer non-idempotence (NFR-06).** Re-running `unimatrix init` duplicates the `plugin:[]` entry / package dep or drifts preserved keys. | Med | Med | **Medium** |
| **R-10** | **model_id carrier blast radius (ADR-002).** New `Option<String>` field on `HookInput`+`ImplantEvent` breaks the ts-rs binding drift gate (vnc-024 #4726), serde back-compat, or is dropped between wire and INSERT. | Med | Med | **Medium** |
| **R-11** | **`session.idle`→Stop over-count (ADR-009 / SR-04).** `session.idle` firing on every idle transition inflates Stop count / miscomputes plugin-side `duration`/`outcome`. Degraded-leg / parity-gap posture — a data-quality guard, NOT a cap failure. | Med | Med | **Medium** |
| **R-12** | **PreCompact experimental API (ADR-009 / SR-03).** `experimental.session.compacting` changes/vanishes upstream; leg breaks. Degraded/flagged-leg posture — must fail safe and stay documented, NOT fail the cap. | Med | Med | **Medium** |
| **R-13** | **Plugin transport fail-open (NFR-02).** Per-event `$ unimatrix hook` shell/UDS failure (binary absent, socket down) blocks or crashes the user's OpenCode session instead of failing open. | Med | Med | **Medium** |
| **R-14** | **Bus-derived field synthesis corruption.** SessionStart/Stop lack `transcript_path`; `cwd`/`worktree` derived from `PluginInput`. Bad derivation stores wrong/empty fields as if full-fidelity. | Med | Low | **Medium** |
| **R-15** | **Untrusted plugin/installer input (security).** Event payloads, `model:{providerID,modelID}`, tool args, and `opencode.json` are untrusted; unvalidated `model_id`/`source_domain` breaks the `^[a-z0-9_-]{1,64}$` contract or reaches SQL/paths. | Med | Low | **Medium** |
| **R-16** | **AC-01 false-pass on parity gap.** Unreachable/degraded legs (SubagentStart injection, degraded Stop) reported as passing full-fidelity, or silently dropping data, instead of recorded as the measured parity gap (FR-12). | Med | Med | **Medium** |
| **R-17** | **Over-cap wiring surgery (ADR-005).** Minimal wiring into over-cap `listener.rs`/`hook.rs`/`observation.rs`/`db.rs`/`migration.rs` regresses adjacent untested code, or an ad-hoc mid-delivery carve introduces untested surgery. | Low | Med | **Low** |

## Risk-to-Scenario Mapping

### R-01: AC-06 proven-but-holed — local-model distinctness (GATING)
**Severity**: High | **Likelihood**: High | **Impact**: C18 declared `proven`/complete while a
user's ollama/qwen3-coder session is indistinguishable from a cloud model — the harness observes but
cannot deliver its raison d'être (#907/#918/#930; SR-01/SR-10).

**Test Scenarios** (this is the C18 gating test):
1. **End-path distinctness (authoritative).** Drive a real OpenCode-shaped LOCAL-model event
   (`model:{providerID:"ollama", modelID:"qwen3-coder"}`) through the assembled path — plugin shim →
   `unimatrix hook --provider opencode --model ...` → UDS → normalizer → `ImplantEvent` →
   ingest-persist INSERT → store — then QUERY the stored record back and assert it is distinguishable
   from a separately-ingested CLOUD-model OpenCode event (e.g. a hosted model_id). Distinctness must
   be observed on the returned/queried record, not on `ImplantEvent` in memory.
2. **Negative / anti-tautology.** The assertion must fail if the carrier is dropped between wire and
   INSERT, homogenized, or NULLed. No injected/mocked attribution dependency; no assertion that stops
   at "the `--model` flag was passed" or "the field is Some(...)" (#4177, #4876).
3. Two local models (e.g. `qwen3-coder` vs `llama3`) are also mutually distinguishable on query.

**Coverage Requirement**: A behavioral end-path test drives a real local-model OpenCode event and
queries back a stored record provably distinct from a cloud-model record, through
plugin→wire→ingest-persist→store→query, with a negative assertion. **C18 does not reach `proven`
until this passes** (ledger guardrail; holds regardless of E2 in-cycle vs fast-follow — if it is
not demonstrated, C18 is `partial`).

### R-02: Silent source_domain=claude-code default (AC-03)
**Severity**: High | **Likelihood**: High | **Impact**: opencode events land mis-attributed as
`claude-code` but tests pass — the SR-07/SR-11 silent-default class.

**Test Scenarios**:
1. **Stored-record assertion.** Ingest a real opencode event through the assembled path; SELECT the
   stored row and assert `source_domain = "opencode"`.
2. **Negative assertion (mandatory).** Assert the same stored row is NOT `source_domain =
   "claude-code"` (per AC-03 verification method).
3. **Site reconciliation (OQ-4).** A test pins WHICH derivation site produces the stored value, so a
   future edit to the other site cannot silently reintroduce the default.
4. **Fail-loud.** An opencode event whose provider-first stamp cannot resolve fails loud, never
   silently falls to `claude-code`.

**Coverage Requirement**: Stored-record positive + negative assertion on `source_domain` from a real
opencode event; the mechanism site is pinned.

### R-03: Split-brain drift (AC-02, col-022)
**Severity**: High | **Likelihood**: High | **Impact**: hook.rs and normalize.js canonicalize
opencode differently; edge client and server disagree — the col-022 defect class.

**Test Scenarios**:
1. Extended `parity_corpus_uds.rs` covers opencode across all 7 canonical events (provider, model,
   subagent cases) and is green — Rust ↔ JS canonicalization identical.
2. Corpus fails if either arm is edited without the other (drift sentinel; #5302 — assert on the
   full contract, not just shared data).
3. Unit assertion: `"opencode"` ∈ `KNOWN_PROVIDERS`.
4. A stored opencode record shows `provider = "opencode"` (AC-02c).

**Coverage Requirement**: Parity corpus extended and green, drift-failing; provider present on the
stored record.

### R-04: Migration cascade incomplete (schema bump — next sequential version, resolved at delivery)
**Severity**: High | **Likelihood**: High | **Impact**: columns missing on fresh vs migrated DBs,
existing migration tests break, or new rows land NULL. (#4153 three paths; #4373 cascade checklist.)

**Test Scenarios**:
1. **Fresh-create** (`db.rs` CREATE) and **ALTER-migrate** (`migration.rs`) both yield the
   `source_domain`/`model_id` columns — column-count parity test.
2. **Legacy-NULL read path (ADR-001 / C6).** A pre-migration row (NULL `source_domain`) reads back
   via the registry/DEFAULT fallback exactly as today — new rows prefer the stored value. Both
   branches of the read-path fork asserted.
3. Schema-version bump lands in all three required paths; existing migration-test assertions updated
   (#4153).
4. Migration idempotent / back-fill row-count asserted (#4373); re-run does not double-apply.

**Coverage Requirement**: Fresh + migrated column parity, legacy-NULL fallback read path asserted on
both branches, version-cascade three-path + idempotency coverage.

### R-05: Existing-provider source_domain regression (T-SEC-12/13) — RESOLVED opencode-only
**Severity**: Low (resolved) | **Likelihood**: Low | **Impact**: none under the decided scope.
**DECIDED 2026-09-15 (human ruling):** write-time stamp is opencode-only; non-opencode rows stay NULL
→ read-derived fallback → existing behavior byte-for-byte. Generalization to all harnesses is a
different outcome deferred to its own cycle (integrity #5681 / C11 / T-SEC re-baseline).

**Test Scenarios**:
1. Assert the write-time stamp fires ONLY for `provider == "opencode"`; a gemini-cli/codex-cli/claude-code
   event leaves `source_domain` NULL at write.
2. Existing `source_domain` tests (T-SEC-12/13, observation.rs) re-run **green, unchanged** — no
   semantic shift for shipped harnesses.
3. A claude-code event still resolves its expected read-derived `source_domain` after the change.

**Coverage Requirement**: T-SEC-12/13 pass unchanged; a test proves the stamp is opencode-only.

### R-06: Subagent orphaning (AC-04 show-stopper)
**Severity**: High | **Likelihood**: Med | **Impact**: subagent telemetry lands unaligned to the
owning feature/cycle.

**Test Scenarios**:
1. Drive child `session.created` with non-null `parentID` + validated `session.agent`; assert the
   **stored** subagent record is aligned to the owning feature/cycle (parent→cycle correlation
   resolved), not orphaned.
2. Assert `session.agent` is captured from the validated harness field, NOT from spoofable tool args.
3. Ordering: child event arriving before/around parent registration still correlates (no lost parent).

**Coverage Requirement**: Stored subagent record aligned to feature/cycle; validated-field provenance
asserted.

### R-07: session.agent on a spoofable channel (AC-04/AC-07)
**Severity**: High | **Likelihood**: Med | **Impact**: identity corruptible; AC-07 seam foreclosed.

**Test Scenarios**:
1. Assert validated `session.agent` rides `extra.agent_type` (observe channel), never MCP tool args.
2. A crafted event with a conflicting agent value in tool args does NOT override the validated field.

**Coverage Requirement**: Observe-channel provenance test + spoof-rejection test.

### R-08: C10 retrieval regression on install (AC-05)
**Severity**: High | **Likelihood**: Med | **Impact**: installer clobbers retrieval config.

**Test Scenarios**:
1. Installer runs on a fixture OpenCode repo; assert `mcp.unimatrix`, local STDIO command, Ollama
   `provider` block, and non-Unimatrix keys preserved **byte-for-byte**.
2. Post-provision retrieval regression test: `context_*` still returns results.
3. Plugin actually provisioned (`.opencode/plugins/` + package dep and/or `plugin:[]` append).

**Coverage Requirement**: Byte-for-byte preservation + retrieval-still-returns after provisioning.

### R-09: Installer non-idempotence (NFR-06)
**Severity**: Med | **Likelihood**: Med
**Test Scenarios**: Re-run installer twice; assert no duplicate plugin/package entries and no drift in
preserved keys.
**Coverage Requirement**: Idempotence assertion on a re-run.

### R-10: model_id carrier blast radius (ADR-002)
**Severity**: Med | **Likelihood**: Med
**Test Scenarios**: (1) ts-rs binding regeneration + drift gate green (#4726). (2) serde back-compat:
a frame WITHOUT `model_id` still deserializes (`#[serde(default)]`). (3) client→listener→DB crossing
test carries `model_id` end to end (lesson #5670).
**Coverage Requirement**: Binding drift gate green; back-compat + one crossing test.

### R-11: session.idle→Stop over-count (ADR-009 / SR-04) — degraded-leg posture
**Severity**: Med | **Likelihood**: Med
**Test Scenarios**: (1) Repeated `session.idle` transitions produce exactly one Stop (over-count
guard). (2) Stop/SessionStart recorded as degraded/bus-derived, not full-fidelity. Framed as a
data-quality guard; instability does NOT fail the cap.
**Coverage Requirement**: Over-count guard asserted; degraded-leg documented, non-cap-failing.

### R-12: PreCompact experimental API (ADR-009 / SR-03) — degraded-leg posture
**Severity**: Med | **Likelihood**: Med
**Test Scenarios**: (1) With the experimental API present, PreCompact lands. (2) With it
absent/changed, the leg fails safe (flag/gate default), does not crash the session, and is documented
as the measured parity gap — NOT a cap failure.
**Coverage Requirement**: Present-path landing + absent-path fail-safe.

### R-13: Plugin transport fail-open (NFR-02)
**Severity**: Med | **Likelihood**: Med
**Test Scenarios**: With the `unimatrix` binary/socket unavailable, the plugin does not block or crash
the OpenCode session (fail-open preserved). No pipeline rewrite — reuses queue/fail-open/drop-detector.
**Coverage Requirement**: Session survives an emit failure.

### R-14: Bus-derived field synthesis corruption
**Severity**: Med | **Likelihood**: Low
**Test Scenarios**: SessionStart/Stop derive `cwd`/`worktree` from `PluginInput` correctly; absent
`transcript_path` handled without storing a bogus value; empty/malformed bus payload does not store
corrupt fields as full-fidelity.
**Coverage Requirement**: Derived fields correct; missing fields degrade honestly.

### R-15 / R-16 / R-17
See Security Risks (R-15), Failure Modes / AC-01 parity-gap (R-16), and Edge Cases / Integration
Risks (R-17) below.

## Behavioral-Outcome Coverage (from the scope behavioral lens)

Every entry point + outcome in SCOPE-RISK-ASSESSMENT.md's behavioral lens gets a REQUIRED scenario
that proves the outcome FROM the user's actual entry point — a seam/sub-entry-point test does NOT
discharge these.

| Outcome (from scope lens) | Entry point | Required scenario (drives the real entry point, asserts the outcome) |
|---|---|---|
| All reachable canonical events land as queryable records; unreachable legs = measured parity gap, not silent loss | OpenCode session with plugin | Drive each of the 7 OpenCode-shaped inputs through the plugin shim → `unimatrix hook`; assert each reachable/bus-derived event is a stored, queryable record; assert unreachable (SubagentStart injection) is recorded as parity gap with no false-pass and no silent drop (R-16, AC-01) |
| Local activity stored + queryable DISTINCT from cloud, through the real assembled path | OpenCode against a LOCAL model | R-01 scenario 1–2: real local-model event end-to-end, queried back distinct from a cloud-model record, with negative assertion. **Gating.** (AC-06) |
| Stored record shows `source_domain=opencode` AND `provider=opencode`, NOT the claude-code default | Any OpenCode event emitted | R-02 (stored `source_domain=opencode` + negative vs `claude-code`) AND R-03.4 (stored `provider=opencode`) — both from a real opencode event, asserted on the stored row (AC-02c/AC-03) |
| Subagent telemetry observable AND aligned to owning feature/cycle | Spawn a subagent in OpenCode | R-06.1: child `session.created`+`parentID`+validated `session.agent` → stored record aligned to feature/cycle; R-07 provenance (AC-04) |
| Plugin provisioned AND `context_*` retrieval still returns (C10 preserved) | User runs `unimatrix init` in an OpenCode repo | R-08.1–3: real installer run on a fixture repo, byte-for-byte preservation + retrieval still returns + plugin present (AC-05) |

A test that proves any of these one layer BENEATH the user's entry point (an injected-attribution
seam, a normalizer unit test, an in-memory `ImplantEvent`) does NOT discharge the row — the scenario
must drive the plugin/CLI/installer entry point the user invokes and assert the stored/queried
outcome.

## Integration Risks

- **Two source_domain derivation sites (OQ-4).** #4306 listener Site A vs `DomainPackRegistry` DEFAULT
  disagree; AC-03 must flow through a pinned site (R-02).
- **Wire→INSERT drop.** `model_id`/`provider` present on `ImplantEvent` but not bound at
  `insert_observation`/`insert_observations_batch` (`listener.rs:3383/3416`) — carrier silently lost
  (R-01, R-10).
- **Cross-language contract.** hook.rs ↔ normalize.js ↔ parity corpus must single-source the full
  contract, not just shared data (#5302; R-03).
- **Schema version coupling.** the schema-version bump (next sequential, resolved against `CURRENT_SCHEMA_VERSION` at delivery — do NOT hardcode) touches three paths + existing migration-test assertions
  (#4153/#4373; R-04).
- **Subagent event ordering.** Child `session.created` correlation to a possibly-not-yet-registered
  parent session (R-06).
- **Existing-provider generalization.** Write-time stamp affects gemini-cli/codex-cli rows and
  T-SEC-12/13 (R-05).

## Edge Cases

- Pre-migration (legacy NULL) observation rows read back via fallback (R-04.2).
- Frame with no `model_id` (cloud model, or field absent) — deserializes and stores cleanly (R-10.2).
- `session.idle` firing repeatedly → single Stop (R-11).
- PreCompact experimental API absent/renamed (R-12).
- Empty/malformed bus payload; missing `transcript_path`; `cwd` underivable (R-14).
- Installer run twice; `opencode.json` with pre-existing partial plugin array (R-09).
- `model_id`/`source_domain` exceeding or violating `^[a-z0-9_-]{1,64}$` (R-15).

## Security Risks

Untrusted-input surfaces and blast radius:
- **Plugin event payloads (C1).** OpenCode event bus / typed-hook data (prompt, tool args, tool
  output, `model:{providerID,modelID}`) is untrusted. Blast radius: fields flow into `HookInput` →
  `ImplantEvent` → DB INSERT. `model_id` and `source_domain` MUST be validated against
  `^[a-z0-9_-]{1,64}$`; reject/sanitize, never pass raw to SQL or path construction (R-15).
- **Spoofable identity (R-07).** `session.agent` must come only from OpenCode's validated
  `agent.get()` field on the observe channel; tool args are attacker-controllable and must never set
  agent identity. Protects AC-04 alignment integrity and the AC-07 seam.
- **Installer file writes (C9).** `opencode.json` and `.opencode/` are user-controlled; installer
  merge must not follow injected paths, must preserve non-Unimatrix keys byte-for-byte, and must not
  execute untrusted content. Blast radius: clobbered retrieval (C10) or arbitrary file write.
- **model_id as an attribution axis.** A crafted `model_id` could impersonate another backend model;
  distinctness (AC-06) relies on it being validated and faithfully persisted, not spoof-swapped.

## Failure Modes

- **Transport failure** (R-13): plugin fails open — event dropped, OpenCode session continues; reuse
  existing drop-detector, never crash/block.
- **Attribution unresolvable** (R-02.4): fail loud, never silently default to `claude-code`.
- **Experimental leg unavailable** (R-12): fail safe behind the flag/gate default; record parity gap.
- **Degraded bus legs** (R-11, R-14): store honestly as degraded/bus-derived; guard over-count; never
  present as full-fidelity and never silently corrupt.
- **Parity gap** (R-16 / FR-12): unreachable legs (SubagentStart injection) recorded as measured
  parity gap — no false-pass, no silent drop.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (AC-06 proven-but-holed cap) | R-01 | ADR-002/003 persist model_id + AC-06 end-path gating test; C18 `partial` until behaviorally demonstrated |
| SR-02 (no model-id carrier) | R-10, R-01 | ADR-002 new wire field + column, parity-corpus + crossing-test coverage |
| SR-03 (PreCompact experimental) | R-12 | ADR-009 degraded/flagged leg; present-path + fail-safe absent-path; non-cap-failing |
| SR-04 (session.idle→Stop over-count) | R-11 | ADR-009 over-count guard; degraded-leg posture, not a cap failure |
| SR-05 (500 code-line cap vs touched files) | R-17 | ADR-005 new-module-thin-wiring; scheduled decomposition; re-measured code-lines |
| SR-06 (split-brain drift col-022) | R-03 | ADR-004 both arms + extended parity corpus (AC-02), drift-failing |
| SR-07 (silent claude-code default) | R-02 | ADR-001 provider-first persist at ingest; stored-record + negative assertion (AC-03) |
| SR-08 (C10 retrieval regression) | R-08, R-09 | ADR-006 additive install branch; byte-for-byte + retrieval + idempotence tests (AC-05) |
| SR-09 (subagent alignment) | R-06, R-07 | ADR-007 validated session.agent on observe channel + parent→cycle alignment (AC-04) |
| SR-10 (path-divergence: seam vs real local path) | R-01 | AC-06 asserted end-to-end from the user's local-model session, not a seam |
| SR-11 (accepted-but-inert source_domain) | R-02 | Fail-loud provider-first stamp; negative assertion vs claude-code |
| SR-12 (AC-07 ceremonial seam) | R-07 | ADR-008 keep seam viable, no N=1 ceremonial test, no enforcement built |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01..R-04) | 14 scenarios — incl. the AC-06 end-path gating test and its negative assertion |
| High | 4 (R-05..R-08) | 11 scenarios |
| Medium | 8 (R-09..R-16) | 17 scenarios |
| Low | 1 (R-17) | 1 scenario |

**Gating test (C18 `proven` blocker):** R-01 scenario 1+2 — a real local-model OpenCode event LANDS
and is QUERYABLE as distinct from a cloud-model event through plugin→wire→ingest-persist→store→query,
with a negative assertion. Not a seam, not a populated-field check, not an injected dependency.
