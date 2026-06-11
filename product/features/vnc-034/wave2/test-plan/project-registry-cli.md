# Test Plan — ProjectRegistry + lifecycle CLI (register / list / delete)

> Component: `crates/unimatrix-server/src/projects.rs` *(new)*
> Source: FR-C3, FR-C4; AC-W2-R4; D3 (list MAY carry store-open status, NO network
> health). Risks: R-03 (shared allowlist), R-04 (per-slug isolated dirs), R-11
> (fail-loud provisioning).
> Locked refinements: **D4** (`delete` de-registers only; `--purge` destroys loudly;
> re-register RE-ATTACHES), **D5** (reserved-slug refusal at `register`), **D6**
> (`register` idempotence is two-state).
>
> `register` is **server-side and creates the store** (own DB, vector index, hash
> chain, analytics under `/data/.unimatrix/{slug}/`) — never client-auto-created
> (ADR-004). `list` and `delete` complete the lifecycle. Slug validation reuses the
> merged `ProjectSlug` newtype — NOT a second validator.

---

## Unit test expectations (cargo)

### A. register (FR-C3, FR-C4, AC-W2-R4)

- `test_register_creates_per_slug_store_dir` — `register("alpha")` over a temp data
  root creates `/{root}/.unimatrix/alpha/` with its OWN DB + vector index + hash
  chain + analytics artifacts. Assert the dir and the per-slug store files exist.
- `test_register_adds_slug_to_registry` — after `register("alpha")`, the registry/
  `[[projects]]` map contains `alpha` and resolves to that dir. Assert `list`
  includes it.
- `test_register_validates_slug_via_newtype` — `register("My_Project")` and
  `register("../etc")` REJECT with the SAME error the router/config give (delegates
  to `ProjectSlug::TryFrom`); no dir created. Cross-component parity with
  `projects-config.md` T-SEC corpus.
- `test_register_never_client_auto_created` — **structural**: assert the store-
  creation entrypoint is the server-side register path only; no client/attach code
  path constructs a per-slug store (ADR-004; cross-check AC-W1-C4 stays intact).

### A.2 register — D5 reserved-slug refusal (separate from charset)

The reserved set is **`v1`, `health`, `observe`, `tools`** (route-grammar segments).
This guard is **separate from and additional to** the D1 charset allowlist — a slug
can be charset-valid yet reserved. Mirror table in `projects-config.md`
(RESERVED-SLUG TEST TABLE).

- `test_register_rejects_reserved_tools_shadowing` — **THE critical shadowing test.**
  `register("tools")` → REJECT as reserved; **no dir created**, slug NOT added to
  routing. `/v1/tools/…` is the default-project alias (ADR-005); a slug named `tools`
  would shadow the default project entirely. `tools` is charset-valid, so a
  charset-only impl wrongly accepts it — this test turns that impl red.
- `test_register_rejects_reserved_route_segments` — `register("v1")`,
  `register("health")`, `register("observe")` → each REJECT as reserved; no dir
  created. Covers the remaining three reserved segments.
- `test_register_reserved_is_separate_from_charset` — **the discriminator.** Assert
  `ProjectSlug::try_from("tools")` is `Ok` (charset-valid) while `register("tools")`
  is `Err(reserved)`. Proves the reserved check is a distinct layer, not folded into
  the charset regex. A charset-only `register` would accept `tools` and fail this.
- `test_register_reserved_exact_match_only` — `register("toolsx")`,
  `register("v1-prod")`, `register("healthcheck")` succeed (only the four EXACT
  segments are reserved); guards against an over-broad `starts_with`/`contains`
  reserved check that would reject legitimate slugs.

### A.3 register — D6 two-state idempotence (interacts with D4)

`register` of an EXISTING slug has **two distinct outcomes** by state — do NOT
collapse them into one "already exists" message.

- `test_register_already_routing_errors_loud` — slug `alpha` is registered AND in the
  routing table → `register("alpha")` again → **loud error** (no silent re-register,
  no store mutation). Assert non-zero, actionable message, existing store/hash chain
  untouched. (D6 state 1.)
- `test_register_dir_exists_deregistered_reattaches` — slug `alpha`'s data dir exists
  on disk but `alpha` is **de-registered** (after a `delete`, D4) → `register("alpha")`
  → **success via re-attach**, NOT an error. Assert the slug returns to the routing
  table bound to the PRESERVED store (see D4 re-attach test in §C). (D6 state 2.)
- `test_register_two_states_distinct_messages` — assert the two outcomes above are
  **distinguishable** to the operator: the routing-collision case and the re-attach
  case do not emit the same generic "already exists" text. (D6: states must not be
  collapsed.)

### B. list (AC-W2-R4; D3 status field)

- `test_list_returns_registered_slugs` — after registering `alpha`, `beta`, `list`
  returns both (stable order). Assert exact set.
- `test_list_empty_when_none_registered` — fresh data root → `list` returns empty,
  not an error.
- `test_list_may_carry_store_open_status` — **D3 (operator-side only)**: IF the
  `list` output carries a per-slug store-open/health status field, assert it is
  computed locally (operator-side, in-process) — e.g. "open" vs "missing dir". This
  is allowed only because it is CLI-side. Assert the status reflects reality
  (register → "open"; delete dir out-of-band → "missing").
- `test_list_exposes_no_network_health` — **negative (D3)**: assert `list` does NOT
  open or advertise any network/HTTP health surface; the status is a local field on
  CLI stdout only. (Network per-slug health is split — see project-router
  `test_no_per_slug_health_endpoint`.)

