# vnc-049 Architecture — OpenCode as the fourth observation harness (C18)

Feature: vnc-049 | GH #986 | Capability C18 | Scope: `product/features/vnc-049/SCOPE.md` (APPROVED)
Grounding: ass-106 findings (GH #982), SCOPE-RISK-ASSESSMENT.md, pattern #5737, ADRs #4306 (vnc-013 provider), #4751 (vnc-026 parity corpus), lesson #5670, PL-10 (#5715), external-identity seam #4357.

## System Overview

OpenCode already retrieves from Unimatrix (MCP `context_*` via `opencode.json`, C10 — proven) but
emits **no behavioral signal**. vnc-049 makes OpenCode the fourth observation harness (after
claude-code, gemini-cli, codex-cli) by feeding the **existing Rust ingestion pipeline unchanged**:
UDS transport, queue, fail-open, drop-detector, listener, storage.

OpenCode has **no command-hook contract** (no stdin/exit-code mechanism — ass-106 finding A). Its
only extension surface is an **in-process TypeScript plugin API**. So the normalization boundary that
claude-code/codex get from a stdin schema must instead live in a **TS plugin shim** that maps each
OpenCode typed hook / bus event to a Claude-shaped `HookInput` and shells to
`unimatrix hook <EVENT> --provider opencode`. The Rust deltas are narrow: a provider normalization
arm, ingest-time attribution persistence, and a model carrier.

The load-bearing discovery from codebase verification (drives ADR-001/002): **`source_domain` is not
persisted today** — the `observations` table has no `source_domain`, `provider`, or `model` column
(`crates/unimatrix-store/src/db.rs`). `source_domain` is derived at **read** time from `event_type`
via `DomainPackRegistry::resolve_source_domain` with a `claude-code` fallback. Because OpenCode emits
the **same canonical event names** as claude-code (`PreToolUse`, …), event-derived resolution cannot
distinguish an OpenCode event from a claude-code one — it reads back `claude-code`. `--provider
opencode` sets only the in-flight `ImplantEvent.provider`, which is then **dropped** (no column). This
is exactly SR-07/SR-11 (silent claude-code default) and it means AC-03 and AC-06 are **not** achievable
by "add a provider flag + a domain pack." Attribution must be **persisted at ingest**.

## Component Breakdown

| # | Component | Responsibility | Location | Status |
|---|-----------|----------------|----------|--------|
| C1 | **OpenCode plugin shim** | Map 7 canonical events from typed hooks + event bus to Claude-shaped `HookInput`; carry `--provider opencode`, `--model`; derive subagent alignment; shell to `unimatrix hook` | `packages/unimatrix/opencode-plugin/` (net-new) | New |
| C2 | **Provider normalization arm (Rust)** | Canonicalize OpenCode event names, stamp provider; `opencode` in `KNOWN_PROVIDERS` | `crates/unimatrix-server/src/uds/hook.rs` (+ new `hook/opencode.rs`) | Edit (over-cap) + new module |
| C3 | **Provider normalization arm (JS mirror)** | Byte-parity port of C2 (col-022 split-brain) | `packages/unimatrix/lib/hook-client/normalize.js` | Edit |
| C4 | **Wire carriers** | `model_id` field on `HookInput` + `ImplantEvent` | `crates/unimatrix-engine/src/wire.rs` | Edit (under-cap) |
| C5 | **Ingest attribution persistence** | New `source_domain` + `model_id` columns; stamp at write from `ImplantEvent.provider`/`model_id` | `unimatrix-store/src/db.rs`, `migration.rs`; `uds/listener.rs` insert sites | Edit (schema + over-cap wiring) |
| C6 | **Read-path attribution** | Prefer stored `source_domain`; surface `model_id`; legacy NULL rows fall back to registry-resolve/DEFAULT | `services/observation.rs` `parse_observation_rows`, `unimatrix-store/src/observations.rs` | Edit |
| C7 | **OpenCode domain pack** | Read-path/category registration + legacy-row resolution | `crates/unimatrix-observe/src/domain/mod.rs` | Edit (under-cap) |
| C8 | **Parity corpus** | OpenCode cases across the 7 events incl. provider, model, subagent | `crates/unimatrix-server/src/uds/parity_corpus_uds.rs` | Edit |
| C9 | **Installer OpenCode branch** | Detect `opencode.json`/`.opencode/`; provision plugin non-clobbering; preserve `mcp.unimatrix`/provider byte-for-byte | `packages/unimatrix/lib/init.js` (+ new `opencode-install.js`) | Edit + new module |

## Component Interactions & Data Flow

### Observation write path (the assembled path AC-03/AC-06 are asserted from)
```
OpenCode runtime (Bun)
  └─ C1 plugin: event bus / typed hook fires
       ├─ maps to Claude-shaped HookInput { hook_event_name, session_id, prompt, extra{...} }
       ├─ resolves in-process model:{providerID,modelID} → --model "ollama/qwen3-coder"
       ├─ resolves parentID → parent feature/cycle (AC-04)
       └─ $ unimatrix hook <EVENT> --provider opencode --model <...>   (UDS, existing transport)
            └─ C2 hook.rs: normalize_event_name + KNOWN_PROVIDERS → canonical name, provider=opencode
                 └─ builds ImplantEvent { provider: Some("opencode"), model_id: Some(...) }  (C4)
                      └─ listener.rs insert_observation / _batch (C5):
                           INSERT ... source_domain = derive(provider), model_id = event.model_id
                                └─ observations row now CARRIES source_domain + model_id
   Read: parse_observation_rows (C6) → prefer stored source_domain (="opencode"), surface model_id
        → queryable record distinct from cloud-model + from claude-code
```

`source_domain` derivation at write is **provider-first and fail-loud** (ADR-001): a record whose
provider is `opencode` stores `source_domain="opencode"`; it never silently falls to `claude-code`.
Legacy rows (NULL column) keep today's read-path registry/DEFAULT behavior — backward compatible.

### Subagent alignment (AC-04)
Child `session.created` with `parentID != null` + validated `session.agent` (OpenCode validates via
`agent.get()`, LLM out of loop). C1 maps it to a `SubagentStart` `HookInput` carrying the validated
agent as `extra.agent_type` (the **observe channel**), and resolves `parentID → parent session →
feature/cycle` for alignment. The validated `session.agent` is **never** folded into MCP tool args
(spoofable channel) — this protects AC-07 (ADR-007/008, SR-09/SR-12).

### Retrieval path (C10 — preserved, regression sentinel)
`mcp.unimatrix` in `opencode.json` is untouched. The installer delta (C9) is strictly additive
(plugin dir + `.opencode/package.json` dep, or `plugin:[]` append); `mcp.unimatrix` + the local
Ollama `provider` block are preserved byte-for-byte (AC-05/SR-08).

## Technology Decisions (see ADRs)

| ADR | Decision |
|-----|----------|
| ADR-001 | Persist `source_domain` at ingest, stamped provider-first; fail-loud, never silent claude-code default (AC-03, SR-07/SR-11) |
| ADR-002 | `model_id` carrier: new wire field + new column, plumbed plugin→`ImplantEvent`→observations, queryable (AC-06, SR-02) |
| ADR-003 | **E2 sizing: AC-06 IN this cycle** (rides the same migration + plumbing as AC-03; ledger guardrail gives zero credit for deferral) |
| ADR-004 | Ingestion = TS plugin shim + `--provider opencode` arm in **both** hook.rs and normalize.js + parity corpus (three coupled touchpoints, #5737, col-022) |
| ADR-005 | Modularity: new-module-with-thin-wiring; no ad-hoc monolith split; scheduled decompositions for over-cap files C18 modifies (PL-10) |
| ADR-006 | Non-clobbering installer OpenCode branch; additive; retrieval regression sentinel (AC-05, SR-08) |
| ADR-007 | Subagent alignment via bus-derived `session.created`+`parentID`+validated `session.agent` on the observe channel (AC-04, SR-09) |
| ADR-008 | AC-07 forward-compat: keep `external_identity` seam + per-call MCP delivery-channel viable; no enforcement, no N=1 ceremonial test (SR-12) |
| ADR-009 | Degraded/experimental legs (PreCompact experimental API; `session.idle`→Stop over-count) absorbed into the parity-gap posture; fail-safe, documented, non-cap-failing (SR-03, SR-04) |

## Integration Surface

Exact names/types so downstream agents do not invent them. Line numbers are current-tree anchors.

| Integration Point | Type / Signature | Source | vnc-049 action |
|---|---|---|---|
| `HookInput.provider` | `Option<String>` `#[serde(default)]` | `unimatrix-engine/src/wire.rs:84` | reuse |
| `HookInput.model_id` | **new** `Option<String>` `#[serde(default)]` | `wire.rs` (HookInput) | add |
| `ImplantEvent.provider` | `Option<String>` `#[serde(default, skip_serializing_if="Option::is_none")]` | `wire.rs:276` | reuse |
| `ImplantEvent.model_id` | **new** `Option<String>` same attrs | `wire.rs` (ImplantEvent) | add |
| ts-rs bindings | `HookInput`/`ImplantEvent` derive `ts_rs::TS` under `cfg(test)` | `wire.rs:55,249` | regenerate + drift gate (vnc-024 #4726) |
| `KNOWN_PROVIDERS` | `const KNOWN_PROVIDERS: &[&str] = &["claude-code","gemini-cli","codex-cli"]` | `uds/hook.rs:158` | add `"opencode"` |
| `normalize_event_name` / `map_to_canonical` | event-name → (canonical, inferred_provider) | `uds/hook.rs:66-105` | add opencode arm (delegates to new `hook/opencode.rs`) |
| JS normalizer | exact port of hook.rs normalizer | `packages/unimatrix/lib/hook-client/normalize.js` | mirror opencode arm (col-022) |
| Parity corpus | oracle-generated goldens + drift check | `uds/parity_corpus_uds.rs` | add opencode cases (AC-02) |
| `observations` table | cols: `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal, phase, topic_source` — **no source_domain/provider/model** | `unimatrix-store/src/db.rs` (CREATE), `migration.rs` (ALTER) | add `source_domain TEXT`, `model_id TEXT` (nullable); bump to the next sequential schema version, resolved against `CURRENT_SCHEMA_VERSION` at delivery (32 as of migration.rs:26 today; version-independent — do NOT hardcode) |
| write path | `insert_observation(...)` `:3383`, `insert_observations_batch(...)` `:3416` | `uds/listener.rs` (production, pre-test-boundary 3455) | bind `source_domain`, `model_id` from `ImplantEvent` |
| read path | `parse_observation_rows(...)` | `services/observation.rs:576` | SELECT new cols; prefer stored `source_domain`; surface `model_id` |
| read path (store) | `fetch_observations_since` `:44`, `load_observations_for_sessions` `:124`, `load_observation_session_stats` `:173` | `unimatrix-store/src/observations.rs` (185 lines, under-cap) | SELECT new cols |
| `resolve_source_domain` | `fn resolve_source_domain(&self, event_type:&str)->String` | `unimatrix-observe/src/domain/mod.rs:180` | unchanged; used only for legacy NULL rows |
| `DEFAULT_HOOK_SOURCE_DOMAIN` | `pub(crate) const … = "claude-code"` | `services/observation.rs:572` | legacy-row fallback only |
| OpenCode domain pack | `DomainPack{ source_domain:"opencode", event_types, categories, rules }`; format `^[a-z0-9_-]{1,64}$` | `domain/mod.rs` | add builtin/config pack |
| `external_identity` seam | `build_context_with_external_identity(..., external_identity: Option<&ResolvedIdentity>)` — always `None` today | `mcp/server.rs` (#4357) | **untouched**, kept viable (AC-07) |
| Installer entry | `unimatrix init` writes `.mcp.json` + `.claude/settings.json`; **no OpenCode writer** | `packages/unimatrix/lib/init.js` | add OpenCode branch (new `opencode-install.js`) |
| Retrieval config | `opencode.json` `mcp.unimatrix { type:"local", command, env }` + Ollama `provider` | repo `opencode.json` | preserve byte-for-byte (SR-08) |
| Plugin API | `@opencode-ai/plugin@1.18.31`; `Plugin(input, opts) => Promise<Hooks>`; `PluginInput.$` (Bun shell), `.client`, `.directory`, `.worktree` | `.opencode/package.json` | consume in C1 |

### OpenCode → canonical event map (C1 shim)

| Canonical | OpenCode source | Kind | Reachable OOB | Notes |
|---|---|---|---|---|
| SessionStart | `session.created` (+`session.updated`) | bus | yes (degraded) | derive `cwd` from `PluginInput.directory`; no `transcript_path` |
| UserPromptSubmit | `chat.message` | typed | yes | `prompt` from `message`/`parts`; carries `model` |
| PreToolUse | `tool.execute.before` | typed | yes | `tool_input` from output `args`; `sessionID→agent` correlation |
| PostToolUse | `tool.execute.after` | typed | yes | synthesize failure semantics from `output`/`metadata` |
| SubagentStart | `session.created` w/ `parentID`+`agent` | bus (derived) | observe-only | injection **unreachable** (parity gap); observation in scope (AC-04) |
| PreCompact | `experimental.session.compacting` | typed, experimental | yes (unstable) | ADR-009 degraded/flagged leg |
| Stop | `session.idle` | bus (derived) | yes (degraded) | `duration`/`outcome` computed plugin-side; over-count guard (ADR-009) |

## Measured parity gap (C18 `done_when`, not defects)

- **SubagentStart retrieval-injection** — unreachable (observe-only bus event). Observation is in scope.
- **No stdin command contract** — structural; every event passes through C1.
- **Stop / SessionStart** — bus-derived, degraded (computed `duration`/`outcome`, no transcript file).
- **PreCompact** — reachable but on an experimental API (ADR-009).

## Modularity assessment (PL-10 / #693, 500 code-line cap, tests excluded)

Re-measured code-lines (tests + block comments excluded), correcting the raw counts in SCOPE:

| File | Raw | **Code-lines** | Over cap? | C18 edits? |
|---|---|---|---|---|
| `uds/hook.rs` | 4403 | **804** | yes | yes (provider arm) |
| `uds/listener.rs` | 10131 | **2504** | yes | yes (write path insert) |
| `services/observation.rs` | 1976 | **975** | yes | yes (read path) |
| `unimatrix-store/src/db.rs` | — | ~**1250** | yes | yes (CREATE +2 cols) |
| `unimatrix-store/src/migration.rs` | — | ~**1500** | yes | yes (ALTER, additive) |
| `background.rs` (#966) | 5075 | **1389** | yes | **NO** — its `observations` INSERT (line 2960) is inside the test module (starts 1943); production write path is `listener.rs` |
| `unimatrix-engine/src/wire.rs` | 2605 | **237** | no | yes (add `model_id`) |
| `unimatrix-observe/src/domain/mod.rs` | 212 | **106** | no | yes (opencode pack) |
| `unimatrix-store/src/observations.rs` | — | **185** | no | yes (read SELECT) |

**Decision (ADR-005):** background.rs is **not** carved — C18 does not touch its production code (the
SCOPE #966/#965 carve candidates rested on the raw-line misread; re-measured, background.rs is
untouched). listener.rs (#965) **is** edited, so its rule applies — but no ad-hoc mid-delivery split
of a 2504-line monolith (untested surgery is itself a risk, and SCOPE forbids it). Instead:
**new-module-with-thin-wiring** for all net-new logic (C2 opencode canonicalization → new
`uds/hook/opencode.rs`; C9 installer → new `opencode-install.js`; C1 plugin is a net-new module under
cap), and the over-cap files (`hook.rs`, `listener.rs`, `observation.rs`, `db.rs`, `migration.rs`)
receive only minimal wiring (a const entry, an INSERT/SELECT column pair, a bind). Per PL-10 done_when
(2), the delivery-phase check records the touched over-cap files and files/updates a **scheduled
decomposition issue** with a test plan for each — deferred, not executed this cycle. New modules ship
at/under cap (done_when 3).

## Dependencies & constraints honored

- **C17 (#5582)**: installer branch extends C17; prerequisite for AC-05.
- **C10 (#5547)**: retrieval preserved as regression sentinel.
- **col-022 split-brain**: opencode arm lands in hook.rs + normalize.js together, guarded by parity corpus.
- **Lesson #5670**: hook-frame changes (`model_id`) must land at Rust hook.rs (extract+insert), JS
  hook-client port, a parity-corpus case, and one client→listener→DB crossing test.
- **C18 ledger guardrail**: C18 stays `partial` until AC-06 local-vs-cloud distinctness is
  behaviorally demonstrated on the assembled path — regardless of E2 timing.

## Open Questions (routed to risk-strategy / tester / delivery PoC)

1. **`session.idle`→Stop semantics** (SR-04): fires once per Stop or on every idle transition? Needs a
   delivery-time PoC; ADR-009 mandates an over-count guard either way.
2. **Existing-provider source_domain impact**: persisting `source_domain` from provider at write means
   new gemini-cli/codex-cli rows would also carry their provider as `source_domain` (a correctness
   improvement, but a behavior change vs today's read-derived value). Tester must confirm whether to
   scope the write-time stamp to `opencode` only or accept the generalization; existing source_domain
   tests (T-SEC-12/13, observation.rs) may need updating.
3. **Subagent MCP connection** (SR/ass-106): do OpenCode child sessions share the parent's MCP client
   or get their own? Gates AC-07 delivery-channel (i) vs proxy (ii). Not this cycle's delivery.
4. **PreCompact experimental stability** (SR-03): gate behind a plugin flag now, or defer the leg?
   ADR-009 flags it degraded; delivery decides the flag default.
5. **Schema version (resolved)**: the next sequential schema version, resolved against `CURRENT_SCHEMA_VERSION` at delivery — 32 as of migration.rs:26 today; do NOT hardcode (a concurrent PR may advance it before vnc-049 lands).
