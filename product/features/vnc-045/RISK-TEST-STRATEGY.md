# Risk-Based Test Strategy: vnc-045 — `context_tag` (mechanism only)

> Source docs: SCOPE.md (SD-1..SD-12; scope REDUCED 2026-07-07 — `protected_tags` value-hygiene policy DEFERRED in full), ARCHITECTURE.md (active ADR-001/002/004/008/009; ADR-003/005/006/007 DEFERRED), SPECIFICATION.md (FR-01..11, NFR-01..07, AC-01..07), SCOPE-RISK-ASSESSMENT.md (SR-01..10).
> Historical evidence cited inline as `#id`. This document identifies what could fail on the SHIPPED mechanism and the scenarios that would detect it; the tester translates scenarios into implementations.
>
> **Scope-reduction note (binding).** This strategy targets ONLY the `context_tag` mechanism. The previous version's Critical R-01 (five-site per-slug `protected_tags` threading → `::default()`) and every config-plumbing / value-hygiene / cadence-guard / `min_trust_level` risk are **VOIDED-BY-DEFERRAL** — they moved to the future `protected_tags` feature and carry **no test requirement here** (see Scope Risk Traceability). No risk below requires a validator, allow-list, config type, or trust check that does not ship.
>
> **Priority = Severity × Likelihood.** Critical = High×High. High = High×Med or Med×High. There is **no Critical risk** in the reduced feature.
>
> **Test-seam constraint (#5468, binding).** The `context_tag` `#[tool]` handler is **not unit-constructible** — it needs a `RequestContext` only the live MCP transport supplies. Orchestration + audit proofs belong at the **`StoreTagService` + store-primitive + `audit_log` read-back** seams (directly constructible); end-to-end route/format proofs belong in the **Stage-3c integration** suite. Do NOT assert handler behavior by instantiating the `#[tool]` fn.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Mutation touches a forbidden surface — accidental routing through `update()`/`context_correct`, a stray learning-column write, a re-hash, an edge/id change, or a stale read from a surface that (wrongly) caches `entry_tags`. Silently reintroduces the exact churn the feature exists to prevent; the derived-state blast radius (ARCH §3, SR-01) must be complete. | High | Low | **High** |
| R-02 | `replace` not one atomic transaction — a crash/error between prior-removal (`DELETE ... LIKE 'namespace:%'`) and new-insert leaves the entry with ZERO `namespace:*` tags (lost status). Historical multi-write posture (#4420) is non-transactional — a live temptation against ADR-004; positive precedent is `insert_in_txn` (#267). Also: the colon-less/null-namespace tag must degrade to `add`, never hard-error. | High | Med | **High** |
| R-03 | Audit record incomplete or lost — `prior_value` null on remove/replace; metadata hits the `"{}"` sentinel (#5468); replace emits two events or zero; `Outcome`/enum serialized as integer not variant-string (#4366); `session_id` unset because it was read after `tokio::spawn` (#4389); fire-and-forget event dropped before read-back settle (#4377). Audit is the PRIMARY, retrofit-hard control (SD-7, ADR-009) — a gap here is the security gap, not cosmetic. | High | Med | **High** |
| R-04 | Value-opacity violated — a vocabulary/shape/allow-list check creeps in, the marked pre-write seam becomes a live `evaluate(tag)` call, or `validate_outcome_tags` is reused/conflated. `delivery:proven`, `delivery:anythingelse`, and free-form `foo` MUST all succeed on bare `Capability::Write`; NO `ProtectedTagsConfig`/allow-list type may ship (SD-8, ADR-008 pt 4). | Med | Low | **Low-Med** |
| R-05 | Lifecycle guards under/over-applied — free-form-on-deprecated wrongly REFUSED (over-restriction), or any-tag-on-quarantined wrongly ALLOWED. Guards live only on the `context_correct` path today (`write_ext.rs:471-482`) and MUST be re-implemented **in** `context_tag`, not inherited (FR-11). | Med | Med | **Med** |
| R-06 | Namespace derivation / tag-parse edge cases mishandled — the audit `namespace` (substring before first `:`, else `null`) is mis-derived, or the colon-less `replace` target is wrong. A wrong split corrupts audit reconstructability or scopes the replace `DELETE` incorrectly. Namespace is DERIVED and recorded, NEVER validated (value-opacity holds). | Med | Med | **Med** |
| R-07 | Live-control wiring missed — `check_write_rate` not called (op is unthrottled, diverging from `store_correct.rs:29`), or `'context_tag'` absent from the `audit_write_count_since` op-list (`audit.rs:84`), silently voiding the latent budget signal. | Med | Low | **Low-Med** |
| R-08 | `tag` / derived-namespace used unparameterized — SQL injection, or an over-broad `DELETE ... WHERE tag LIKE 'namespace:%'` when the derived namespace contains `%`/`_` LIKE metacharacters, removing unintended tags. See Security Risks. | Med | Low | **Low-Med** |

## Risk-to-Scenario Mapping

### R-01: Forbidden-surface mutation / derived-state blast radius (High)
**Severity**: High · **Likelihood**: Low · **Impact**: The entire feature rationale collapses — a tag change that zeroes the learning vector, re-hashes, or bumps the id is indistinguishable from `context_correct`. ARCH §3 asserts every `entry_tags` reader is live SQL and nothing caches tags (ADR-002); if that enumeration is incomplete, a mutation yields a stale read. Comprehensive coverage required despite low likelihood — this is the core-value guard.

**Test Scenarios** (store-primitive + service seam; handler not unit-constructible, #5468):
1. Seed an entry with non-zero `confidence`, `access_count`, `helpful_count`, `unhelpful_count`, `last_accessed_at`. Mutate a tag (add/remove/replace). Assert all five learning columns byte-identical pre/post (contrast: `context_correct` zeroes them — `write_ext.rs:542-561`). (FR-06, NFR-01, AC-02)
2. Assert `content_hash` and `previous_hash` identical pre/post; integrity oracle (`chain_verify.rs:152`) yields no `ContentHashMismatch`. (FR-04, NFR-02)
3. Assert entry `id` unchanged, no supersession version minted, full edge set byte-identical pre/post — behavioral proof `update()`/`correct()` were NOT called. (FR-03/FR-04, NFR-03, AC-01)
4. **Read-freshness / no stale window** (ADR-002, NFR-04): issue a tag-filtered query immediately after `add` → entry present; after `remove` → entry absent; no tick-rebuild lag, no invalidation step. Proves the "all tag reads are live SQL, nothing caches tags" blast-radius claim (ARCH §3) for the read-path filter (`graph_read_filter.rs:186-209`) and canonical hydration (`load_tags_for_entries`, `read.rs:111-150`).

**Coverage Requirement**: Invariance asserted across learning vector, hash chain, edges, and entry id for at least one add, one remove, one replace. Read-freshness proven for add and remove. Comprehensive despite low likelihood.

### R-02: Non-atomic replace loses status; colon-less degrade (High)
**Severity**: High · **Likelihood**: Med · **Impact**: An entry left with zero `namespace:*` tags after an interrupted replace = lost status with no recovery signal. ADR-004 mandates ONE transaction (prior-removal + insert); the historical multi-write posture (#4420) is non-transactional and is a live temptation; `insert_in_txn` (#267) is the positive precedent to mirror.

**Test Scenarios** (store-primitive seam):
1. **Namespaced replace**: entry holds `delivery:partial`; call `replace` with `delivery:proven`. Assert exactly one `delivery:*` tag remains (`proven`), the prior is gone, in one observable step (`replace_tag` returns the evicted prior).
2. **Atomicity / rollback**: `replace_tag` runs `DELETE ... LIKE 'namespace:%'` + `INSERT new` in a single SQLite transaction; inject a forced INSERT failure and assert the DELETE rolls back (prior survives) — never a zero-tag window. (FR-05, NFR-05)
3. **Colon-less / null-namespace replace degrades to add** (ADR-004 edge case, ARCH §4.3): `replace` with a tag having no `:` performs a pure insert (no prior removed, never hard-errors on a valid tag); audit records `{action:"replace", namespace:null, prior_value:null, new_value:tag}`. Assert no existing tags are removed.
4. **Replace with no prior in the namespace**: `replace_tag` returns `None` prior → pure insert; audit `prior_value:null`, still one event.
5. Replace emits ONE audit event carrying prior+new together, not one per DELETE/INSERT half (cross-links R-03).

**Coverage Requirement**: Store-level transactionality proving DELETE+INSERT roll back together; behavioral proof a replace never yields a zero-value window; both the namespaced path AND the colon-less degrade-to-add path covered. NFR-05 measurable target: 0 observable zero-value intermediate states.

### R-03: Audit incompleteness — the primary, retrofit-hard control fails silently (High)
**Severity**: High · **Likelihood**: Med · **Impact**: SD-7/ADR-009 make audit the substitute for identity-based prevention AND the one genuinely retrofit-hard piece (append-only; a future `protected_tags` must add ZERO fields). A missing `prior_value`, a `"{}"` sentinel, a lost event, a double event, or a mis-serialized enum destroys reconstructability — the only accountability the model has.

**Test Scenarios** (`audit_log` read-back seam):
1. **`prior_value` mandatory**: after every `remove` and every `replace` (with a prior), assert `metadata.prior_value` is present and non-null. After a plain `add` with no prior, assert `prior_value` is null/absent and `new_value` carries the tag. (FR-10, AC-04)
2. **`{}` sentinel trap** (#5468): assert the emitted `metadata` is a well-formed `{action, namespace, tag, prior_value, new_value}` JSON object and is **never `"{}"`** on a real mutation. On a serialize error the code must skip the event (warn), not emit the sentinel.
3. **Exactly one event per mutation**: a replace emits ONE `operation="context_tag"` record (not one per DELETE/INSERT half); add/remove each emit one.
4. **Namespace derivation in metadata**: `namespace` = substring before first `:` for `delivery:x` (= `delivery`), `null` for a colon-less tag — recorded, never validated (cross-links R-06).
5. **Serde forms**: `Outcome`/enum fields serialize as variant strings, not integers (#4366) — read-back assertions match on `"Success"`, not `0`. `session_id` is captured from `ctx.audit_ctx.session_id` **before** `tokio::spawn`, not filled by `::default()` after (#4388/#4389).
6. **Fire-and-forget timing** (#4377/#5468): the audit write is fire-and-forget (`log_event_async` via `tokio::spawn`); read-back tests need a settle delay before querying `audit_log`, else a false-negative "no audit" flake.
7. Field completeness: `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used="write"`, `timestamp` all present. (FR-10)

**Coverage Requirement**: Every action variant (add, remove, replace, colon-less degrade) has an audit read-back assertion covering full field set + `prior_value` rule + non-`{}` metadata + single-event count + variant-string serde. Fire-and-forget settle handled so audit tests are deterministic.

### R-04: Value-opacity violated / accidental validator (Low-Med)
**Severity**: Med · **Likelihood**: Low · **Impact**: A vocabulary/allow-list/shape check creeping in — or the marked seam becoming a live `evaluate(tag)` call — silently converts the fast path into the deferred hygiene policy, breaking the north-star property (SD-8) and pre-building inert surface the human explicitly deferred.

**Test Scenarios** (service seam + static/review assertions):
1. Table test: (a) `delivery:proven` accepted; (b) `delivery:anythingelse` accepted (no rejection path exists); (c) free-form `foo` accepted; (d) same `Write` agent throughout — no identity/trust difference changes the outcome. (FR-07, AC-05)
2. **No validator shipped**: grep/review confirms no `ProtectedTagsConfig`, allow-list, `evaluate_protected_tag`, or vocabulary type was introduced; the pre-write seam is a marked comment/note only — no stub, no config, no call.
3. **Not conflated with `validate_outcome_tags`**: assert `context_tag` does NOT invoke `validate_outcome_tags` (`tools.rs:895-898`); a tag equal to a reserved outcome key is written as-is (the two vocabularies are independent). `validate_outcome_tags` behavior on the `context_store` path is unchanged (no regression).

**Coverage Requirement**: Three-value acceptance table + static proof that no hygiene/config type shipped and the outcome-tag validator is not reused. **Do NOT write a test that requires a rejection path** — none ships.

### R-05: Lifecycle guards under/over-applied (Med)
**Severity**: Med · **Likelihood**: Med · **Impact**: Over-restriction blocks legitimate free-form annotation of deprecated entries; under-restriction lets mutations onto quarantined entries the `context_correct` path would refuse. Guards are re-implemented in-op (FR-11), not inherited.

**Test Scenarios** (service/store seam):
1. Quarantined entry → ANY `context_tag` (add, remove, replace; any tag) refused with a lifecycle `invalid_params`. (AC-07)
2. Deprecated entry + arbitrary/free-form tag → **ALLOWED** and written (the easy-to-miss case; over-restriction fails here — no "refuse protected tag on deprecated" rule ships, it is deferred). (AC-07, SD-12)
3. Active entry → all valid mutations proceed (guard does not over-fire on normal state).

**Coverage Requirement**: Quarantine-refusal (across all three actions) + deprecated-allow + active-control, proven at the service/store seam.

### R-06: Namespace derivation / tag-parse edge cases (Med)
**Severity**: Med · **Likelihood**: Med · **Impact**: A wrong prefix split corrupts the audit `namespace` (harming per-namespace reconstructability) or mis-scopes the replace `DELETE`. Namespace is derived-and-recorded, never validated.

**Test Scenarios** (table test against the derivation helper + service seam):
1. `delivery:proven` → namespace `delivery`. Byte-PREFIX before first `:`.
2. Tag exactly equal to a prefix-with-colon (`"delivery:"`, empty value) → namespace `delivery`, tag stored as-is (value-opaque; no rejection).
3. Colon-less tag (`"reviewed"`) → namespace `null`; `add` audits `namespace:null`; `replace` degrades to add (cross-links R-02 #3).
4. Value containing the delimiter (`"delivery:proven:extra"`) → namespace is the substring before the **first** `:` (= `delivery`); assert deterministic; full tag stored verbatim.
5. Prefix appears mid-string not at start (`"x-delivery:proven"`) → namespace `x-delivery` (first-colon rule), not `delivery`; documents the derivation is positional, not vocabulary-aware.
6. Empty / whitespace-only tag → `invalid_params` validation error (malformed tag), not a silent write. (ARCH §4.4)

**Coverage Requirement**: Boundary table covering namespaced, colon-terminated, colon-less, multi-colon, mid-string-colon, and empty tags; first-colon positional semantics proven; every case's audit `namespace` asserted.

### R-07: Live-control wiring missed (Low-Med)
**Severity**: Med · **Likelihood**: Low · **Impact**: An unthrottled op diverges from `store_correct`'s posture; a missing op-list entry silently voids the latent SLN1 budget signal.

**Test Scenarios**:
1. `check_write_rate` fires: exceed the per-caller limit → op throttled with `ServiceError::RateLimited` (route/service seam, respecting the `UdsSession` exemption, `gateway.rs:60/166`). (FR-08, AC-06a)
2. `audit_write_count_since` includes `context_tag`: after N `context_tag` mutations, assert the persistent counter reflects them (`'context_tag'` in the op-list, `audit.rs:84`) — latent signal, NOT a live throttle. (FR-09, AC-06b)

**Coverage Requirement**: Throttle enforcement + op-list inclusion, each proven behaviorally at a reachable seam.

### R-08: Injection / over-broad DELETE (Low-Med) — see Security Risks.

## Integration Risks

- **Handler non-constructibility (#5468)** — the highest-leverage constraint. The `#[tool]` `context_tag` fn is NOT constructible in unit scope (no `RequestContext`). Orchestration + audit behavior must be proven at reachable seams: real `StoreTagService` (`services/store_tag.rs`) + real store primitives (`add_tag`/`remove_tag`/`replace_tag`) + `audit_log` read-back over `make_server()`, with end-to-end route/format proofs in the Stage-3c integration suite. Plan for this seam split — do not assume handler unit tests.
- **Replace atomicity ↔ single audit event (R-02×R-03)** — one store transaction MUST correspond to exactly one audit row; a mismatch (two audit rows for a replace, or a committed tag with no audit) is an integration-seam bug.
- **`entry_tags` FK `ON DELETE CASCADE`** (nxs-008 #360) — deleting the parent entry cascades tags; confirm `context_tag` on a since-deleted entry surfaces a clean error (`CoreError::Store`), not a partial write.
- **Service wiring parity with `store_correct`** — `StoreTagService::tag` must sequence identity/gate (handler) → `check_write_rate` (service) → store write → fire-and-forget audit exactly as `store_correct.rs:29/98-102`; a reordering (e.g. audit before commit, or throttle after write) is a seam defect.

## Edge Cases

- Add a tag that already exists (idempotent INSERT vs duplicate-key error — assert the defined behavior against the `entry_tags` PK/unique constraint).
- Remove a tag that is absent (no-op vs error — assert defined behavior; audit `prior_value` handling when the tag didn't exist).
- Replace when NO prior tag exists in the derived namespace (`replace_tag` returns `None`) — pure insert, audit `prior_value:null`, `action` still `replace` (R-02 #4).
- Colon-less replace (degrade-to-add), tag equal to `"namespace:"` (empty value), multi-colon value, empty tag (R-06).
- Concurrent mutations on the same `(entry, namespace)` — two racing replaces; last-writer-wins at the DB, but assert no interleaving yields two `namespace:*` tags (atomic tx, R-02).
- Rate-limit boundary values (exactly N, N+1) for `check_write_rate` (R-07).
- Quarantined/deprecated/non-existent `id` (cross-links R-05).

## Security Risks

**Untrusted input surface**: `id` (i64), `action` (string), `tag` (string), `agent_id` (string, audit-only). All arrive over MCP from a `Capability::Write` holder.

- **R-08 — Injection / over-broad DELETE**: `tag` is written directly to `entry_tags`. All INSERT/DELETE MUST use bound parameters (no string interpolation). The replace path `DELETE ... WHERE tag LIKE 'namespace:%'` uses a **derived** namespace (substring of the caller's `tag` before first `:`); if that namespace contains SQL `LIKE` metacharacters (`%`, `_`), the delete over-matches and removes unintended tags. **Test**: (a) a `tag` containing SQL metacharacters/quotes is stored and matched literally — no injection; (b) a `tag` whose derived namespace contains `%`/`_` either is rejected as malformed or is LIKE-escaped so the replace deletes only true `namespace:` matches, never siblings. **Blast radius**: bounded to one entry's `entry_tags` rows (per-slug DB, vnc-034 structural isolation) — no cross-entry, no cross-project reach.
- **Authorization blast radius**: `context_tag` adds NO privilege over `context_correct` (SD-10); a `Write` holder could already mutate any tag. Confirm the op gates on `Capability::Write` only, mints no `Capability::Tag`, does not split add/remove/replace at the capability layer, and consults neither `agent_id` nor `TrustLevel` for authorization (FR-02, SD-9). `agent_id` is self-declared → attribution is declarative; the audit trail (R-03), not identity, is the control.
- **The two preserved seams are seams only — no behavior to test**: the value-opacity pre-write interception point (ADR-008 pt 4) and the `Capability::Write` gate LOCATION (ADR-008 pt 1-2) are marked code/ADR notes for a future `protected_tags`. vnc-045 ships **no** validator and **no** trust check — do NOT write a test requiring an `evaluate(tag)` rejection path or a `min_trust_level` accept/reject difference. Their correctness is covered negatively by R-04 (no validator shipped) and by the authorization-blast-radius assertion above (no trust consulted).
- **Cross-project isolation (A3)**: inert under 1-client:1-project + per-slug DB (vnc-034 structural isolation, ARCH §9). Tags are never a cross-project access-control filter. Carry-forward guard: if cross-project retrieval is introduced, tags-as-ranking must not become tags-as-access-control (flag for a spike) — documented, not closed.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| Missing `Capability::Write` | `require_cap` error; no write; no audit |
| Unknown `action` / malformed/empty `tag` | `rmcp::ErrorData::invalid_params`; no write; no audit |
| Quarantined entry (any action) | `invalid_params` lifecycle refusal; no write |
| `check_write_rate` exceeded | `ServiceError::RateLimited`; no write |
| Store write failure mid-replace | full rollback — prior value survives, never a zero-value entry (R-02) |
| Colon-less replace | degrade to `add` (pure insert); never hard-errors on a valid tag (R-02 #3) |
| Audit serialize error | warn + SKIP the event (never emit `"{}"` sentinel, #5468); the mutation still succeeded — accept the rare audit gap over a corrupt record |
| Audit write (fire-and-forget) drop | mutation succeeds; audit is best-effort-async — accountability degraded but write is not blocked (matches `store_correct` posture) |

Rejections before the write (authz, lifecycle, malformed tag, throttle) MUST leave NO trace (no audit, no partial tag). Failures after commit MUST NOT roll back the successful tag mutation. Tagging a **deprecated** entry is NOT an error (SD-12).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (derived-state blast radius; tags-outside-hash invariant) | R-01 | **LIVE.** ARCH §3 enumerates every `entry_tags` reader — all live SQL, nothing caches tags (ADR-002). Resolved; verified via invariance + read-freshness scenarios (R-01). |
| SR-02 (non-atomic replace loses status) | R-02 | **LIVE.** ADR-004: replace is ONE transaction, ONE audit event. Addressed — R-02 tests atomicity + rollback + colon-less degrade. |
| SR-03 (allow-list reads as rigor it lacks; evidence-binding) | — | **VOIDED-BY-DEFERRAL.** No allow-list ships; the op is explicitly value-opaque (any value succeeds) — there is no false rigor to mis-read. Evidence-binding remains the evaluating agent's job (Non-Goal 4). Not tested. |
| SR-04 (hygiene fast-path-only; `context_correct` bypass) | — | **VOIDED-BY-DEFERRAL.** "Bypass" presupposes a hygiene policy; none ships. `context_correct` is unchanged and `context_tag` grants no privilege (SD-10), but there is no allow-list to bypass. Not tested. |
| SR-05 (carry-forward spikes; tenant-isolation A3 assumption) | Security §, A3 | **LIVE (documented carry-forward).** A3 confirmed inert-and-safe at HEAD (structural per-slug DB, ARCH §9). Left OPEN as a guard — flag if cross-project retrieval is introduced. |
| SR-06 (five coupled per-slug config sites; daemon divergence) | — | **VOIDED-BY-DEFERRAL.** Five-site per-slug `protected_tags` threading moved to the future feature. Nothing to build or test in vnc-045 (the largest surface and the `::default()`-rots risk — gone). |
| SR-07 (hygiene conflated with `validate_outcome_tags`) | R-04 (residual) | **VOIDED-BY-DEFERRAL** for the validator itself. Residual sliver retained: R-04 asserts `context_tag` does NOT invoke `validate_outcome_tags` and no validator shipped (value-opacity). |
| SR-08 (cadence-guard novel state model) | — | **VOIDED-BY-DEFERRAL.** Per-`(entry, namespace)` cadence guard deferred (ADR-007). `check_write_rate` (R-07) is the only rate control that ships. Not tested. |
| SR-09 (`merge_configs` inherits undeclared allow-list values) | — | **VOIDED-BY-DEFERRAL.** No config type, no `merge_configs` arm ships (ADR-006 deferred). Not tested. |
| SR-10 (uni-capability skill / config drift) | — | **VOIDED-BY-DEFERRAL.** No `delivery:` config ships; `delivery:` is a value-opaque example only. The skill depends on no vnc-045 config. Not tested. |
| — (audit completeness; retrofit-hard) | R-03 | **LIVE, architecture-introduced.** Not from an SR — ADR-009/SD-7 introduce the complete generic audit shape as the primary control. Addressed comprehensively via R-03. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 0 | — (the prior Critical R-01 threading risk is VOIDED-BY-DEFERRAL) |
| High | 3 (R-01, R-02, R-03) | ~16 (invariance×3-4 + read-freshness; atomicity/rollback + colon-less degrade + one-event; audit completeness×4 actions + sentinel + serde + timing) — comprehensive |
| Med | 2 (R-05, R-06) | ~9 (lifecycle 3 combos; namespace-derivation boundary table ×6) — targeted |
| Low-Med | 3 (R-04, R-07, R-08) | ~8 (value-opacity acceptance ×3 + no-validator static proof; throttle + op-list; injection + LIKE-escape) — basic + security-critical for R-08 |

## Residual Risks (architecture leaves open)

1. **Declarative attribution (SD-9)** — `agent_id` is self-declared (`credential_type="none"`); audit accountability rests on an un-authenticated identity. Real in OSS by construction; closed only under enterprise per-agent OIDC/OAuth. Documented, accepted.
2. **Audit best-effort delivery** — fire-and-forget audit write can drop (process death between commit and spawn completion) without blocking the mutation. The primary control is best-effort, not transactional with the write. Matches existing `store_correct` posture; a stronger guarantee would require a transactional audit sink (out of scope).
3. **Cross-project tenant isolation (A3)** — inert ONLY while no cross-project retrieval exists. Carry-forward guard: reintroduce a spike if cross-project retrieval is ever added, so tags-as-ranking never becomes tags-as-access-control.
4. **Two preserved retrofit seams are inert by design (SD-8, SD-9)** — the value-opacity interception point and the `Write`-gate trust-elevation location ship as marked notes with no behavior. Correct by construction (nothing to enforce); their value materializes only when the deferred `protected_tags` feature is built. No test obligation now.

## Knowledge Stewardship
- Queried: `context_search` for atomic-replace/partial-write posture, audit `{}`-sentinel + fire-and-forget settle + handler non-constructibility, and `Outcome` serde form. Findings: #4420 (partial-write posture historically NON-transactional — a live temptation against ADR-004, elevated R-02 likelihood), #267/#92 (`insert_in_txn` / correction-chain atomicity — positive precedent to mirror for R-02), #5468 (audit `"{}"` sentinel + fire-and-forget settle-delay + `#[tool]` handler NOT unit-constructible — shaped R-01/R-03 test-seam guidance), #4366 (`Outcome` serde variant-string not integer — R-03 assertion form), #4388/#4389 (capture `session_id` before `tokio::spawn` — R-03 field completeness), #4377 (audit fire-and-forget async test strategy — R-03 settle). The prior version's threading-gap evidence (#3216/#5269/#5427) is now VOIDED with the deferred per-slug config surface.
- Stored: nothing novel. The recurring patterns this feature instantiates (audit `{}` sentinel; handler non-constructibility test-seam; DELETE+INSERT-as-one-tx) are already captured (#5468, #267). No cross-feature risk pattern surfaced that isn't already recorded; per stewardship rules I did not store feature-specific risks.
