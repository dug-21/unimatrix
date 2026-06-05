# Scope Risk Assessment: vnc-024

F1 is pure plumbing, but it locks a contract F2–F5 inherit without re-negotiation. The dominant risk class is **silent infidelity** — codegen/round-trip that looks correct but isn't (Unimatrix #885, #3557, #4311 confirm serde round-trip is the single most-omitted test category and silent-fallthrough is a recurring gate failure). Reviewed: SCOPE.md, PRODUCT-VISION.md (principle 8), ass-068 Q3/Q4/Q7, ass-069 Q2/Q4/Q7.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ts-rs serde-compat fidelity on the hard cases — `#[serde(tag="type")]` internally-tagged enums (`HookRequest`/`HookResponse`) and `#[serde(flatten)] extra` (`HookInput`). If ts-rs emits a shape that compiles but mis-models the tag/flatten, the contract ships wrong and F2 builds on it. ts-rs is new to the workspace (no prior in-repo evidence). | High | Med | Architect: make the round-trip fixture (Goal 2) the authority over the generated type, not vice-versa. Spec: require at least one fixture per tagged variant AND a flatten-with-extra-keys fixture, asserted by the `node --test` harness — not just type compilation. |
| SR-02 | Codegen captures *structure* but not *serde behavior* — `None`-vs-omission under `skip_serializing_if`, `#[serde(default)]` optionality. #885/#3557: this exact gap caused a gate-failure rework; it is the most-skipped test category. | High | High | Spec: AC-06 must enumerate every `skip_serializing_if`/`default` field by name (already lists 4) and require a dual-direction assertion (emitted-as-absent vs deserialized-to-default), per pattern #3557. |
| SR-03 | ts-rs leaks into the runtime dep graph or shipped binary despite "dev-only" intent (feature-unification, transitive pull-in). | Med | Low | Spec: AC-15 + `cargo audit` is correct; add an explicit assertion that ts-rs appears under `[dev-dependencies]` only and is absent from `cargo tree --edges normal`. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Ship-a-contract-F2-inherits while keeping the TS client OUT: if the wire surface omits a field F2/#670 needs (e.g. delta `offset`/`bytes` typing, retention enum variants), F2 must re-open the contract — defeating F1's purpose. | High | Med | Architect: treat the generated bindings + retention enum as the frozen F2/#670 interface. Spike provenance (ass-069 Q2/Q7) names every field; cross-check the emitted `.ts` against that list before merge. |
| SR-05 | Scope creep pull-forward: the accept-and-drop guard tempts building the #670 in-memory buffer "while we're here." That buffer (offset-merge, distill, purge) is explicitly OUT. | Med | Med | Spec: AC-12 asserts *no persistence* (negative test: no observation row), not *buffering*. Reviewer rejects any in-memory transcript accumulation in F1. |
| SR-06 | Two transcript carriers coexist (`transcript_excerpt` legacy vs `transcript_delta` forward path). F1 ships both on the wire without stating precedence, inviting silent drift downstream. | Low | Med | Spec: document in the wire contract that streamed deltas supersede the excerpt (ass-069); F1 only needs the note, not the merge logic. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | **Secrets-to-disk hole (principle 8).** The `RecordEvent` generic-observation fall-through (`listener.rs:849`) persists any unknown `event_type` to durable storage. A `transcript_delta` carries raw conversation bytes that may contain secrets. Sequencing ("no client streams yet") is NOT a safety property. This is a silent-fallthrough exactly matching pattern #4311 — valid-looking output, no error, wrong durability. | High | High | Architect/Spec: the accept-and-drop branch is a REQUIRED guard, not an optimization. Make its negative test (AC-12: zero observation rows for a delta) a gate prerequisite per #4311 — green before any downstream AC. Cover both `/observe` (HTTP) and UDS dispatch paths. |
| SR-08 | `Accept` header must be read before `request.into_parts()` (`router.rs:203`) consumes the request; capturing it late silently yields JSON when text was requested. | Med | Med | Spec: AC-07/AC-08 must assert the negotiated content-type for both branches; constraint already names the ordering — keep it explicit. |
| SR-09 | `format_injection` re-implementation drift: any server text path that doesn't call the exact `hook.rs:1047` fn breaks the byte-identical gate now or later. | Med | Low | Spec: AC-07 byte-identical assertion is correct; constraint 4 forbids re-impl — keep as a hard gate. |

## Assumptions

- **A1 (SCOPE Background Research / ass-068 Q3):** ts-rs serde-compat handles the workspace's exact annotation set. If false → SR-01/SR-02 materialize and the migration's anti-drift premise fails. Round-trip fixtures are the safety net, not codegen alone.
- **A2 (Non-Goals / ass-069 Q7):** the `RetentionConfig` enum (`PurgeOnCycleClose | RetainDays(u32)`) is a sufficient enterprise seam. If a future policy needs encrypt-at-rest/data-residency the enum extends without re-architecting (goal #4710 "extend, never re-architect"). A bare `u32` would NOT extend — the enum choice is load-bearing and correct.
- **A3 (Background Research / `listener.rs:849`):** the generic-observation arm is the only durable-write path a delta can reach. If another arm also persists, SR-07's guard is incomplete.

## Design Recommendations

1. **SR-07 first.** Make the accept-and-drop negative test a gate prerequisite (#4311): no downstream AC is trusted until raw deltas provably hit no disk path — HTTP *and* UDS.
2. **SR-01/SR-02:** the round-trip fixture is the contract authority, not the generated `.ts`. Require per-tagged-variant + flatten + None-omission fixtures asserted by `node --test` (#885, #3557), not type-compile-only.
3. **SR-04:** freeze the emitted bindings + retention enum against the ass-069 Q2/Q7 field list before merge — F2/#670 inherit this surface with no second negotiation.
4. **SR-05:** AC-12 asserts non-persistence, never buffering; reviewer rejects any #670 pull-forward.
5. **A2/goal #4710:** keep the retention field enum-typed; reject any bare-`u32` simplification in review.
