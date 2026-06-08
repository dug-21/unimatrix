## ADR-003: Wire Contract — Additive `ImplantEvent.cycle_stamp: Option<CycleStampPayload>`, 7th ts-rs Export, End-to-End Round-Trip AC

### Context

The F1 wire contract is frozen-additive: optionals carry `#[serde(default, skip_serializing_if)]`, no `deny_unknown_fields` anywhere, existing parity fixtures and ts-rs bindings must pass byte-unchanged. `ImplantEvent` (wire.rs:225) already carries optional `topic_signal`/`provider` this way. ass-072 rejected the two zero-wire-change alternatives: reusing `topic_signal` (server cannot tell contract from guess — the #588 failure class) and a payload key (payload is event-type-specific; a typed top-level field gives one read point and a contract fixture). Lesson #3486: a previous context_cycle field was validated but never forwarded at payload construction — new fields must be verified at BOTH ends. ts-rs codegen discipline is governed by vnc-024 ADR-001 (#4726): test-gated derive, `export_to = "../bindings/"`, drift-checked by the export sentinel test + CI `git diff --exit-code bindings/`.

### Decision

1. **New struct in wire.rs** (beside `TranscriptDeltaPayload`):
   ```rust
   /// F4b: client-declared cycle attribution (contract, not inference).
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   #[cfg_attr(test, derive(ts_rs::TS))]
   #[cfg_attr(test, ts(export, export_to = "../bindings/"))]
   pub struct CycleStampPayload {
       pub topic: String,
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub phase: Option<String>,
   }
   ```
2. **New field on `ImplantEvent`**, appended after `provider`:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub cycle_stamp: Option<CycleStampPayload>,
   ```
3. **7th ts-rs export**: `CycleStampPayload::export_all(&cfg)` added to the export sentinel test (wire.rs:443, currently "all six"; rename/recount to seven). `ImplantEvent.ts` regenerates with the new optional field — an expected, additive binding diff; all other committed bindings stay byte-identical.
4. **JS attach shape** (ADR-002 decoration): `cycle_stamp: { topic, phase }` with `phase` key omitted when null and the whole key omitted when no tracker file exists — exact `skip_serializing_if` parity with the existing `topic_signal`/`provider` omit-when-null rule in `implantEvent`.
5. **Tolerance matrix (AC-02, binding)**: old server + stamping client → unknown field ignored (no `deny_unknown_fields`); new server + Rust hook / old client → `cycle_stamp: None` → legacy chain (ADR-004). All pre-existing wire fixtures pass **unmodified** — the field never appears in them (`skip_serializing_if`). Mixed clients need no feature flag: stamp presence is per-event and self-describing.
6. **End-to-end round-trip AC (#3486 / SR-03, binding for delivery)**: one integration fixture must traverse the full path — client decoration attaches the stamp → server record path reads `event.cycle_stamp` → observation row lands with `topic_signal = stamp.topic`, `phase = stamp.phase`, `topic_source = 'declared'`. Both serde unit tests (None-absent / Some-present / null-tolerant, mirroring the col-017 `topic_signal` trio at wire.rs:1345-1367) AND the row-level assertion are required; field-exists-on-the-struct is not sufficient evidence. The server read must be verified at **all three** record sites (single ~listener.rs:719, ~:861, batch ~:1042) — the #3486 failure mode is a site forgetting the read.

### Consequences

Easier: the server distinguishes contract from guess by field presence alone — precedence becomes structural (presence-gated), not re-orderable; the typed payload gets a generated TS binding so the client never hand-mirrors it; fixtures prove F1-frozen compatibility mechanically. Harder: a 7th binding file to keep in CI drift-check; three server read sites must stay in lockstep (mitigated by the round-trip AC + a shared helper for stamp application rather than three inline copies — specification should mandate one `apply_stamp_to_row`-style function).

Cross-references: SCOPE AC-02, SR-03, lesson #3486, vnc-024 ADR-001 (#4726), ass-072 Q1 wire rationale, ADR-002 (attach point), ADR-004 (server read semantics), ADR-005 (topic_source).
