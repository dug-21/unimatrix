# vnc-045 — `context_tag`: Domain-Agnostic In-Place Tag Mutation (mechanism only)

> Status: SCOPING — scope reduced by human 2026-07-07. **vnc-045 ships the `context_tag` MECHANISM only.** The `protected_tags` value-hygiene policy is DEFERRED to a clearly-marked future extension (see "Deferred / Future Extension"). DELIVERY of research spikes **ass-093** (#926, mechanism) and **ass-094** (#927, authorization + anti-poison — informs the deferred policy and the retrofit-hard seams kept here). Tracks GH #928.
>
> **This is a domain-agnostic platform feature.** `context_tag` is a lightweight, audited, **parallel fast path** for mutating the generic tag lane in place — tag-only, preserving the entry's learning vector, **value-opaque** (the engine writes a tag it never interprets). **`delivery:` is now merely an example of a tag `context_tag` can mutate — there is no protected-tag policy shipping in vnc-045.**

## Problem Statement

A domain-agnostic knowledge platform needs a way to mutate **volatile per-entry tags in place** — cheaply, with a clean audit trail — without dragging the change through the heavy content-correction path. Today the only MCP route that can change an entry's tags is `context_correct`, which is built for **content** corrections and on every call rewrites the record, mints a supersession version, re-points edges, and **hard-resets the entry's entire learning vector to zero** (`confidence=0.0, access_count=0, last_accessed_at=0, helpful_count=0, unhelpful_count=0` — `write_ext.rs:542-561`).

**Core rationale (the axis distinction).** A volatile status tag changes on a high frequency; the entry's content does not. Routing a tag change through the content-hash / `context_correct` path therefore destroys the entry's accumulated self-learning history on every change — churn the self-learning vision warns against. Tags are **outside the content hash** (`compute_content_hash` hashes only title+content — `hash.rs:7-16`), so an in-place tag mutate is integrity-safe by construction and touches none of the learning columns. `delivery:*` (capability delivery status; uni-capability `SKILL.md:38,83,86,89-90`, pattern #5505) is the archetypal such tag — but only an example; the engine stores and mutates a tag it never interprets.

**A parallel fast path, not a lockdown.** `context_tag` is a *parallel* route to `context_correct`, which is left **unchanged**. Anyone who can mutate content can already mutate any tag through `context_correct` — so `context_tag` adds no new privilege and closes no hole. Its purpose is to make the frequent, volatile case cheap (tag-only, learning-vector-preserving), not to lock tags down.

**Audit is the primary control, and the one genuinely retrofit-hard piece.** The audit log is append-only; reshaping a record later is the painful kind of change. So the **complete, generic audit event shape is specced in full NOW**, even though nothing enforces tag hygiene yet — a future `protected_tags` feature must add **zero** audit surface. Every mutation writes `operation="context_tag"`, `metadata {action, namespace, tag, prior_value, new_value}` + `agent_id` + timestamp; `prior_value` mandatory on remove/replace.

**Why defer `protected_tags` (human's reasoning — retrofit-hard vs. retrofit-cheap).** "Architect for enterprise" applies only to the **retrofit-HARD class**: contracts baked into immutable/append-only substrate — schema shape, the hash chain, the tag-lane-outside-content-hash property (already true), the **authorization gate LOCATION**, and the **audit-event SHAPE**. `protected_tags` is additive config + a validator module reading a tag the engine already stores → it **retrofits cleanly at the same cost later**, so there is no reason to pre-build it. Pre-building inert config plumbing is strictly **worse** than deferring: inert threading has nothing consuming it, so it cannot be behaviorally tested and it rots (the unthreaded `::default()` trap sits undetected until someone finally builds the feature). Build the threading with the feature that gives it teeth.

## Settled Decisions (binding)

**Mechanism — from ass-093 (intact):**
- **SD-1 — In-place tag mutate on the generic tag lane.** New MCP op `context_tag(id, action, tag)`, `action ∈ {add, remove, replace}`, writing `entry_tags` directly. NOT status-as-edge, NOT a new metadata table, NOT an entry-level metadata column. Tags are outside the content hash (`hash.rs:7-16`). (ADR-001)
- **SD-2 — Do NOT reuse the 24-column `update()` primitive** (`write.rs:97`, rewrites `content_hash`/`previous_hash` at `write.rs:115`). Write `entry_tags` directly via single-row `INSERT`/`DELETE` (mirror `write.rs:161`/`168`). (ADR-001)
- **SD-3 — No schema change.** No hash change, no first-class `status` column on `EntryRecord`, no `entry_metadata` column.
- **SD-4 — Live reads, no in-memory invalidation.** Tags read live from `entry_tags` on every query (`graph_read_filter.rs:186-209`); the mutate needs no cache invalidation and opens no stale-index window. (ADR-002)
- **SD-5 — `replace` is a first-class action, atomic in one transaction** (remove prior + add new). This is why a future per-prefix `single_value` config is NOT a new action — it just makes `replace` the default for that prefix. (ADR-004)
- **SD-6 — Transition history lives in the audit log, not the content chain** (ass-093 Q5). Content-chain version loss for a volatile tag is acceptable.

**Retrofit-hard seams kept in scope (the reason to build them now):**
- **SD-7 — Complete, generic audit event shape, specced in full NOW.** `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used`, timestamp, `metadata {action, namespace, tag, prior_value, new_value}`; **`prior_value` mandatory on remove/replace**. The `namespace` field is **derived from the tag prefix** (substring before the first `:`, else `null`) and is **recorded, NEVER validated** (value-opacity). Audit is append-only → this is the one genuinely retrofit-hard piece; a future `protected_tags` must add **zero** audit surface. (ADR-003)
- **SD-8 — Value-opacity at the write path (north star + the extension seam).** The handler writes the tag **without interpreting its value** — no allow-list, no vocabulary, `delivery:anything` and free-form tags alike succeed on `Write`. There is exactly one obvious pre-write point where a future `evaluate(tag) -> Allowed | Rejected` hygiene check drops in; mark it with a single ADR/code note ("hygiene validator intercepts here"). **NO stub, NO empty `ProtectedTagsConfig`.**
- **SD-9 — Authorization gate LOCATION is the trust seam — reuse `Capability::Write`.** Same baseline `context_correct` requires (`tools.rs:1245`). The retrofit-hard contract is the **gate location**, not a trust field: Enterprise later adds a trust-level check **at that same gate**. `min_trust_level` is a field on a `ProtectedTagsConfig` that does not exist until `protected_tags` is built — so there is **nothing to wire now and no gap**. The only forward-note: "trust elevation attaches at the existing `Write` gate." `agent_id` is **audit-only, never an authorization input** (self-declared).
- **SD-10 — `context_tag` grants no new privilege; `context_correct` is unchanged.** The op is a fast path, not an access-control boundary on tags.

**Live controls kept:**
- **SD-11 — Wire `check_write_rate`** (`gateway.rs:166`, the only live throttle) and **add `'context_tag'` to the `audit_write_count_since` op-list** (`audit.rs:84`, a cheap latent signal — not a live throttle).
- **SD-12 — Lifecycle: refuse tagging a QUARANTINED entry** (generic mechanism hygiene; guard lives only on the `context_correct` path today, `write_ext.rs:471-482`). Free-form tags **are allowed** on deprecated entries (the "refuse protected tag on deprecated" rule is a protected-tag concept and is deferred).

## Goals

1. Add MCP op **`context_tag(id, action, tag)`**, `action ∈ {add, remove, replace}`, gated on `Capability::Write` — a lightweight, audited, in-place single-tag mutate on the non-hashed tag lane, so any domain can change volatile tags **without** `context_correct` and without zeroing the learning vector, re-hashing, re-embedding, or re-pointing edges. (SD-1..SD-6, SD-9, SD-10)
2. **Preserve the learning signal and read-freshness** — touch no learning column; live reads, no invalidation (SD-4).
3. **Spec and emit the complete generic audit event** (retrofit-hard) — the shape a future `protected_tags` reuses with zero additions (SD-7).
4. **Value-opaque write path** with a single marked hygiene-validator seam — no stub, no config (SD-8).
5. **Reuse the `Write` gate** as the trust seam location; no `min_trust_level` now (SD-9).
6. **Live controls** — `check_write_rate` + `'context_tag'` in `audit_write_count_since` (SD-11); quarantine refusal (SD-12).

## Non-Goals

1. **`protected_tags` value-hygiene policy** — deferred in full (see Deferred / Future Extension). No allow-list, no vocabulary validation, no config type ships in vnc-045.
2. **`context_tag` is NOT an access-control boundary on tags.** It is a fast path; `context_correct` (unchanged) can still change any tag (SD-10).
3. **Identity-based / elevated-trust authorization** — authorization is `Capability::Write` only; `agent_id` is audit-only. No `min_trust_level` (it belongs to the deferred config), no anti-self-attestation (SD-9).
4. **Platform evidence enforcement** — the platform does NOT verify that a value like `delivery:proven` is backed by proof; evidence-binding is the evaluating agent's responsibility (uni-capability skill / vision-guardian gate), not the engine.
5. **Content mutation in `context_tag`** — tags only. A `proven` transition that also needs to write proof content (`proven_by`/`delivered_by`) remains a **separate `context_correct`, owned by the evaluating agent** (`proven_by` is immutable CONTENT criteria; `delivery:` is volatile STATUS — different axes; `SKILL.md:89-90`, pattern #5505).
6. **Modifying `context_correct`** — left entirely unchanged (SD-10).
7. **Schema changes** — no `EntryRecord.status` column, no `entry_metadata` column (ass-093 Q4 (c) rejected); no status-as-edge (Q4 (b) rejected).
8. **Per-`(entry, namespace)` cadence guard** — deferred (it is namespace-scoped, a `protected_tags` concept). `check_write_rate` is the rate limit that ships.
9. **Credentialed/authenticated identity transport** — `credential_type` stays `"none"`, `agent_id` self-declared; the declarative-attribution bound is documented, not closed.

## Deferred / Future Extension — `protected_tags` value-hygiene policy

Everything below is **out of scope for vnc-045** and carries **no test requirements here**. It retrofits cleanly onto the seams kept above (audit shape SD-7, value-opaque seam SD-8, `Write`-gate location SD-9) at the same cost as building it now — which is why it is deferred:

- **`protected_tags` config type** (`Vec<ProtectedTagRule>` of `{prefix, allowed_values, single_value, ...}`).
- **Five-site per-slug threading** (new nested `UnimatrixConfig` type, `merge_configs` arm, `validate_config` extension, `PER_SLUG_CONFIG_CLASSIFICATION` entry, `UnimatrixServer` snapshot + `build_project_server` threading) — including the daemon-vs-per-slug-serving-path divergence.
- **Value-hygiene allow-list validator module** (rejecting typo'd values like `delivery:provn`) — drops into the marked SD-8 seam.
- **`single_value` CONFIG** — makes `replace` the default for a prefix. (The `replace` **action** itself ships now, SD-5.)
- **`min_trust_level`** — the enterprise trust-elevation check at the existing `Write` gate (SD-9).
- **`PerSlugOverlayable` classification** so separate slugs on one instance carry different tag policies.

## Background Research (grounded integration points)

**Mechanism (ass-093):** Content hash covers only title+content (`hash.rs:7-16`); integrity oracle recomputes title/content only (`chain_verify.rs:152`); no re-embed (embedding input is title+content, `embed/src/text.rs:10`). Tags live in `entry_tags` (ADR-006; FK `ON DELETE CASCADE`, nxs-008 #360), written independently: INSERT `write.rs:78/168`, `write_ext.rs:613`; DELETE-then-reINSERT `write.rs:161/168`. `update()` (`write.rs:97`) rewrites all 24 columns incl. `content_hash`/`previous_hash` (`write.rs:115`) — must not be reused (SD-2). Live tag reads: `graph_read_filter.rs:186-209` (SD-4).

**Authorization + audit surfaces (ass-094):** every mutating op gates on `Capability::Write` (`tools.rs:884` store, `tools.rs:1245` correct) via `require_cap` (`server.rs:631`) → `require_capability` (server crate `infra/registry.rs:92`, checks only `capabilities.contains(&cap)`). `TrustLevel` (`schema.rs:236`) is stored but never consulted — and is **not touched** by vnc-045 (SD-9). `AuditEvent` (`schema.rs:360`) has `metadata: String` JSON (default `"{}"`, `:408`), append-only, no hash chain; `Outcome` enum `schema.rs:333`. Live limiter `check_write_rate` (`gateway.rs:166`, 60/3600s per `CallerId`, in-memory, `UdsSession`-exempt `gateway.rs:60`), enforced at `store_ops.rs:114`/`store_correct.rs:29`. `audit_write_count_since` (`audit.rs:79`) counts only `('context_store','context_correct')` (`audit.rs:84`). Value-opacity precedent / interception point: `validate_outcome_tags` runs at `tools.rs:895-898` — but vnc-045 adds **no** validator there (SD-8).

**Wiring template (context_correct):** handler `tools.rs:1230`, registered `#[tool(name="context_correct")]` `tools.rs:1226` inside `#[rmcp::tool_router]` block `tools.rs:611`; identity via `build_context_with_external_identity(...)` `tools.rs:1236` (yields `ctx.agent_id`/`ctx.audit_ctx`/`ctx.caller_id`); gate `tools.rs:1245`; store call `store_ops.correct(...)` `tools.rs:1323` → service `store_correct.rs:20`, which runs `check_write_rate` at `:29` then writes+audits. **New op adds:** a `#[tool]` fn + `Parameters` struct in the router block, ctx build, `Write` gate, direct `entry_tags` write (add/remove/replace), the marked value-opaque pre-write seam, `check_write_rate`, audit event.

## Acceptance Criteria

- **AC-01:** `context_tag(id, action, tag)` with `action ∈ {add, remove, replace}` exists as a registered MCP op gated on `Capability::Write`, writes `entry_tags` directly (single-row INSERT/DELETE), and never invokes `update()` or `context_correct`. A mutation leaves `content_hash`/`previous_hash` and all edges unchanged.
- **AC-02:** After a `context_tag` mutation the entry's learning columns (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`) are unchanged (contrast: `context_correct` zeroes them). The mutated tag is visible to the next tag-filtered read with no invalidation step.
- **AC-03:** `replace` is atomic in one transaction (prior value removed + new value added); a partial failure leaves the entry's tags unchanged.
- **AC-04 (audit — complete generic shape, retrofit-hard):** Every mutation emits `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used`, timestamp, and `metadata {action, namespace, tag, prior_value, new_value}`. `namespace` is derived from the tag prefix (substring before the first `:`, else `null`) and is **recorded but never validated**. `prior_value` is present on every remove/replace. The record is sufficient to reconstruct what changed, when, and by whom, and a future `protected_tags` adds no field to it.
- **AC-05 (value-opacity):** The handler writes any tag value without interpreting it — `delivery:proven`, `delivery:anythingelse`, and a free-form `foo` all succeed on bare `Capability::Write`. There is no allow-list. Exactly one pre-write point is marked (ADR/code note) as the future hygiene-validator seam; no stub or config type is introduced.
- **AC-06:** `context_tag` calls `check_write_rate` (subject to the live per-caller limit) and is counted by `audit_write_count_since` (`'context_tag'` added to its op-list).
- **AC-07:** `context_tag` on a **quarantined** entry is refused; tagging a **deprecated** entry is allowed (no protected-tag concept ships).

## Constraints

- Tags are outside the content hash (`hash.rs:7-16`) — mutation MUST NOT touch `content_hash`/`previous_hash` (SD-1/SD-3).
- MUST NOT reuse `update()` (`write.rs:97`) — write `entry_tags` directly (SD-2).
- No schema change (SD-3). The audit `metadata` JSON already accommodates the SD-7 shape (no `AuditEvent` column change).
- Authorization is `Capability::Write` only; `TrustLevel` is **not** touched; `agent_id` is audit-only (SD-9).
- `check_write_rate` is the only live throttle (`gateway.rs:166`, resets on restart, exempts `UdsSession`); `audit_write_count_since` is a latent (non-enforcing) signal.
- Attribution is declarative (`credential_type="none"`, `agent_id` self-declared) — accountability rests on the audit trail (SD-7), not identity prevention.
- `context_correct` is unchanged; `context_tag` grants no new privilege (SD-10).
- Rust workspace rules: file-size limits; extend existing fixtures/helpers; Grep/Glob not Bash.

## Scope Risks Voided by This Deferral

- **Five-site per-slug config threading + daemon-vs-per-slug divergence** (was the largest surface and the `::default()`-rots risk) — moved to the future `protected_tags` feature.
- **The per-slug config-plumbing finding** (new nested type, `merge_configs` arm, `validate_config`, build-enforced `PER_SLUG_CONFIG_CLASSIFICATION` entry, server-state threading) — moved to the future feature; nothing to build or test in vnc-045.
- **Activating `TrustLevel` / `min_trust_level` gate logic** — no gap now (the field's owning config does not exist); the gate LOCATION is the only forward contract, already satisfied by reusing the `Write` gate.
- **Namespace-scoped cadence guard** — deferred with the `protected_tags` concept.

## Open Questions

**None.** All prior questions are resolved: `protected_tags` (authorization strength, vocabulary, config classification, single_value) is deferred wholesale; authorization is `Capability::Write`; the audit shape and value-opaque seam are the only forward contracts and are fully specified.

## Dependencies

- **ass-093 FINDINGS** (`product/research/ass-093/FINDINGS.md`, #926) — mechanism. **ass-094 FINDINGS** (`product/research/ass-094/FINDINGS.md` + `-INTERNAL`/`-EXTERNAL`, #927) — authorization + anti-poison; informs the retrofit-hard seams kept (audit shape, gate location) and the deferred policy.
- **uni-capability skill** (`.claude/skills/uni-capability/SKILL.md:38,83,86,89-90`) + **pattern #5505** — the worked-example consumer (`delivery:` tag), STATUS-vs-CONTENT axis. `context_tag` does not depend on this vocabulary (value-opaque).
- **vnc-014** — `AuditEvent` provenance fields (the SD-7 shape lands in its `metadata`). **nxs-008 #360** — `entry_tags` FK. (**vnc-040 Feature A / #799** per-slug config and **vnc-034** per-slug DB are dependencies of the DEFERRED `protected_tags` feature, not vnc-045.)
- **Confirmed integration points at HEAD:** `hash.rs:7-16`; `write.rs:78/97/115/161/168`; `write_ext.rs:471-482/613`; `graph_read_filter.rs:186-209`; `gateway.rs:60/166`; `store_ops.rs:114`; `store_correct.rs:29`; `audit.rs:79/84`; `schema.rs:236/263/317/333/360`; `infra/registry.rs:92`; `tools.rs:611/884/895-898/1226/1230/1245/1323`.

## Tracking

- GH Issue: **#928** (link to vnc-045 after Session 1). Research inputs: #926 (ass-093), #927 (ass-094).
- **Future feature (deferred):** `protected_tags` value-hygiene policy — config type + five-site per-slug threading + allow-list validator (at the SD-8 seam) + `single_value` config + `min_trust_level` (enterprise, at the SD-9 gate) + `PerSlugOverlayable` classification. Retrofits onto vnc-045's audit shape and value-opaque seam with zero changes to them.
- Carry-forward spikes (out of scope): `context_correct` learning-vector reset on all content corrections (ass-093 OoS); `audit_write_count_since` dormancy / SLN1 persistent-budget wiring (ass-094 OoS); metadata-filter-bypass as a tenant-isolation risk if cross-project retrieval is ever introduced (ass-094 OoS — inert under 1-client:1-project).
