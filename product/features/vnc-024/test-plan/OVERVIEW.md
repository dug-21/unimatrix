# vnc-024 Test Plan — OVERVIEW

> F1 wire/server foundation (#672). Dominant risk class: **silent infidelity** — code that compiles
> and looks right but models the wrong thing (a mis-serialized ts-rs type, a delta silently written
> to disk, a retention enum that accepts what OSS cannot safely honor). F1 freezes a contract F2–F5 /
> #670 inherit; an undetected defect here is a permanent wrong foundation, not a recoverable bug.
> The test strategy is therefore **evidence over trust**: round-trip fixtures (not generated `.ts`)
> are the contract authority, and the zero-durable-rows guard test is a GATE PREREQUISITE.

## Test Strategy Layers

| Layer | Runner | What it proves | Components |
|-------|--------|----------------|------------|
| Rust unit / `#[cfg(test)]` | `cargo test -p unimatrix-engine`, `-p unimatrix-server` | serde emit/parse behavior, config load/validate/merge, guard structure | all |
| Node round-trip harness | `node --test crates/unimatrix-engine/bindings/contract.test.mjs` | TS bindings agree with Rust on serde **behavior** (tag, flatten, None-omission, dual-sided delta) | contract-fixtures |
| Shell / build gate | `cargo metadata`, `cargo tree --edges normal`, `cargo audit`, `git diff --exit-code` | dev-only footprint, CI drift-gate functions | ts-rs-codegen |
| HTTP integration | `unimatrix-server` over HTTP `POST /observe` (tower handler) | content negotiation, byte-identity, **AC-12 HTTP transport** | observe-content-negotiation, transcript-delta-guard |
| UDS integration | `unimatrix-server` UDS dispatch | guard on UDS transport, batch arm, UDS parity (AC-10) | transcript-delta-guard, observe-content-negotiation |

## GATE PREREQUISITE — AC-12 / R-03 (read first)

**The zero-durable-rows negative test must be green before any downstream AC is trusted** (SR-07 /
#4711 / #4311). It is a single test obligation discharged across **three arms**, all required:

1. **HTTP transport** — `POST /observe` with a `transcript_delta` `RecordEvent` (reaches dispatch via `router.rs:234`) → `Ack` + zero observation rows.
2. **UDS transport** — direct UDS dispatch of the same delta → `Ack` + zero rows.
3. **`RecordEvents` batch arm** — a batch with one delta + N normal events → the delta persists **nothing**, the N others persist normally.

A single-transport pass does NOT satisfy this. See `transcript-delta-guard.md` for the full assertion set, including the structure/anti-pattern checks (the guard early-`return Ack`s before `:793`/`:849`; it must NOT reuse the col-022 specialize-then-fall-through pattern #1266).

## Risk → AC → Test Mapping

| Risk | Priority | AC(s) | Test layer | Component plan |
|------|----------|-------|-----------|----------------|
| R-01 ts-rs mis-models serde (tag/flatten) | Critical | AC-04, AC-05, AC-11 | Node + Rust round-trip | contract-fixtures |
| R-02 None-vs-omission one-directional/trivial | Critical | AC-06 | Node + Rust round-trip (dual-direction, non-trivial) | contract-fixtures |
| R-03 secrets-to-disk hole | **Critical / GATE** | **AC-12** | HTTP + UDS integration | transcript-delta-guard |
| R-04 guard one-transport / batch missed | Critical | AC-12 | HTTP + UDS + batch | transcript-delta-guard |
| R-05 format_injection byte-identity/budget drift | High | AC-07 | HTTP integration (incl. truncation) | observe-content-negotiation |
| R-06 non-injection response text-formatted | High | AC-09 | HTTP integration | observe-content-negotiation |
| R-07 Accept read after into_parts | High | AC-07, AC-08 | HTTP integration (content-type at boundary) | observe-content-negotiation |
| R-08 frozen contract omits F2/#670 field | High | AC-11, AC-13 | binding cross-check + Node | contract-fixtures, transcript-retention |
| R-09 retention touchpoint missed / weak reject | High | AC-13, AC-14 | Rust config unit | transcript-retention |
| R-10 retention TOML repr (mostly dissolved) | Low | AC-13 | Rust config unit | transcript-retention |
| R-11 PartialEq on TranscriptRetention | Medium | AC-13, AC-14 | Rust config unit (compile + merge) | transcript-retention |
| R-12 ts-rs runtime leak | Medium | AC-15 | shell (cargo tree/metadata/audit) | ts-rs-codegen |
| R-13 two carriers drift (precedence note) | Low | AC-11 | reviewer/doc check | contract-fixtures |
| R-14 CI diff-gate non-functional | High | AC-03 | shell (mutate→fail, restore→pass) | ts-rs-codegen |
| (parity) UDS output unchanged | — | AC-10 | UDS golden compare | observe-content-negotiation |

Every Critical and High risk has at least one concrete, per-component assertion. AC-01/AC-02
(file/shell existence) live in `ts-rs-codegen.md`.

## Cross-Component Test Dependencies

- **`TranscriptDeltaPayload` is shared** between contract-fixtures (the dual-sided fixture, AC-11)
  and transcript-delta-guard (the guard parses into the same struct, AC-12). The two plans must
  assert the *same* `{offset:u64, bytes:String}` shape — a divergence is itself a defect.
- **`format_injection` budget** couples observe-content-negotiation (the `/observe` text path) to the
  production UDS caller's `max_bytes` constant (OQ-1). The byte-identity test (AC-07) must call the
  real `hook.rs:1047` fn with the production budget, including a truncation case.
- **CI gate (R-14) protects every codegen AC** — if the diff-gate is non-functional, R-01/R-02/R-08's
  protection evaporates. ts-rs-codegen's gate self-test is a meta-prerequisite for the contract-fixtures
  plan's trust.
- **Config merge re-validation (#3905)** couples transcript-retention's merge test (AC-14) to its
  validate test (AC-13): a merged `RetainDays` must still be rejected.

## Integration Harness Plan (infra-001)

### Critical scoping note — infra-001 does NOT cover vnc-024's integration ACs

The infra-001 harness drives `unimatrix-server` over **MCP JSON-RPC on stdio**. vnc-024's
integration-level behavior lives on **two different transports the stdio harness does not exercise**:

- **HTTP `POST /observe`** — a custom tower handler (vnc-022 #669), not the MCP stdio surface. AC-07/AC-08/AC-09 (content negotiation) and the HTTP arm of AC-12 run here.
- **UDS hook dispatch** (`listener.rs` `RecordEvent`/`RecordEvents`) — the `transcript_delta` guard (AC-12 UDS + batch arms) and AC-10 UDS parity run here.

Therefore **no existing infra-001 suite validates AC-07/08/09/10/12**, and **no new infra-001 suite
should be added** for them — they are out-of-band (HTTP tower + UDS) relative to the stdio harness.
These ACs are covered by **server-crate integration tests** (Rust `#[tokio::test]` against the HTTP
handler and UDS dispatch), specified in the `observe-content-negotiation.md` and
`transcript-delta-guard.md` component plans. Attempting to force them through the MCP stdio harness
would test the wrong interface.

### infra-001 suites to run in Stage 3c

| Suite | Why | Blocking? |
|-------|-----|-----------|
| `smoke` (`-m smoke`) | Mandatory minimum gate — proves the server still builds, handshakes, stores, searches, and restarts after the wire.rs / config.rs / listener.rs / router.rs edits. | **Yes — gate** |
| `protocol` | wire.rs derives (`TS` + serde) touch the shared envelope types; confirm MCP handshake/JSON-RPC compliance and tool discovery are unaffected by the codegen derives. | Recommended |
| `tools` | config.rs and listener.rs edits sit on the tool/dispatch path; confirm all tools still parse params and return correct response formats. | Recommended |
| `lifecycle` | listener.rs (dispatch) + config.rs (RetentionConfig) are storage/lifecycle-adjacent; confirm store→search and restart-persistence are unregressed (no delta accidentally persisted, no config-load regression). | Recommended |
| `adaptation` | covers format negotiation at the MCP layer; sanity-check the response-format machinery the `/observe` mapper change is adjacent to. | Optional |

**Expectation:** all selected infra-001 suites are **regression baselines** — vnc-024 is additive, so
they must remain green. Any new failure is triaged per the USAGE-PROTOCOL decision tree: feature-caused
→ fix; pre-existing → GH Issue + `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — …")`; bad assertion
→ fix the test. **Never fix an unrelated infra-001 failure in this PR.**

### New integration tests required (server-crate, NOT infra-001)

These are net-new Rust integration tests in `unimatrix-server` (the brief's `Files to Modify` already
anticipate extending the relevant test modules). They are detailed in the component plans:

1. **AC-12 zero-durable-rows (GATE)** — HTTP + UDS + batch arms. `transcript-delta-guard.md`.
2. **AC-07 byte-identity** — `/observe` `Accept: text/plain` Entries body == `format_injection(...)` incl. truncation. `observe-content-negotiation.md`.
3. **AC-08/AC-09 allowlist + JSON parity** — Entries/BriefingContent honor text; Pong/Ack/Error stay JSON; content-type asserted at the HTTP boundary. `observe-content-negotiation.md`.
4. **AC-10 UDS parity** — golden compare of UDS hook output before/after. `observe-content-negotiation.md`.

### New non-infra integration tests required (Node + shell)

5. **AC-05/AC-06/AC-11 Node round-trip** — `contract.test.mjs` via `node --test`. `contract-fixtures.md`.
6. **AC-03 CI drift-gate self-test** — mutate→non-zero, restore→zero. `ts-rs-codegen.md`.
7. **AC-15 dev-only footprint** — `cargo tree --edges normal`, `cargo metadata`, `cargo audit`. `ts-rs-codegen.md`.

## Test Conventions

- Rust: `#[tokio::test]` for async HTTP/UDS paths; extend the existing `wire.rs:379+` and config/listener
  `#[cfg(test)]` suites — **do not scaffold new infra** (Constraint 8). Test naming
  `test_{unit}_{scenario}_{expected}`.
- Node: `contract.test.mjs`, ~dozen lines, no TS client package; `node --test`; assert **behavior**
  (narrows to the right union member / key absent / parses into the typed struct), not merely "parses".
- Fixtures: committed under `crates/unimatrix-engine/bindings/fixtures/*.json`, Rust-emitted, frozen
  against the ass-069 Q2/Q7 field list before merge.
- Determinism: no time/random in fixtures; row-count assertions query the DB directly, not via search.

## Open Questions (for delivery)

- **OQ-1 (R-05):** the exact `max_bytes` constant the production UDS caller passes to `format_injection`
  — the AC-07 test must use this same constant. Delivery confirms; if it is not a single shared symbol,
  the byte-identity test should reference one source to avoid silent future drift.
- **OQ-2:** does the server-crate integration test suite already have a fixture/helper for booting the
  HTTP `/observe` handler and for counting observation rows (`insert_observation` target table)? If not,
  one focused helper is needed (reused by AC-07/08/09 and AC-12) — extend, don't duplicate.
- **OQ-3:** confirm the `wants_text` predicate semantics for `Accept: text/plain, application/json` and
  `Accept: */*` (contains `text/plain` ⇒ text). Edge-case tests assume "contains text/plain".
