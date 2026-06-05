# Agent Report: vnc-024-agent-1-architect

## Task
Produce the architecture for vnc-024 (issue #672) — F1 wire/server foundation: ts-rs codegen + CI
diff-gate + node round-trip fixtures; /observe content negotiation; transcript_delta accept-and-drop
guard; transcript_retention enum.

## Deliverables
- ARCHITECTURE.md: product/features/vnc-024/architecture/ARCHITECTURE.md
- ADR-001..005 (one file per decision) in product/features/vnc-024/architecture/

## Key Decisions
1. ADR-001 — ts-rs dev-dependency; derive TS on the 5 wire types; codegen on cargo test; bindings at crates/unimatrix-engine/bindings/ (CI diff-gate, F2/F5 vendor).
2. ADR-002 — round-trip JSON fixtures (Rust emit + node --test) are the contract authority over the generated .ts; assert serde BEHAVIOR (None-omission, tagged variant, flatten).
3. ADR-003 — /observe content negotiation: Accept read before into_parts; text/plain for Entries+BriefingContent ONLY; Pong/Ack/Error stay JSON; reuse format_injection (no re-impl); UDS untouched.
4. ADR-004 — transcript_delta is a new event_type value (no new variant) + REQUIRED accept-and-drop guard in the RecordEvent arm (after sanitize, before persistence) covering HTTP+UDS via one dispatch_request branch; persists nothing (secrets posture, SR-07/principle 8). Negative test is a gate prerequisite.
5. ADR-005 — transcript_retention as TranscriptRetention enum (PurgeOnCycleClose | RetainDays(u32)) threaded through all four RetentionConfig touchpoints (struct/default/validate/merge); enterprise seam (#4710).

## Integration surfaces verified against real code (2026-06-05)
- wire.rs: HookInput/HookRequest/HookResponse/ImplantEvent/EntryPayload confirmed; event_type free-form String, payload serde_json::Value; MAX_PAYLOAD_SIZE 1 MiB.
- router.rs: Accept must be read before request.into_parts() at :203; CONTENT_LENGTH pattern at :191-196.
- observe.rs: observe_response_to_http(:18), prefix_session_id(:69).
- hook.rs:1047 format_injection (free fn, same crate — needs pub(crate)).
- listener.rs: RecordEvent arm; guard insertion point after sanitize_session_id (:757), before feature extraction (:793) and the insert_observation disk write (:849-863). Both transports converge here.
- config.rs RetentionConfig: struct :1499, defaulters/Default :1541-1559, validate() :1571, project-wins merge :3307-3329. Struct derives PartialEq (merge uses !=) — enum must derive PartialEq.

## Open questions (non-blocking, for delivery)
- Confirm serde TOML representation for TranscriptRetention (externally-tagged: "PurgeOnCycleClose" vs { RetainDays = N }).
- Confirm the max_bytes injection budget the /observe text path passes to format_injection so AC-07 byte-identity holds against the production UDS caller.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- prior wire ADRs (col-022 #1273 event_type-routing, vnc-013 #4306 provider field, ADR-005 #246 length-prefixed JSON), vnc-024 /observe content-negotiation pattern #4703, ass-068/069 FINDINGS. All built upon; none contradicted/superseded.
- Stored: entries #4712 (ADR-001), #4713 (ADR-002), #4714 (ADR-003), #4715 (ADR-004), #4716 (ADR-005) via context_store category=decision topic=vnc-024.

---

# Design-Review Addendum (issue #672) — two FINAL decisions encoded

Encoded two FINAL design-review decisions (human: Doug). Not relitigated.

## Files changed
| File | Change |
|------|--------|
| ADR-001-ts-rs-codegen-dev-dependency.md | Five exported wire types -> six: added typed `TranscriptDeltaPayload {offset:u64, bytes:String}` ts-rs binding so the one new field isn't emitted as any/JsonValue. |
| ADR-002-round-trip-fixtures-as-contract-authority.md | Named the delta payload as a covered fixture; round-trip now DUAL-SIDED (Rust<->TS, like AC-06), parsing into `TranscriptDeltaPayload` both sides. |
| ADR-004-transcript-delta-accept-and-drop-guard.md | Documented the typed `TranscriptDeltaPayload` struct (wire carrier unchanged — still rides `ImplantEvent.payload`); guard parses payload into the struct. |
| ADR-005-transcript-retention-enum.md | OSS `validate()` now REJECTS `RetainDays` as enterprise-only (was range-check accept). Added scope definition (raw session transcript = ephemeral working state) + no-content-scanner guardrail. R-10 mostly dissolves. |
| ARCHITECTURE.md | Codegen diagram (6 types), Deliverable 1/3/4 rows, data-flow, tech-decisions table, integration surface (`TranscriptDeltaPayload`), open questions (R-10 dissolution). |

## Typed-delta mechanism settled
Dedicated `TranscriptDeltaPayload { offset: u64, bytes: String }` struct deriving `TS` + `#[ts(export)]` = 6th exported binding. Wire carrier UNCHANGED (value still rides `ImplantEvent.payload: serde_json::Value`; no new HookRequest/RecordEvent variant). Struct = (a) typed cross-language contract killing F2 hand-mirror drift, (b) the deserialization target the accept-and-drop guard parses into. Round-trip fixture is dual-sided.

## RetainDays settled
Enum shape kept as enterprise seam; OSS `validate()` hard-rejects `RetainDays` (enterprise-only error), not accepted-and-ignored. `PurgeOnCycleClose` is the only OSS-accepted value.

## Knowledge Stewardship (design-review pass)
- Queried: mcp__unimatrix__context_briefing + context_lookup -- located existing vnc-024 ADR entries #4712/#4713/#4714/#4715/#4716.
- Stored: corrected the 4 changed ADRs in place via context_correct (required update method): #4712->#4718 (ADR-001), #4713->#4719 (ADR-002), #4715->#4720 (ADR-004), #4716->#4721 (ADR-005). Deprecated orphan #4717 (stray context_store duplicate, superseded by the #4712 correction chain). ADR-003 #4714 unchanged.
