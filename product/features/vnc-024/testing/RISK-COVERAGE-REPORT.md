# Risk Coverage Report: vnc-024

> F1 wire/server foundation (#672). Stage 3c test execution. Strategy: **evidence over
> trust** — round-trip fixtures (not generated `.ts`) are the contract authority, and the
> zero-durable-rows guard (AC-12) is a GATE PREREQUISITE discharged across three arms.
>
> **GATE STATUS (AC-12 / R-03 / R-04): GREEN on all three arms** — UDS dispatch + RecordEvents
> batch (Stage 3b unit/component, 5/5) and the HTTP-transport arm (Stage 3c, this report). See
> AC-12 row and the "AC-12 Three-Arm Gate" section.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | ts-rs compiles but mis-models serde (tag/flatten/None/typed-delta) | `wire::tests::*` (90), `contract.test.mjs` (4 incl. dual-sided delta) | PASS | Full |
| R-02 | None-vs-omission one-directional/trivial | `wire::tests::implant_event_serialize_none_omits_field`, `..._some_includes_field`, `context_search_source_*`, `compact_payload_*`; node "None-vs-omission" | PASS | Full |
| R-03 | Secrets-to-disk hole (delta → durable SQLite) — **GATE** | `test_transcript_delta_uds_acks_zero_rows` (zero rows, queried directly), `test_transcript_delta_parses_into_typed_payload` | PASS | Full |
| R-04 | Guard one-transport / batch missed — **GATE** | UDS: `..._uds_acks_zero_rows`; batch: `..._in_batch_dropped_rest_persist`; HTTP arm: `test_observe_http_prefix_session_id_preserves_delta_routing`, `..._batch_prefix_preserves_delta_drop_routing`, `..._delta_body_deserializes_to_record_event` | PASS | Full |
| R-05 | format_injection byte-identity / budget drift | `test_observe_text_entries_byte_identical`, `..._over_budget_matches_truncation`, `..._empty_returns_204`, `test_observe_http_accept_text_plain_negotiates_text` (boundary byte-identity), `uds::hook::tests::format_injection_*` (11) | PASS | Full |
| R-06 | Non-injection response text-formatted | `test_observe_text_pong_stays_json`, `..._ack_stays_204_json_path`, `..._error_stays_json`, `..._briefingcontent_returns_text`, `test_observe_http_text_allowlist_at_boundary` | PASS | Full |
| R-07 | Accept read after `into_parts` (header lost) | `test_observe_http_accept_text_plain_negotiates_text`, `..._no_accept_negotiates_json`, `..._accept_json_negotiates_json`, `..._accept_multivalue_*`, `..._accept_wildcard_*` (boundary extraction replays exact handler ordering) | PASS | Full |
| R-08 | Frozen contract omits F2/#670 field | `wire::export_bindings_*`, fixtures cross-checked vs ass-069 list; `TranscriptDeltaPayload.ts` 6th export present; retention enum carries both variants | PASS | Full |
| R-09 | Retention touchpoint missed / weak reject | `test_retention_compiled_default_is_purge`, `..._absent_block_defaults_purge`, `..._present_but_field_absent_defaults_purge`, `test_validate_rejects_retaindays_enterprise_only`, `test_retention_merge_project_wins`, `test_retention_merge_revalidated_rejects_retaindays` | PASS | Full |
| R-10 | Retention TOML repr (mostly dissolved) | `test_purge_on_cycle_close_toml_deserializes`, `test_bare_u32_rejected` | PASS | Full |
| R-11 | PartialEq on TranscriptRetention | `test_retention_merge_project_wins` (real `!=` inequality exercised; derive present, compiles) | PASS | Full |
| R-12 | ts-rs runtime leak | `cargo tree -p unimatrix-engine --edges normal` → ts-rs ABSENT; `--edges dev` → present (v12.0.1) | PASS | Full |
| R-13 | Two carriers drift (precedence note) | Wire-contract precedence note (deltas supersede excerpt); no merge logic in F1 (reviewer/Stage 3a) | PASS | Full (doc) |
| R-14 | CI diff-gate non-functional (meta-gate) | `.github/workflows/ci.yml:39-46` (cargo test → `git diff --exit-code` bindings → node); **self-test run**: mutate→exit 1, restore→exit 0 | PASS | Full |

## AC-12 Three-Arm Gate (R-03 / R-04 — GATE PREREQUISITE)

The zero-durable-rows obligation is discharged across **all three required arms** — green:

| Arm | Test | Transport | Assertion | Result |
|-----|------|-----------|-----------|--------|
| UDS dispatch | `test_transcript_delta_uds_acks_zero_rows` | direct UDS | `Ack` + `SELECT COUNT(*) FROM observations == 0` (queried directly, not via search) | PASS |
| RecordEvents batch | `test_transcript_delta_in_batch_dropped_rest_persist` | UDS batch | delta dropped (0 rows for delta session_id), N=2 normal events persist | PASS |
| HTTP `/observe` | `test_observe_http_prefix_session_id_preserves_delta_routing` + `..._batch_prefix_preserves_delta_drop_routing` + `..._delta_body_deserializes_to_record_event` | HTTP tower | HTTP-path `prefix_session_id` mutates session_id, but guard keys on `event_type`, so the delta STILL routes to drop (R-04 integration trap closed) | PASS |

Supporting: `test_transcript_delta_malformed_payload_still_acks_zero_rows` (offset:0/empty bytes,
missing/extra keys → still `Ack`, 0 rows, no error); `test_transcript_delta_requires_session_write`
(NFR-04 — guard sits after the capability check; no new auth surface);
`test_transcript_delta_parses_into_typed_payload` + `test_observe_http_delta_empty_bytes_routes_to_drop`
(shared-shape coupling: guard parses into the typed `TranscriptDeltaPayload`, not raw `Value`).

**Why the HTTP arm is structural, not row-counting:** HTTP `/observe` and UDS converge on the same
`dispatch_request` `RecordEvent` arm; the zero-rows behavior of that single arm is proven by the UDS
tests. The HTTP-specific risk (R-04 trap) is that the HTTP path's `prefix_session_id` transform could
bypass the guard — so the HTTP arm asserts the transform preserves the drop-routing key. A full
HTTP-boot row-count test would require the embedding model and re-exercise the identical dispatch arm;
the structural assertion targets exactly the HTTP-unique surface. The mapper-level fire-and-forget
`Ack` for a delta is also covered (delta is never text — ADR-003/004).

## Test Results

### Unit / Component Tests (Rust — `cargo test`, summaries only)

| Crate / group | Total | Passed | Failed |
|---------------|-------|--------|--------|
| `unimatrix-engine` (full: wire 90 + scenarios/confidence/integration) | full suite | all | 0 |
| `unimatrix-engine` `wire::tests` (R-01/02/08/11 Rust side + fixtures) | 90 | 90 | 0 |
| `unimatrix-server` lib (whole crate) | 3503 | 3502 | 1 (pre-existing, GH#684) |
| — `transcript_delta` guard (AC-12 UDS+batch+auth+typed) | 5 | 5 | 0 |
| — `observe_text` content-negotiation mapper (AC-07/08/09) | 7 | 7 | 0 |
| — `observe_http` **NEW boundary integration** (AC-07/08/09/12, R-07) | 10 | 10 | 0 |
| — `retention` config (AC-13/14, R-09/10/11) | 11 | 11 | 0 |
| — `uds::hook` (AC-10 parity: format_injection unchanged) | 196 | 196 | 0 |

The single server-lib failure is `server::tests::test_schema_integer_type_preserved_for_all_nine_fields`
— **pre-existing, out-of-scope** (see Gaps / Pre-Existing Failures).

### Node Round-Trip Harness (`node --test contract.test.mjs`)

| Total | Passed | Failed | Covers |
|-------|--------|--------|--------|
| 4 | 4 | 0 | AC-04 (literal `type` per variant), AC-05 (per-variant fixtures), AC-06 (None-vs-omission, dual-direction), AC-11 (`TranscriptDeltaPayload` TS→Rust dual-sided) |

### Integration Tests (infra-001 — stdio MCP harness)

| Metric | Value |
|--------|-------|
| Smoke selected | 23 |
| Passed (with corrected launch) | 19 |
| Failed (all pre-existing / unrelated) | 4 |
| **Gate status as committed** | **BLOCKED by harness drift — GH#685** |

**The infra-001 smoke gate could not run against the committed harness:** every test errored at init
(`ServerDied: Server exited with code 2`) because `harness/client.py:97-98` launches the binary with a
stale `serve --stdio` subcommand the current binary no longer accepts (default invocation is now the
stdio MCP path). Filed **GH#685**; this is harness-vs-binary CLI drift from the rmcp-1.7 migration
(vnc-023 #674), NOT a vnc-024 defect (vnc-024 touched neither `main.rs` nor `main_tests.rs`).

With the launch corrected **locally for verification only** (committed harness left untouched), the
server boots and **19/23 smoke tests pass**, proving vnc-024 is non-regressive on the MCP stdio surface
(handshake, store, search, scan, capability enforcement, restart-persistence all green). The 4 remaining
failures are all pre-existing and unrelated to vnc-024:

| Test | Failure | Attribution |
|------|---------|-------------|
| `test_tools.py::test_get_with_string_id` | `-32602: invalid type: string "1", expected i64` | rmcp-1.7 string-int coercion regression — **GH#684** (same root cause as the unit-test failure) |
| `test_tools.py::test_deprecate_with_string_id` | same `-32602` string-int | **GH#684** |
| `test_volume.py::test_store_1000_entries` | `-32007: Unknown category 'feature'` | category-allowlist / config drift (cf. closed GH#632); harness config, not vnc-024 |
| `test_lifecycle.py::test_cycle_start_goal_does_not_block_response` | `-32602: tool not found` (`context_cycle`) | harness references a tool absent from this build; not vnc-024 |

Per OVERVIEW §"infra-001 does NOT cover vnc-024's integration ACs": AC-07/08/09/10/12 are out-of-band of
the stdio harness (HTTP `/observe` tower + UDS dispatch) and are fully covered by the server-crate Rust
integration tests above — not by any infra-001 suite. No new infra-001 suite was added (per plan).

## xfail / GH Issue References

| Issue | Title | Nature | Marker |
|-------|-------|--------|--------|
| [GH#684](https://github.com/dug-21/unimatrix/issues/684) | `test_schema_integer_type_preserved_for_all_nine_fields` + string-id integration: rmcp-1.7 `id` schema/coercion | Pre-existing (vnc-023 #674) | **Unit test** — no `@pytest.mark.xfail` applies; tracked via issue. Integration manifestations (`test_get_with_string_id`, `test_deprecate_with_string_id`) noted in the issue thread. |
| [GH#685](https://github.com/dug-21/unimatrix/issues/685) | infra-001 harness launches server with stale `serve --stdio` — every suite errors at init | Pre-existing harness drift (vnc-023/vnc-022 CLI change) | Harness launch fix; gates entire suite. No marker (the harness file is unmodified in this PR). |

No integration tests were deleted, commented out, or `xfail`-masked to make a suite pass. The temporary
launch patch used to verify smoke behavior was reverted (`harness/client.py` is clean — verified
`git diff --stat` empty).

## Gaps

**None for vnc-024's risk surface.** Every Critical and High risk (R-01..R-09, R-14) has a passing,
specific test; the AC-12 gate is green on all three arms. R-10..R-13 (Medium/Low) covered.

**Pre-existing failures (NOT gaps in vnc-024 coverage):**
1. **GH#684** — rmcp-1.7 changed `context_lookup`/`context_get`/`context_deprecate` integer-`id`
   handling. Unit-test assertion `["integer","null"] != "integer"` and integration `string "1" → i64`
   share this cause. `server.rs` last modified by vnc-023 (#674); vnc-024 made zero changes to it.
2. **GH#685** — infra-001 harness cannot launch the current binary (`serve` subcommand removed by the
   rmcp migration). Blocks the smoke gate as committed; vnc-024 touched no CLI entrypoint.

Both warrant their own bugfix-protocol sessions (rmcp-1.7 schema surface; harness launch contract).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `ts-rs` under `[dev-dependencies]`; `wire.rs` derives `TS`+`#[ts(export)]` on the 6 types; `cargo tree --edges dev` shows ts-rs v12.0.1 |
| AC-02 | PASS | `cargo test -p unimatrix-engine` regenerates 6 committed `.ts` files; `git diff --exit-code bindings/` clean after regen |
| AC-03 | PASS | CI gate `ci.yml:39-46` (test → diff → node), correctly ordered; **self-test: mutate→exit 1, restore→exit 0** |
| AC-04 | PASS | `contract.test.mjs` "every request/response fixture narrows to the correct tagged variant (literal type)" |
| AC-05 | PASS | Rust `wire::tests` round-trip + `node --test` (4/4); both deserialize the same committed fixtures |
| AC-06 | PASS | `wire::tests::implant_event_serialize_{none_omits,some_includes}_field`, `context_search_source_*`, `compact_payload_*`; node None-vs-omission (dual-direction) |
| AC-07 | PASS | `test_observe_text_entries_byte_identical` + `..._over_budget_matches_truncation` (== `format_injection(&items, MAX_INJECTION_BYTES=1400)`, production budget, truncation); `test_observe_http_accept_text_plain_negotiates_text` (boundary content-type=text/plain) |
| AC-08 | PASS | `test_observe_json_envelope_unchanged_all_variants` (Ack→204, Entries/BriefingContent/Pong→200 JSON, Error→400 JSON); `test_observe_http_no_accept_negotiates_json`, `..._accept_json_negotiates_json` (boundary) |
| AC-09 | PASS | `test_observe_text_briefingcontent_returns_text`; `..._pong_stays_json` (server_version parseable), `..._ack_stays_204_json_path`, `..._error_stays_json`; `test_observe_http_text_allowlist_at_boundary` |
| AC-10 | PASS | `uds::hook::tests::format_injection_*` (196 hook tests green); only `format_injection` visibility bumped to `pub(crate)` — no behavior change to the UDS path |
| AC-11 | PASS | `contract.test.mjs` delta TS→Rust + `wire::tests` Rust→TS into `TranscriptDeltaPayload`; `TranscriptDeltaPayload.ts` is the 6th export; carrier rides free-form `event_type`/`payload` (`test_observe_http_delta_body_deserializes_to_record_event`) |
| **AC-12 (GATE)** | **PASS — all 3 arms** | UDS: `test_transcript_delta_uds_acks_zero_rows` (0 rows); batch: `..._in_batch_dropped_rest_persist`; HTTP: `test_observe_http_prefix_session_id_preserves_delta_routing` + `..._batch_prefix_*` + `..._delta_body_deserializes_*`; guard parses typed payload, not raw Value; early-return (not col-022 fall-through #1266) |
| AC-13 | PASS | `test_retention_compiled_default_is_purge`, `..._absent_block_defaults_purge`, `test_validate_rejects_retaindays_enterprise_only` (enterprise-only error naming RetainDays, any N incl. 0, NOT a range error), `test_purge_on_cycle_close_toml_deserializes`, `test_bare_u32_rejected`; enum + `PartialEq` derive |
| AC-14 | PASS | `test_retention_merge_project_wins` (project wins per-field merge) + `test_retention_merge_revalidated_rejects_retaindays` (merged result re-validated, #3905) |
| AC-15 | PASS | `cargo tree -p unimatrix-engine --edges normal` → ts-rs ABSENT from runtime edges; present only under dev-deps |

All 15 ACs PASS. AC-12 gate prerequisite green on UDS + batch + HTTP before downstream ACs trusted.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #4452 (gate-fix integration test must use an agent that exercises the dropped path — applied: the AC-12 HTTP arm asserts the `prefix_session_id` transform preserves drop-routing), #4515 (gate-3b "all code, zero tests" failure mode — confirmed NOT present here; Stage 3b shipped comprehensive UDS+config+mapper tests), ADR-003 #4714 (HTTP /observe content negotiation allowlist).
- Stored: pattern entry below — a reusable testing pattern discovered (transport-convergence guard testing) not previously generalized in Unimatrix.
