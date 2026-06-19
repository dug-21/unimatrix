# Test Plan: `UnimatrixServer::new` (additive `Option<ServiceLayer>`)

> Component: `crates/unimatrix-server/src/server.rs:281-386` (defaults `306-333`).
> Change (ADR-001): append final param `services: Option<ServiceLayer>`; `Some(s)` ⇒ use `s`;
> `None` ⇒ existing test-default body (NLI off, pool 1, default params, empty allowlist).
> Risks: **R-06** (constructor regression / cloud-only branch). ACs: **AC-6**. FR-7, FR-8, NFR-6.

This is the riskiest edit to existing code (R-06): a broken `None` arm breaks unit tests; divergent
`Some`/`None` parity logic creates a cloud-only path the local install never exercises (violates the
one-isolation-seam invariant, NFR-5 / vnc-034 ADR-003).

---

## Unit test expectations

### AC-6.1 — test-default path preserved byte-for-byte
- `test_server_new_none_yields_test_defaults`
  - **Arrange:** construct via `UnimatrixServer::new(..., None)` with the existing test inputs.
  - **Act:** inspect the resulting `ServiceLayer`'s config-visible state.
  - **Assert:** NLI disabled, rayon pool effective size 1, `InferenceConfig == default`,
    `ConfidenceParams == default`, `CategoryAllowlist` empty — i.e. exactly the prior
    `server.rs:306-333` behavior. The assertion is on observable state, not "it compiled."

### AC-6.2 — existing call sites compile & pass unchanged
- All pre-crt-056 `UnimatrixServer::new` unit-test call sites compile after appending `, None`
  (additive). **Assert (by the suite passing):** no behavioral change in any existing
  `server.rs`/`services` unit test. This is the regression guard — the whole existing suite is the
  assertion. (`cargo test --workspace` green is the gate.)
  - *Flag (per memory: file-scope agents must flag adjacent breakage):* enumerate every existing
    `UnimatrixServer::new` caller (incl. `http_provision.rs`, `main.rs`, `test_support.rs`, and all
    `tests/*.rs`); each must get `, None` or `Some(layer)`. A missed caller is a compile break that
    the field-scope implementer MUST flag, not silently leave.

### AC-6.3 — `Some(s)` arm uses the supplied layer
- `test_server_new_some_uses_supplied_service_layer`
  - **Arrange:** build a config-driven `ServiceLayer` (NLI on, non-default `ConfidenceParams`).
  - **Act:** `UnimatrixServer::new(..., Some(layer))`.
  - **Assert:** the server's `ServiceLayer` is the supplied one (NLI on, non-default params) — the
    `Some` arm does **not** rebuild or fall back to defaults.

---

## Source audit expectation (R-06, one isolation seam — AC-6 same-path proof)

- **No `if cloud { ... } else { ... }` parity branch.** Audit the constructor body: the only
  divergence between arms is `Some` (use supplied) vs `None` (build test default). Parity *logic*
  (how a config-driven layer is built) lives in `build_project_server`, not duplicated per arm.
- **Same-path proof:** assert (structurally, in `daemon-http-boot.md`) that the single-project
  daemon ALSO constructs via `Some(config-driven)`. The `None` arm is reachable **only** from unit
  tests. No production path reaches `None`.

---

## Edge cases / failure modes

- `None` with otherwise-valid inputs ⇒ valid server, test-default behavior (AC-6.1).
- A field-scope implementer changing the `None`-arm body to anything other than the byte-for-byte
  prior body is a regression — AC-6.1 is the detector.

## Coverage requirement

AC-6 = (unchanged existing unit suite passes) + (AC-6.1 explicit default-behavior assertion) +
(same-path structural proof, `daemon-http-boot.md`). A green compile alone is **not** sufficient —
the default-behavior assertion must be explicit.
