# C8 JS Bundle Decoder — `lib/hook-client/bundle.js` (+ Rust `bundle_codec`)

Risks: R-10, R-11, R-21. ACs: AC-24, FR-03. Extend the shared parity corpus (`crates/unimatrix-server/tests/fixtures` + `tests/bundle_codec.rs` + JS corpus test).

| Case | Rust | JS |
|---|---|---|
| v:2, `fp:"sha256:<64hex>"` (existing) | accept | accept |
| v:2, `fp:null` | accept | accept |
| `fp:""` | reject | reject |
| `fp:"sha256:"` | reject | reject |
| uppercase hex fp | reject | reject |
| missing `fp` key | reject | reject |
| extra key | reject | reject |
| pre-vnc-050 fixture bundle (frozen bytes) | accept, pinned | accept, pinned |

Tests:
- `parity_corpus_rust_js_identical` — both decoders produce identical accept/reject + decoded fields per corpus entry.
- `init_bundle_ca_stores_fingerprint_null` — `init --bundle <ca bundle>` → stored `fingerprint:null`; spawned bridge then uses CA branch (asserted via connecting to CA-trusted server, failing against self-signed).
- `init_bundle_pinned_unchanged` — stored record byte-identical to main golden (companion).
- `fp_null_against_self_signed_fails_loud_at_attach` — exit 1, token absent.
