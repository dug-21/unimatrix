# vnc-049 Pseudocode — OVERVIEW

OpenCode as the fourth observation harness (C18). This file is the map: which components exist,
what data crosses boundaries, the shared types introduced/modified, and the **implementation
dependency ordering** (waves) the Delivery Leader plans from.

Source of truth: ARCHITECTURE.md §Component Breakdown / §Integration Surface, ADR-001..009,
SPECIFICATION.md FR-01..12 / AC-01..07, RISK-TEST-STRATEGY.md R-01..17. Interface names below are
traced to those documents and to current-tree signatures — none are invented.

## Components

| # | File | Surface | Location |
|---|------|---------|----------|
| C1 | c1-plugin-shim.md | TS (net-new) | `packages/unimatrix/opencode-plugin/` |
| C2 | c2-provider-arm-rust.md | Rust | `uds/hook.rs` + new `uds/hook/opencode.rs` |
| C3 | c3-provider-arm-js.md | JS | `packages/unimatrix/lib/hook-client/normalize.js` |
| C4 | c4-wire-carriers.md | Rust | `unimatrix-engine/src/wire.rs` |
| C5 | c5-ingest-persistence.md | Rust + SQL | `unimatrix-store/src/{db.rs,migration.rs}`, `uds/listener.rs` |
| C6 | c6-read-path.md | Rust | `services/observation.rs`, `unimatrix-store/src/observations.rs` |
| C7 | c7-domain-pack.md | Rust | `unimatrix-observe/src/domain/mod.rs` |
| C8 | c8-parity-corpus.md | Rust | `uds/parity_corpus_uds.rs` |
| C9 | c9-installer.md | JS (+ new module) | `packages/unimatrix/lib/{init.js,opencode-install.js}` |

## Data flow (the assembled write→read path AC-03/AC-06 are asserted from)

```
OpenCode runtime (Bun)
  └─ C1 plugin: typed hook / bus event fires
       ├─ builds Claude-shaped HookInput fields
       ├─ resolves model:{providerID,modelID} → "ollama/qwen3-coder"
       ├─ resolves parentID → parent feature/cycle (subagent, AC-04)
       └─ $ unimatrix hook <EVENT> --provider opencode --model <providerID>/<modelID>
            └─ C2 hook.rs run(): map_to_canonical + KNOWN_PROVIDERS ("opencode" arm, delegates hook/opencode.rs)
                 → HookInput.provider="opencode", HookInput.model_id=Some("ollama/qwen3-coder")   [C4]
                 └─ ImplantEvent { provider:Some("opencode"), model_id:Some(...) }                 [C4]
                      └─ C5 listener.rs: ObservationRow gains source_domain/model_id, derived at write:
                           source_domain = if provider=="opencode" {"opencode"} else {NULL}  (ADR-001, opencode-only)
                           model_id      = event.model_id (validated ^[a-z0-9_-/]... see C4/C1 note)
                         INSERT observations (... source_domain, model_id)                          [C5]
   Read: C6 parse_observation_rows / store SELECTs
        → prefer stored source_domain when non-NULL (="opencode"); NULL → legacy resolve (C7)
        → surface model_id → record queryable, local-model distinct from cloud + from claude-code
```

`--provider opencode` alone is necessary but **not sufficient** (ADR-004): it stamps only in-flight
`ImplantEvent.provider`. Correct stored attribution requires C5 persistence + C4 carrier.

## Shared types (introduced or modified)

- **`HookInput`** (`wire.rs:57`) — ADD `model_id: Option<String>` `#[serde(default)]`. Existing fields
  reused: `hook_event_name, session_id, cwd, transcript_path, prompt, provider, mcp_context, extra`.
- **`ImplantEvent`** (`wire.rs:251`) — ADD `model_id: Option<String>`
  `#[serde(default, skip_serializing_if="Option::is_none")]` (mirrors `provider` @276).
- **`ObservationRow` (write-path struct used by `listener.rs` inserts)** — ADD `source_domain:
  Option<String>`, `model_id: Option<String>`. NOTE (open question OQ-A): the read struct
  `ObservationRow` in `unimatrix-store/src/observations.rs:14` is a *different* struct (no
  `topic_signal/phase/topic_source`); the write-path `ObservationRow` bound at `listener.rs:3383/3416`
  is the one that gains the two new fields. Delivery must confirm which struct each site uses.
- **`observations` table** — ADD `source_domain TEXT`, `model_id TEXT` (both nullable). `db.rs` CREATE
  + `migration.rs` additive ALTER, schema version = next sequential (resolve vs `CURRENT_SCHEMA_VERSION`
  at delivery; 32 today — do NOT hardcode).
- **`DomainPack`** (`domain/mod.rs:28`) — ADD builtin/config opencode pack `{ source_domain:"opencode",
  event_types, categories, rules }`. Used only for legacy NULL-row read fallback (ADR-001), not the
  AC-03 mechanism.
