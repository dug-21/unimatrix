# Test Plan — Bundle Codec (Rust, sole encoder)

> Component: `crates/unimatrix-server/src/client_bundle.rs` · Surface: `tests/bundle_codec.rs` + `tests/fixtures/c1c2-parity/bundle-golden.json` · Risks: R-03, R-04 (Crit) · AC-05

## Scope
Rust is the SOLE `v:2` encoder. `encode_bundle` composes BOTH `mcp_url` and `observe_url` from `{public_base}` + the route grammar; `validate_schema` enforces exactly 5 keys `{v, mcp_url, observe_url, token, fp}`; `BUNDLE_VERSION = 2`. EXTEND the existing corpus (#4956) — do NOT scaffold new.

## Unit Test Expectations

### Encode / round-trip (R-03)
- `test_encode_bundle_v2_composes_both_urls` — assert `encode_bundle(2, mcp_url, observe_url, token, fp)` yields `unimatrix-bundle:<base64url-nopad(canonical-json)>` whose decoded JSON has exactly keys `{v:2, mcp_url, observe_url, token, fp}` in canonical order.
- `test_decode_bundle_v2_round_trip` — `decode_bundle(encode_bundle(...))` returns `Bundle { v:2, mcp_url, observe_url, token, fp }` field-equal to input.
- `test_mcp_observe_url_composition` — given `public_base = "https://h"` and slug `alpha`, assert `mcp_url == "https://h/v1/alpha"` and `observe_url == "https://h/v1/alpha/observe"` (URLs composed from route grammar, not a bare slug).

### Strict-reject matrix on the Rust side (R-03)
`decode_bundle`/`validate_schema` must reject with `BundleError`/`ServerError`, NO partial accept:
- `test_reject_missing_key` — JSON missing `observe_url` → reject.
- `test_reject_extra_key` — JSON with a 6th key → reject.
- `test_reject_wrong_type_key` — `v` as string, or `mcp_url` as number → reject.
- `test_reject_non_https_url` — `mcp_url = "http://h/..."` or `"ftp://..."` → reject (assert `https://`-only).
- `test_reject_unknown_major_version` — `v: 3` → reject.

### v:1 hard-cut (R-04)
- `test_reject_v1_shaped_bundle` — a well-formed `v:1` artifact (`{v:1, base_url, token, fp}`) presented to `v:2` decode → loud reject; assert NO `v:1` compat arm survives (no `base_url` acceptance path).
- `test_no_v1_fallback_decode_path` — assert there is exactly one version arm (`v == 2`); any other major fails closed.

### Guard ordering (R-03 / NFR-08)
- `test_max_raw_len_runs_first` — a raw paste exceeding `MAX_RAW_LEN` fails the length cap BEFORE scheme/base64url/JSON/schema (assert the error is the length-cap variant, not a downstream parse error). At exactly `MAX_RAW_LEN`: accepted to that boundary; one byte over: rejected.

## Parity Corpus (R-03 — the shared oracle)
- **Regenerate** `bundle-golden.json` from the Rust oracle for `v:2`: update `GoldenFields` to `{v, mcp_url, observe_url, token, fp}`; `test_generate_c1_bundle_golden` (`#[ignore]`) re-emits rows.
- `test_c1_bundle_golden_is_stable` — re-encode every row's fields, assert byte-equality with committed `wire`. A canonical key-order / base64url-alphabet / escaping change fails HERE, not at a user paste (R-03 atomicity guard).
- Synthetic 64-hex tokens only — never `sk-`-style secrets (lesson #4792).

## Edge Cases (from Risk Strategy)
- Bundle at exactly `MAX_RAW_LEN` and one byte over.
- Malformed / non-`https://` URL in either URL field.
- Unknown major version (forward-compat reject).

## Coverage Requirement
Encode and decode move atomically; the corpus is the `v:2` shared oracle; no single-side `v:2` passes its own round-trip against a `v:1` counterpart. Strict-reject matrix complete on the Rust side; the JS side mirrors it (bundle-decoder-js.md).