### C. delete / --purge / re-attach — D4 (the integrity discriminators)

**D4 (locked):** the per-slug hash chain is sacred and unrollbackable. `delete` is
**de-register only** (data dir preserved); `--purge` is the ONLY destroy path and
MUST be **loud** (slug-name confirmation); de-register → re-register **RE-ATTACHES**
to the preserved chain, never clobbers it.

- `test_delete_deregisters_and_preserves_data_dir` — register `alpha`, write at least
  one knowledge entry → `delete("alpha")` → assert: (1) `alpha` is DROPPED from the
  registry/routing (`list` excludes it; router → `UnknownProject`), AND (2) the on-disk
  dir `/{root}/.unimatrix/alpha/` **and its files (DB, vector, hash chain, analytics)
  still EXIST**. `delete` must NOT touch disk. (D4 default = de-register, data safe.)
- `test_purge_requires_slug_confirmation_or_no_destroy` — **loud-destroy test.**
  `--purge alpha` **without** the slug-name confirmation (e.g. wrong/missing
  confirmation token) → MUST NOT destroy: assert the data dir is STILL present and a
  loud "confirmation required" error is returned (non-zero). Only `--purge alpha`
  **with** the matching slug-name confirmation removes the dir — assert the dir is
  then gone. (D4: destroy is opt-in, confirmed, and loud.)
- `test_purge_with_confirmation_removes_dir_and_deregisters` — `--purge alpha` with
  correct confirmation → dir removed AND slug de-registered AND router →
  `UnknownProject`. The full destroy path in one assertion.
- `test_deregister_reregister_reattaches_to_preserved_chain` — **the highest-value
  integrity test (D4 insist).** Sequence: `register("alpha")` → write ≥2 knowledge
  entries (capture the hash-chain head H1 and entry IDs) → `delete("alpha")`
  (de-register; dir preserved) → `register("alpha")` again. Assert the re-registered
  `alpha` **RE-ATTACHES to the preserved store**: (1) the prior entries are still
  present and readable, (2) the hash chain head is the SAME H1 (chain CONTINUED, not
  reset to genesis), (3) it is the SAME store dir — NOT a fresh empty store / new
  genesis chain. A fresh-store impl (new genesis, empty entries) MUST fail this test.
- `test_purge_then_register_is_fresh_store` — contrast/guard: after `--purge alpha`
  (confirmed, dir gone) → `register("alpha")` legitimately creates a FRESH store with
  a new genesis chain. Confirms purge truly severs the old chain (re-attach only
  applies to the de-register path, not the purge path).
- `test_delete_unregistered_slug_errors_loud` — `delete("ghost")` (never registered)
  → loud, actionable error, no panic, non-zero (R-11). No accidental dir traversal.
- `test_delete_validates_slug` — `delete("../etc")` / `--purge("../etc")` rejected by
  the newtype before any filesystem op; no path join from raw input (R-03 reuse).

### D. Lifecycle round-trip (AC-W2-R4)

- `test_register_list_delete_roundtrip` — register `alpha` → `list` shows it +
  store dir exists → `delete alpha` → `list` excludes it + slug unresolvable, **dir
  still on disk (D4 de-register)**. The AC-W2-R4 happy path.
- `test_register_delete_reregister_roundtrip` — full D4+D6 integrity loop: register →
  write → delete → re-register → assert re-attach (chain continued). The lifecycle
  view of `test_deregister_reregister_reattaches_to_preserved_chain`.

### E. Fail-loud provisioning (R-11, NFR-03)

- `test_register_on_unwritable_root_fails_loud` — `register` against an unwritable
  data root → actionable error, no panic, no `.unwrap()`, non-zero. Grep: no
  `.unwrap()` in the registry provisioning path.

---

## Integration test expectations

- The registry feeds `ProjectRouter` construction; the HTTP integration tests in
  `project-router.md` (`test_two_slugs_route_to_distinct_stores`, isolation) build
  their two-slug fixture by `register`-ing `alpha`/`beta`, proving register→route is
  end-to-end coherent.
- infra-001: not applicable to register/list/delete (no CLI-over-stdio harness path);
  registry lifecycle is unit + Rust-integration covered. Smoke remains the
  single-project backward-compat gate (OVERVIEW §4.1).

---

## Edge cases

- Register the same slug after a `delete` (de-register) — **RE-ATTACHES** to the
  preserved store/chain (D4), NOT a fresh store. After `--purge` (confirmed) — fresh
  store, new genesis. Covered by `test_deregister_reregister_reattaches_to_preserved_chain`
  and `test_purge_then_register_is_fresh_store`.
- `list` with a registered slug whose dir was deleted out-of-band → status reflects
  "missing" (D3 local status), `list` does not panic.
- Concurrent register of two distinct slugs — both succeed, isolated dirs.
- `--purge` confirmation typo (confirmation ≠ slug) → no destroy, loud error (the
  loud-destroy guard must not fire on near-misses).

## Notes
- Slug validation is the SAME `ProjectSlug` newtype across config / register / route
  — one allowlist, no drift (OVERVIEW §3). The **D5 reserved-slug** guard (§A.2) is a
  SEPARATE layer on top of the charset newtype, not a second charset validator.
- **D4 integrity invariant:** `delete` never touches disk; `--purge` is the only
  destroy and is loud (slug confirmation); de-register → re-register re-attaches the
  preserved hash chain. The chain is unrollbackable — destruction is never default.
- D3 boundary is load-bearing: `list` status is operator-side ONLY; NO over-the-wire
  per-slug health (that surface = the rejected ADR-004/OQ-B slug-listing leak).