- **`KNOWN_PROVIDERS`** (`hook.rs:158`) — ADD `"opencode"`.
- **Canonical event set** (unchanged): SessionStart, UserPromptSubmit, PreToolUse, PostToolUse,
  SubagentStart, PreCompact, Stop.

## OpenCode → canonical event map (owned by C1, mirrored in C2/C3 naming, covered by C8)

| Canonical | OpenCode source | Reachability |
|---|---|---|
| SessionStart | `session.created` (+`session.updated`) | bus-derived (degraded) |
| UserPromptSubmit | `chat.message` | reachable (carries model) |
| PreToolUse | `tool.execute.before` | reachable |
| PostToolUse | `tool.execute.after` | reachable |
| SubagentStart | child `session.created` + `parentID` + `session.agent` | observe-only (injection = parity gap) |
| PreCompact | `experimental.session.compacting` | reachable (experimental, ADR-009) |
| Stop | `session.idle` | bus-derived (degraded; over-count guard) |

## Derivation-site pin (resolves OQ-4 / R-02)

The stored `source_domain` for opencode rows is produced at the **write path (C5, `listener.rs` insert
site)** from `ImplantEvent.provider`, NOT at read time. `resolve_source_domain` (C7) is authoritative
ONLY for legacy NULL rows. A test (C6/C8) pins this so a future edit to the read site cannot
reintroduce the `claude-code` default for opencode rows.

## Implementation dependency ordering (waves)

Waves are ordered by data-contract dependency. **col-022 coupling constraint overrides parallelism:**
C2 + C3 + C8 (and C1's event map) are the split-brain triad and MUST land in one change/PR (ADR-004,
lesson #5670) — the waves below sequence *design/implementation start*, not separate merges for the
triad.

- **Wave 1 — Foundation (no intra-feature deps):**
  - **C4** wire carriers (`model_id` on HookInput + ImplantEvent, regenerate ts-rs bindings). Everything
    downstream carries this field; must exist first.
  - **C7** opencode domain pack (independent; legacy-row read fallback + category registration).
  - **C5a** schema only (`db.rs` CREATE + `migration.rs` ALTER + version bump) — column existence is a
    prerequisite for both write-bind and read-SELECT; can start alongside C4.

- **Wave 2 — Ingest + normalization (dep: Wave 1):**
  - **C5b** write path (`listener.rs` bind `source_domain`/`model_id` from ImplantEvent) — needs C4
    (ImplantEvent.model_id) + C5a (columns).
  - **C2** Rust provider arm + new `hook/opencode.rs` — needs C4 (populates model_id into HookInput→
    ImplantEvent); adds "opencode" to KNOWN_PROVIDERS.

- **Wave 3 — Read + edge + plugin (dep: Wave 2):**
  - **C6** read path (SELECT new cols, prefer-stored, legacy fallback) — needs C5a columns + C7 fallback.
  - **C3** JS normalizer mirror — pairs with C2 (col-022); lands in the same change as C2/C8.
  - **C1** plugin shim — needs C4's `--model` CLI contract + C2's opencode provider arm accepting the frame.

- **Wave 4 — Guards + provisioning (dep: Wave 3):**
  - **C8** parity corpus opencode cases — needs C2 + C3 both present (asserts Rust↔JS identical). Split-brain gate.
  - **C9** installer opencode branch — provisions the C1 plugin artifact; needs C1 to exist. Independent of Rust waves.

Critical path: **C4 → C5 → {C2, C1} → C8**. AC-06 gating end-path test (R-01) exercises C1→C4→C2→C5→C6.

## Cross-cutting risks each component must honor

R-01 (AC-06 end-path, gating) → C1+C4+C5+C6+C8. R-02 (silent claude-code) → C5+C6. R-03 (split-brain)
→ C2+C3+C8. R-04 (migration cascade) → C5. R-05 (opencode-only stamp) → C5. R-06/R-07 (subagent) → C1.
R-08/R-09 (installer) → C9. R-10 (model_id blast radius) → C4. R-11/R-12 (degraded legs) → C1.
R-13 (fail-open) → C1. R-15 (untrusted input validation) → C1+C4+C5.

## Open questions (flagged, not resolved here)

- **OQ-A (new):** two `ObservationRow` structs — confirm the write-path struct at `listener.rs:3383/3416`
  is the one gaining `source_domain`/`model_id`, and where it is constructed from `ImplantEvent`.
- **OQ-3/R-11:** `session.idle`→Stop once-per-Stop vs per-idle-transition — delivery-time PoC (C1).
- **OQ-7/R-12:** PreCompact experimental flag default (C1).
- **model_id charset:** value is `"<providerID>/<modelID>"` which contains `/`, so it does NOT match
  the `^[a-z0-9_-]{1,64}$` source_domain contract. C4/C1/C5 must define a separate `model_id` validation
  charset (allowing `/`) — flagged in C4.
</content>
</invoke>
