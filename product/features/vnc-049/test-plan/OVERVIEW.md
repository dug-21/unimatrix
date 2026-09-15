# vnc-049 Test Strategy — OVERVIEW

OpenCode as the fourth observation harness (C18). Test plans root every scenario in
`RISK-TEST-STRATEGY.md` (R-01..R-17) and discharge every AC in `ACCEPTANCE-MAP.md`
(AC-01..07) on the assembled path — never on a proxy.

Grounding: ARCHITECTURE.md (ADR-001..009), SPECIFICATION.md (FR-01..12, NFR-01..06),
knowledge #5285 (derive-don't-seed / anti-seed traps), #5427 (source-assertion string-counting
is blind — pair with a behavioral per-site matrix), #4373 (schema-version cascade checklist),
#4153/#378 (migration must exercise old-schema DBs), #4177/#3548 (tautology / omitted-assertion
gate history), #5737 (three-touchpoint harness onboarding).

## 1. Overall Test Strategy

Three tiers, each mapped to a component band from the Component Map (C1..C9):

| Tier | What it proves | Where | Components |
|------|----------------|-------|-----------|
| **Unit** | Pure logic: event-name canonicalization, provider stamping, model_id/source_domain validation (`^[a-z0-9_-]{1,64}$`), read-path fork (stored vs legacy-NULL), installer merge helpers | Rust `#[cfg(test)]` modules; JS unit (node/jest) for normalize.js + installer | C2, C3, C4, C6, C7, C9 |
| **Cross-language parity** | Rust `hook.rs` ↔ JS `normalize.js` canonicalize opencode identically; drift-fails if one arm edited alone | `parity_corpus_cases*.rs` + `parity_corpus_gen.rs` (`assert_coverage`/`all_arm_keys()`) + JS fixtures under `packages/unimatrix/test/fixtures/parity/`, gated by `scripts/check-parity-drift.sh` | C2, C3, C8 (coupled) |
| **Assembled-path / behavioral (authoritative)** | The stored/queried record carries correct attribution driven from the real entry point (plugin/CLI/installer), NOT a seam | Rust crossing test in `listener.rs mod tests` (3456+) following the `listener/tests/foreign_domain.rs` dispatch→store→read-back pattern + infra-001 `UnimatrixHookClient` UDS-observe integration | C1→C4→C5→C6 chain; C9 installer |

