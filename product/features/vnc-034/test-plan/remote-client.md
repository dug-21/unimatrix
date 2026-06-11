# Test Plan — RemoteClient (`init --remote`)

> `lib/hook-client/` (pure JS, zero native binary, zero added runtime deps). `init --remote <bundle> [--slug <s>]`: bundle parse (delegates to the C1 decoder — see `bundle-codec.md`), cert pin via custom `checkServerIdentity`, slug append, skills copy, size gate. **Lead risks: R-02 (pin), R-05 (parse boundary), R-06 (1:1), R-12 (size/secrets).**

## AC-IDs covered
AC-W1-C1 (per-OS working client), AC-W1-C2 (pin exact cert, reject mismatch), AC-W1-C3 (<250 KB), AC-W1-C5 (one-project-only, unrepresentable), AC-W1-C6 (skills copied, no CLAUDE.md block, pointer printed), AC-W1-C7 (N-clients-one-project shared path), AC-W1-C8 (onboarding — manual), AC-CT-ROT (diagnosable mismatch — client half).

---

## Unit tests (JS)

### R-02 — cert pinning (AC-W1-C2)
- `test_checkserveridentity_computes_pin_over_cert_raw` — the custom `checkServerIdentity` computes `sha256(cert.raw)` formatted as `sha256:<lowercase-hex>` and constant-form-compares to the pinned `fp`. Uses the committed `fingerprint-golden.json` corpus (never a hand-written expected value).
- `test_pin_match_accepts` — presented cert whose DER matches the pinned `fp` → identity check passes.
- `test_pin_mismatch_rejects_with_diagnosable_error` (AC-W1-C2, AC-CT-ROT client half) — a changed/wrong cert → rejected with a **clear, diagnosable** error naming **expected vs presented** `sha256:` and pointing to "re-run `client-bundle` and `init --remote`". Assert it is NOT a bare opaque TLS handshake error.
- `test_pin_bypasses_ca_chain` — the pin path does not consult a CA trust store (self-signed, no CA dep); assert no CA-validation branch is reachable.

### R-05 — bundle ingestion boundary (delegates to bundle-codec)
- `test_init_remote_rejects_malformed_bundle` — `init --remote <malformed>` surfaces the decoder's rejection and **does not create a client config / store**; process survives (links AC-W1-C9/C10 through the init entry point).
- `test_init_remote_length_cap_before_decode` — an over-cap raw `--remote` argument is rejected on length before any decode (the AC-W1-C10 guard reached through `init`).

### R-06 — bound to exactly one project (AC-W1-C5, FR-X2)
- `test_client_has_no_second_project_field` — **source assertion**: the resulting client config / API has no field or method to address a second project; the slug is baked into the base-url at init. Mis-target is unrepresentable, not runtime-rejected.
- `test_slug_appended_to_base_url` — `--slug foo` → effective endpoint `base_url + /v1/foo/tools/...`; no `--slug` → `/v1/tools/...` (Default). The slug is appended client-side (NOT in the bundle, C1/C5).
- `test_attach_unregistered_slug_errors_no_store` (AC-W1-C4 client half) — attaching a slug the server has not registered → client errors and creates no store. (Server-registration is Wave 2, but the client's *no-auto-create* behavior is Wave-1.)

### R-12 — size + secret hygiene
- `test_remote_install_under_250kb` (AC-W1-C3, NFR-01) — measure the installed client footprint; assert `< 250 KB` hard gate. Zero added runtime deps; no native binary.
- `test_token_not_logged_by_client` — `init --remote` never prints the token to stdout/stderr/logs (the token lands only in the persisted client config).

### AC-W1-C6 — onboarding artifacts
- `test_skills_copied` — post-init filesystem has the skills present.
- `test_claudemd_block_not_appended` — `init` does NOT append a CLAUDE.md knowledge block (`uni-init` owns that).
- `test_unimatrix_init_pointer_printed` — stdout prints the `/unimatrix-init` pointer.

### AC-W1-C7 — N-clients-one-project shared code path
- `test_two_distinct_clis_share_one_client_codepath` — two distinct LLM CLIs initialized from the same bundle/slug use the **single** shared client code path (no per-LLM branch). Source/structural assertion of the shared path; live data-sharing is AC-W2-R5 (Wave 2).

## Integration tests (per-OS — see OVERVIEW §4.2)
- **AC-W1-C1 per-OS:** CI matrix Linux / macOS-arm / Windows — `init --remote <bundle>` then a live knowledge call over HTTPS against the served cert. Where a live runner is unavailable, degrade to documented manual walkthrough + the platform-independent pin/parse unit tests; flag in coverage report (do not silently drop).
- **AC-CT-ROT reconnect:** rotate server cert WITHOUT re-bundle → reconnect surfaces the diagnosable mismatch (above). Then re-bundle + re-init → reconnect succeeds.
- **AC-W1-C8 onboarding:** end-to-end timed manual walkthrough (install → ingest bundle → connect); no manual cert handling beyond the bundle.

## Edge cases (assigned here)
- **Windows HTTPS-remote-only:** assert NO local-mode code path is reachable on Windows (no UDS branch) — `RISK-TEST-STRATEGY §Edge Cases`.
- Bundle with valid base64url but invalid JSON reaches `init` → clean error, no partial config written.
- Re-running `init --remote` with a new bundle (rotation) overwrites the pinned fp cleanly.

## Concrete assertions
The mismatch test asserts the error **message names both fingerprints** and the remediation — "diagnosable" is the contract (AC-CT-ROT), so an assertion that merely "rejection occurred" is insufficient.
