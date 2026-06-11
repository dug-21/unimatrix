# Test Plan — BundleCodec (`run_client_bundle` Rust encoder + JS decoder)

> Server: `crates/unimatrix-server/src/client_bundle.rs` (new, sync pre-tokio C-10). Client: pure-JS decoder in `lib/hook-client/`. Wire form `unimatrix-bundle:<base64url(canonical-json)>` (ADR-001, C1). **Lead risks: R-02 (parity), R-05 (parser trust boundary).**

## AC-IDs covered
AC-W1-S5b (stdout/stderr split, token redacted), AC-W1-C9 (strict-schema load-bearing guard), AC-W1-C10 (4 KB cap before decode), AC-CT-C2 (bundle carries the parity-correct `fp`), AC-W1-S4 (served-cert fp), round-trip (R-05.3).

---

## Encoder — server side (Rust, `run_client_bundle`)

### Canonical encode (R-02 / C1)
- `test_bundle_encode_canonical_field_order` — encoded JSON field order is exactly `v,base_url,token,fp`; no insignificant whitespace; `v == 1`.
- `test_bundle_wire_has_scheme_prefix` — output line starts with `unimatrix-bundle:`.
- `test_bundle_base64url_no_padding` — payload is RFC4648 URL-safe base64, **no `=` padding**; alphabet uses `-`/`_` not `+`/`/`.
- `test_generate_c1_bundle_golden` (oracle/regen) — emit `{ fields, wire }` rows into `crates/unimatrix-server/tests/fixtures/c1c2-parity/bundle-golden.json`. Synthetic 64-hex token (NOT `sk-`-style — lesson #4792). Consumed by the JS decoder test; never hand-written on the JS side.
- `test_c1_bundle_golden_is_stable` (regression guard) — re-encode every row's `fields`; assert == committed `wire`.

### stdout/stderr split + token redaction (AC-W1-S5b, NFR-06)
- `test_client_bundle_stdout_is_opaque_blob_only` — capture stdout; assert it is exactly the single `unimatrix-bundle:…` line and nothing else (pipeable).
- `test_client_bundle_stderr_echoes_base_url_and_fp_only` — capture stderr; assert it contains the decoded `base_url` and `cert-fingerprint`, human-readable.
- `test_client_bundle_token_absent_from_stdout_and_stderr` — **load-bearing**: assert the token hex string appears in NEITHER stdout NOR stderr (it lives only inside the base64url stdout blob). Also assert it is not emitted to any log line during the run.
- `test_client_bundle_edit_me_placeholder_visible_on_stderr` — with `UNIMATRIX_PUBLIC_URL` unset, the `<EDIT-ME>` base-url is visible on stderr so the operator catches it before distributing (FR-A5b rationale).

### fp wiring (AC-W1-S4 / AC-CT-C2)
- `test_bundle_fp_field_matches_fingerprint_oracle` — the `fp` in the emitted bundle == `fingerprint_leaf_der(served_leaf_der)`; format `^sha256:[0-9a-f]{64}$`.

### sync pre-tokio (C-10)
- `test_run_client_bundle_is_sync_no_runtime` — structural: `run_client_bundle` runs without a tokio runtime (dispatched in the C-10 sync subcommand block), reading token + leaf DER directly from the data volume.

---

## Decoder — client side (pure JS)

### Guard ordering (AC-W1-C10, AC-W1-C9 — LOAD-BEARING, R-05)
The two guards MUST run in this order; tests prove the order, not just the outcomes.

- `test_bundle_length_cap_before_decode` (AC-W1-C10) — feed a raw pasted string **over 4 KB that is NOT valid base64url**; assert it is rejected by the **byte-length check**, with an error that is the length-cap error, **not** a base64 decode error and **not** a JSON parse error. This proves the cap ran *before* decode/parse (the parser-DoS guard).
- `test_bundle_at_exactly_cap_boundary` (edge) — a string at exactly 4 KB → accepted by the length gate (then proceeds to schema validation); at 4 KB + 1 → rejected on length.
- `test_bundle_strict_schema_reject_missing_field` (AC-W1-C9) — decoded JSON missing any of `v/base_url/token/fp` → rejected.
- `test_bundle_strict_schema_reject_extra_field` — a fifth key present → rejected (exactly-four-keys).
- `test_bundle_strict_schema_reject_wrong_type` — e.g. `v` as string, `token` as number → rejected.
- `test_bundle_reject_unknown_major_version` — `v: 2` → rejected (client rejects unknown major).
- `test_bundle_field_format_validation` — `base_url` must be `https://`; `token` must be 64 lowercase hex; `fp` must match `^sha256:[0-9a-f]{64}$`. Each violation rejected individually.

### Scheme + payload integrity (R-05)
- `test_bundle_reject_bad_scheme_prefix` — missing/wrong `unimatrix-bundle:` prefix → rejected fast.
- `test_bundle_reject_non_base64url_body` — within-cap but non-base64url body → rejected on decode (distinct from the over-cap length reject above).
- `test_bundle_reject_valid_base64url_invalid_json` (edge) — base64url decodes but the bytes are not valid JSON → rejected, no crash.
- `test_bundle_reject_truncated_payload` — truncated base64url → rejected, process survives.
- `test_bundle_parser_never_crashes_on_corpus` — a malformed/truncated/oversized corpus → every input rejected, the JS process survives every case (R-05 coverage requirement).

### Cross-stack parity + round-trip (R-02, AC-CT-C2)
- `test_bundle_decode_matches_golden_fields` — for each `bundle-golden.json` row, decode `row.wire` → assert fields == `row.fields` (JS consumes the Rust-oracle corpus; no hand-written expectation).
- `test_pin_matches_fingerprint_golden` — for each `fingerprint-golden.json` row, JS computes the pin over `row.der_b64` (the `cert.raw` path) → assert == `row.fp`.
- `test_bundle_roundtrip_server_encode_client_decode` — server `unimatrix-bundle:` encode → client decode yields identical `{base_url, token, fp}` (R-05.3).

## Concrete assertions
The order-proving test (AC-W1-C10) asserts the *error variant/message class* is the length-cap reject, not merely that some rejection occurred — that is the only way to prove the cap ran before decode.