**Design rule enforced throughout (anti-proxy, #907/#918/#930):** a test that backs an AC's
`done_when` MUST drive the real assembled wiring — the actual `unimatrix hook` entry point, the
real INSERT, the real transport — then assert on the **queried stored record**. A test that
hand-constructs the `ImplantEvent`, seeds the DB row via SQL, or asserts `field.is_some()` /
"the `--model` flag was passed" is a proxy and does NOT discharge AC-03 or AC-06 (see #5285: do
not seed the attribution join — DERIVE it over the wire).

## 2. Risk → Test Mapping

| Risk | Priority | Component(s) | Discharging scenario(s) | Proof surface |
|------|----------|--------------|--------------------------|---------------|
| **R-01** AC-06 proven-but-holed (GATING) | Critical | C1,C4,C5,C6 | Local-model event through plugin→hook→UDS→INSERT→store→**query**, distinct from cloud-model on the queried row; negative (drop/homogenize/NULL fails); two local models mutually distinct | Assembled-path crossing test + infra-001 UDS-observe |
| **R-02** silent source_domain=claude-code | Critical | C5,C6 | Stored-row `source_domain="opencode"` + **mandatory negative** NOT `"claude-code"`; derivation site PINNED (OQ-4); fail-loud on unresolvable | Assembled-path stored-row SELECT |
| **R-03** split-brain drift (col-022) | Critical | C2,C3,C8 | Parity corpus green across 7 events; drift-fails if one arm edited alone (`assert_coverage` "arm keys without a corpus case"); `"opencode"∈KNOWN_PROVIDERS`; stored `provider="opencode"` | `parity_corpus_cases*.rs`/`_gen.rs` + crossing test |
| **R-04** migration cascade incomplete | Critical | C5,C6 | Fresh-create ≡ ALTER-migrate column parity; legacy-NULL read fallback (both fork branches); 3-path bump + idempotency/back-fill per #4373 | migration tests + read-path tests |
| **R-05** existing-provider regression (RESOLVED opencode-only) | Low | C5 | Stamp fires ONLY for `provider=="opencode"`; non-opencode rows stay NULL; **T-SEC-12/13 green unchanged** | write-path per-provider matrix |
| **R-06** subagent orphaning (AC-04) | High | C1,C5 | Stored subagent record aligned to owning feature/cycle; ordering race (child-before-parent) still correlates | Assembled-path subagent test |
| **R-07** session.agent spoofable channel | High | C1 | Validated `session.agent` rides `extra.agent_type`; conflicting tool-args value does NOT override | Provenance + spoof-rejection test |
| **R-08** C10 retrieval regression on install | High | C9 | Installer on fixture repo: `mcp.unimatrix`+STDIO+Ollama block byte-for-byte; retrieval still returns; plugin provisioned | Installer integration test |
| **R-09** installer non-idempotence | Med | C9 | Re-run: no duplicate plugin/dep entries, no key drift | Installer idempotence test |
| **R-10** model_id carrier blast radius | Med | C4 | ts-rs drift gate green; serde back-compat (frame w/o model_id deserializes); one client→listener→DB crossing test (#5670) | wire unit + crossing test |
| **R-11** session.idle→Stop over-count | Med | C1 | Repeated idle → exactly one Stop; Stop/SessionStart recorded degraded/bus-derived | Plugin unit + shim behavior |
| **R-12** PreCompact experimental API | Med | C1 | Present→lands; absent/changed→fail-safe, no crash, documented parity gap | Plugin unit (present/absent) |
| **R-13** plugin transport fail-open | Med | C1 | Binary absent / socket down → session survives, no block/crash | Plugin fail-open test |
| **R-14** bus-derived field synthesis | Med | C1 | cwd/worktree from PluginInput correct; absent transcript_path handled; malformed payload degrades honestly | Plugin unit |
| **R-15** untrusted input | Med | C1,C4,C5,C9 | model_id/source_domain validated `^[a-z0-9_-]{1,64}$`; reject/sanitize, never raw to SQL/path; installer follows no injected path | Validation unit + boundary |
| **R-16** AC-01 false-pass on parity gap | Med | C1,C8 | Each reachable/bus-derived event → stored queryable record; SubagentStart injection recorded as measured parity gap, no false-pass, no silent drop | 7-event assembled sweep |
| **R-17** over-cap wiring surgery | Low | C2,C5,C6 | Adjacent existing tests (listener/hook/observation/db/migration) stay green; no ad-hoc carve | Full `cargo test --workspace` |

## 3. Cross-Component Test Dependencies

- **C1+C3+C8 are coupled (col-022).** The plugin (C1), the JS normalizer arm (C3), and the
  parity corpus (C8) change together. The parity corpus is the drift sentinel — its test plan
  (c8) owns the assertion that editing one arm alone fails the suite (#5302: single-source the
  full CONTRACT, not just shared data).
- **C4→C5→C6 is the attribution spine.** `model_id`/`provider` added at wire (C4) must be bound
  at INSERT (C5) and surfaced at read (C6). R-01/R-10 crossing tests exercise all three; a wire
  or read test alone does NOT discharge them (Wire→INSERT drop is the named integration risk).
- **OQ-4 site pinning is shared by R-02 and R-05.** One derivation site produces the stored
  `source_domain`; both the AC-03 positive/negative test (C5/C6) and the opencode-only R-05
  matrix must key on that pinned site so a future edit to the *other* site cannot silently
  reintroduce the claude-code default. Per #5427, a per-site behavioral matrix — not a
  call-count/string-match — is required.
- **AC-06 gating cascade.** C18 stays `partial` until the R-01 end-path distinctness test passes
  on the queried record; every other AC passing does not lift the gate.

## 4. Integration Harness Plan (infra-001)

### 4.1 What the harness can and cannot reach for this feature

infra-001 exercises the compiled binary. Two entry points matter here:
- **MCP `context_*` JSON-RPC** — used for the AC-05 retrieval-regression assertion (C10 preserved).
- **UDS observe path (`harness/hook_client.py` `UnimatrixHookClient`)** — the harness CAN drive
  real observation ingestion over UDS via the wire enum (`session_register`/`record_event`/
  `session_close`, incl. `record_post_tool_use`) and then query stored rows back. The proven model
  is `test_lifecycle.py::test_observe_row_count_delta_persist_vs_noop_close` (:4823, GH#819): the
  live `daemon_server` fixture (real UDS + hook sockets) drives register→record→close, then reads
  back with `_observation_row_count` (:4782) via a fresh `sqlite3 SELECT ... FROM observations` on
  the per-slug store, settle-polling fire-and-forget writes (`_wait_for_row_count`, :4801). This is
  the vehicle for the AC-03/AC-06 assembled-path stored-record assertions through the compiled
  binary. New AC-03/AC-06 tests extend this pattern (add SELECT of `source_domain`/`model_id`).

**Anti-seed contract (#5285, load-bearing):** the AC-03/AC-06 harness tests MUST drive the event
over the wire and DERIVE attribution — they must NOT seed the observation row via the direct-SQLite
INSERT pattern used elsewhere in `test_lifecycle.py` (e.g. :1274, :2072, :3633 topic_source
readback), nor inject the struct field the way Rust `make_stamped_event(topic_signal=...)` does.
Seeding produces a believable-but-fake green. The queried record must carry attribution that flowed
plugin/CLI→wire→INSERT. Note `test_parity_legs.py` uses a `FakeHookClient` monkeypatch — that is NOT
a real-ingest vehicle and must not be used to discharge AC-03/AC-06.

### 4.2 Suites that apply (per suite-selection table)

| Feature touches | Suites to run in Stage 3c |
|-----------------|---------------------------|
| Server tool logic / observation write path | `tools`, `protocol`, `lifecycle` |
| Store/schema changes (new columns, migration) | `lifecycle` (restart persistence), `volume`, `edge_cases` |
| Security (untrusted model_id/source_domain, installer input) | `security` |
| Any change | `smoke` (MANDATORY minimum gate) |

Full-suite pre-merge run required (server code touched). Availability suite is pre-release only,
not this cycle's gate.

### 4.3 New integration tests to add (Stage 3c implements)

Added to infra-001 suites, following `test_{concept}_{behavior}` naming and cumulative-fixture
rule (extend existing fixtures/helpers, never fork scaffolding):

1. **`test_opencode_event_stored_source_domain_opencode`** (`suites/test_security.py` or a new
   attribution group in `test_lifecycle.py`) — drive an opencode UDS-observe event; query the
   stored record; assert `source_domain=="opencode"` AND NOT `"claude-code"` (AC-03, R-02).
   Derive, do not seed.
2. **`test_opencode_local_vs_cloud_model_distinct`** (`test_lifecycle.py`, extend the GH#819
   `daemon_server` pattern) — drive a local-model opencode event (`ollama`/`qwen3-coder`) and a
   cloud-model opencode event via `UnimatrixHookClient`; query both back (SELECT `model_id`);
   assert they are distinguishable on the stored `model_id`. Negative: assert distinctness fails
   if model_id is dropped/homogenized/NULLed. (AC-06 GATING, R-01.) The authoritative behavioral
   proof also lives as a Rust crossing test (c5, following `foreign_domain.rs`); this harness test
   proves it through the compiled binary end-path.
3. **`test_opencode_provider_stored`** (`test_lifecycle.py`) — stored record shows
   `provider=="opencode"` (AC-02c, R-03.4).
4. **`test_opencode_subagent_aligned_to_cycle`** (`test_lifecycle.py`) — child `session.created`
   +`parentID`+validated agent → stored subagent record aligned to owning feature/cycle (AC-04,
   R-06). Uses `admin_server`/`shared_server` as correlation needs accumulated parent state.
5. **`test_opencode_legacy_null_source_domain_fallback`** (`test_lifecycle.py` restart/persistence
   or `edge_cases`) — a pre-migration/NULL-source_domain row reads back via registry/DEFAULT
   fallback unchanged (AC-03 legacy branch, R-04.2).
6. **`test_source_domain_stamp_opencode_only`** (`test_security.py`) — a non-opencode event
   (claude-code/gemini-cli) leaves `source_domain` NULL at write and resolves via read-derived
   fallback; T-SEC-12/13 unchanged (R-05).
7. **`test_model_id_validation_boundary`** (`test_security.py`) — model_id violating
   `^[a-z0-9_-]{1,64}$` (too long, bad chars, SQL-ish) is rejected/sanitized, never persisted raw
   (R-15, edge cases).
8. **Installer suite** — installer non-clobber/idempotence/retrieval tests are JS-side (see c9);
   the infra-001 side is the post-provision retrieval-regression assertion via MCP `context_*`
   (AC-05c).

### 4.4 Existing suites that already cover feature-adjacent behavior

- `protocol`/`tools` — MCP handshake + `context_*` unchanged; regression sentinel that the schema
  migration and new columns did not break tool responses.
- `lifecycle` restart-persistence — validates the new columns survive a restart (R-04 persistence).
- `security` T-SEC-12/13 — existing source_domain tests; MUST re-run **green, unchanged** (R-05).

**When NOT to add integration tests:** pure event-name canonicalization parity is owned by the
Rust/JS parity corpus (C8) and unit tests — not re-proven through MCP. ts-rs binding drift is a
build-gate (C4), not an infra-001 suite. Significant harness infra changes → GH Issue, not this PR.

### 4.5 Smoke gate

`python -m pytest suites/ -v -m smoke --timeout=60` is the MANDATORY minimum gate before Gate 3c.
Full-workspace unit run via the hardened convention; full-workspace LINK smoke (#878 guard);
integration smoke — all three are Stage 3c entry gates.

## 5. Per-Component Test Plan Index

| File | Component | Primary risks | Authoritative assertions |
|------|-----------|---------------|--------------------------|
| c1-plugin-shim.md | C1 OpenCode plugin shim (TS) | R-11,R-12,R-13,R-14,R-16,R-07,R-06,R-15 | 7-event mapping; fail-open; bus-derived honesty; observe-channel provenance |
| c2-provider-arm-rust.md | C2 Rust normalizer arm | R-03,R-17 | `opencode∈KNOWN_PROVIDERS`; canonicalization; delegates to hook/opencode.rs |
| c3-provider-arm-js.md | C3 JS normalizer mirror | R-03 | byte-parity mirror of C2; drift sentinel |
| c4-wire-carriers.md | C4 model_id wire field | R-10 | ts-rs drift gate; serde back-compat; carrier not dropped |
| c5-ingest-persistence.md | C5 schema + write path | R-01,R-02,R-04,R-05,R-17 | **AC-03 + AC-06 assembled-path stored-record**; migration cascade; opencode-only stamp; OQ-4 site pin |
| c6-read-path.md | C6 read-path attribution | R-02,R-04 | prefer-stored vs legacy-NULL fork (both branches); surface model_id |
| c7-domain-pack.md | C7 OpenCode domain pack | R-04 (legacy) | legacy-NULL resolution; format contract |
| c8-parity-corpus.md | C8 parity corpus | R-03,R-16 | 7-event opencode cases; drift-fails one-arm edit |
| c9-installer.md | C9 installer branch | R-08,R-09,R-15 | byte-for-byte preservation; retrieval still returns; idempotence |

## 6. Open Questions

- **OQ-4 (derivation-site pin) — tester-blocking for authoring c5/c6.** WHICH site stamps the
  stored `source_domain` (listener Site A `provider.unwrap_or` per #4306 vs `DomainPackRegistry`
  DEFAULT on the hook path) is resolved by delivery/architecture; the test must key its per-site
  behavioral matrix (#5427) on the pinned site. Test plan assumes the ADR-001 provider-first
  ingest stamp at the listener insert site; confirm at Stage 3b.
- **OQ-3/R-11 (session.idle→Stop semantics)** — needs the delivery-time PoC to know whether the
  over-count guard asserts "exactly one Stop per session" or a debounce window. c1 test plan
  specifies the guard behavior abstractly; concrete assertion pinned when the PoC lands.
- **AC-06 harness reachability** — the authoritative R-01 proof is the Rust crossing test (always
  reachable). Whether the infra-001 UDS-observe path can also drive the *plugin* (TS) end (vs only
  the `unimatrix hook` CLI end) determines if test #2 above covers plugin→CLI or CLI→store only;
  if the plugin layer is not reachable from infra-001, its coverage is a node-side plugin test
  (c1) plus the Rust crossing test (c5) — the CLI→store segment is the harness's contribution.
- **Schema version** — `CURRENT_SCHEMA_VERSION` reads **31** in the current tree
  (`migration.rs:26`), so the next sequential is **32**. The IMPLEMENTATION-BRIEF/ARCHITECTURE text
  "32 as of migration.rs:26 today" is off by one against the tree — resolve against the live
  constant at delivery, do NOT hardcode (a concurrent PR may advance it). Migration test assertions
  use `>=` predicates per #4373; clone `migration_v30_to_v31.rs` as the v31→v32 template.
- **Parity-corpus target file (correction for delivery).** The BRIEF/ARCHITECTURE name
  `uds/parity_corpus_uds.rs` for C8, but that file is UDS **framing/byte** goldens only. The 7-event
  provider/normalizer cases live in `uds/parity_corpus_cases*.rs` + `uds/parity_corpus_gen.rs`
  (arm-key coverage drift-gated by `assert_coverage` @gen.rs:363 / `all_arm_keys()` @gen.rs:92).
  c8 is written against that empirically-correct seam; delivery/pseudocode must confirm which file
  receives the opencode cases (likely `parity_corpus_cases.rs` + new arm keys).
- **Read-path coverage gap.** `load_observation_session_stats` (`unimatrix-store/src/observations.rs:173`)
  has no direct test today; since C6 adds a SELECT of the new columns there, c6 requires new direct
  coverage for it (see c6).
