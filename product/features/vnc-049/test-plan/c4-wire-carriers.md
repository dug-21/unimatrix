# Test Plan — C4 Wire Carriers (model_id on HookInput + ImplantEvent)

`crates/unimatrix-engine/src/wire.rs` — add `model_id: Option<String>` (`#[serde(default)]` on
HookInput; `#[serde(default, skip_serializing_if="Option::is_none")]` on ImplantEvent). Regenerate
ts-rs bindings. Under-cap file (237 code-lines).

Risks owned: **R-10** (carrier blast radius). ACs: contributes to AC-06 (the carrier that R-01/c5
proves end-path). Lesson **#5670**: model_id must land at hook.rs extract+insert, JS port, a
parity-corpus case, AND one client→listener→DB crossing test — the last two are owned by c8 and c5.

Test surface: Rust `#[cfg(test)]` in `wire.rs` (mod tests) + the ts-rs binding gate + the JS
contract-roundtrip gate.

## Unit expectations

### serde back-compat (R-10.2 — the critical back-compat assertion)
- `test_hookinput_deserializes_without_model_id` — a HookInput JSON frame WITHOUT `model_id`
  deserializes cleanly to `model_id: None` (`#[serde(default)]`). This is the pre-vnc-049 /
  cloud-model / other-harness frame; it MUST NOT break.
- `test_implantevent_deserializes_without_model_id` — same for ImplantEvent.
- `test_implantevent_omits_model_id_when_none` — `skip_serializing_if` means a None `model_id` is
  NOT serialized (keeps existing fixtures byte-stable — see the ts-rs/contract gate below).
- `test_hookinput_roundtrip_with_model_id` — a frame carrying `model_id="qwen3-coder"` round-trips
  serialize→deserialize preserving the value.

### ts-rs binding drift gate (vnc-024 #4726, R-10.1)
- Existing gate `test_export_bindings_all_seven_written_and_nonempty` (`wire.rs:500`) MUST stay
  green after adding `model_id` to HookInput and ImplantEvent — regenerate the committed
  `crates/unimatrix-engine/bindings/*.ts` (HookInput.ts, ImplantEvent.ts) and commit them, else the
  drift gate fails. Test plan requirement: the regenerated bindings are committed in the same change.
- **JS contract-roundtrip** (`packages/unimatrix/test/hook-client/contract-roundtrip.test.js`, AC-14)
  — the client-produced frames vs committed Rust ts-rs fixtures (`crates/unimatrix-engine/bindings/
  fixtures/*.json`). Requirement: `test_record_event_omitted_topic_provider_matches_fixture` (:143)
  and the record-event fixtures stay valid; add/adjust a fixture that carries `model_id` so the JS
  side and Rust side agree on the new field. `contract.test.mjs` narrowing must still pass.

### Validation contract (R-15 — shared with c5)
- `test_model_id_format_contract` — document the `^[a-z0-9_-]{1,64}$` contract for `model_id`.
  Where validation is enforced (wire deserialize vs listener bind) is pinned at Stage 3b; the test
  asserts an over-length / bad-char `model_id` is rejected or sanitized before it reaches the DB
  (the authoritative persist-side assertion is in c5).

## Cross-references (lesson #5670 — four coordinated landings)
1. hook.rs extract+insert of model_id → covered by c2 (canonicalization carries model_id) + c5
   (bind at INSERT).
2. JS hook-client port → c3.
3. Parity-corpus case carrying model_id → c8.
4. One client→listener→DB crossing test carrying model_id end to end → **c5** (the R-01 gating test
   also serves this). A wire-only roundtrip test here does NOT discharge #5670's crossing-test
   requirement — it only proves the field survives (de)serialization.
