# vnc-045 — SPECIFICATION: `context_tag` (mechanism only)

> Source: `product/features/vnc-045/SCOPE.md` (scope REDUCED by human 2026-07-07 — `protected_tags` DEFERRED in full; SD-1..SD-12 binding).
> Research inputs: `product/research/ass-093/FINDINGS.md` (mechanism), `product/research/ass-094/FINDINGS.md` (authorization + anti-poison — informs the retrofit-hard seams kept here and the deferred policy).
>
> This specification translates SCOPE into testable requirements. Where SCOPE and this spec conflict, SCOPE governs. Settled Decisions (SD-1..SD-12) are binding and are NOT re-opened here. **`protected_tags` (value-hygiene policy, per-slug config, `min_trust_level`, cadence guard) is out of scope** — see §11.

---

## 1. Objective

Add a new MCP operation `context_tag(id, action, tag)` — a lightweight, audited, **parallel fast path** for mutating an entry's generic tag lane (`entry_tags`) in place, tag-only, preserving the entry's content hash, edges, embedding, and full learning vector. The op is **value-opaque**: the engine stores and mutates a tag it never interprets — `delivery:proven`, `delivery:anything`, and free-form `reviewed` all succeed on bare `Capability::Write`. `delivery:` is merely an illustrative example of a tag the op can mutate; **no protected-tag vocabulary or policy ships in vnc-045.**

---

## 2. Ubiquitous Language (Domain Terms)

Downstream agents MUST use these terms with these exact meanings.

| Term | Definition |
|------|-----------|
| **Fast path** | `context_tag` — the parallel, tag-only, learning-preserving route to mutate `entry_tags`. Contrast: the **heavy path** (`context_correct`), which rewrites the record, mints a supersession version, re-points edges, and zeroes the learning vector. |
| **Lockdown** | An access-control boundary that *prevents* an authorized writer from changing a tag. `context_tag` is explicitly **NOT** a lockdown (SD-10). It adds no privilege and closes no hole; `context_correct` can still change any tag. |
| **Value-opacity** | The engine writes the supplied `tag` **without interpreting its value** — no allow-list, no vocabulary, no shape check. Any string succeeds on `Capability::Write`. This is the north-star property and the location of one preserved retrofit seam (§11, SD-8). |
| **Audit-as-primary-control** | Accountability, not prevention, is the security model. Every mutation writes a dedicated `operation="context_tag"` audit record sufficient to reconstruct what changed, when, and by whom (SD-7). The append-only audit event is the one genuinely retrofit-hard piece and is specced in full NOW. |
| **Learning vector** | The entry's self-learning columns: `confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`. `context_correct` hard-resets these to zero; `context_tag` MUST NOT touch them. |
| **Namespace (audit-derived)** | The substring of `tag` before the first `:` (else `null`). It is **derived and recorded in the audit metadata, NEVER validated** — value-opacity holds. It exists so audit records are queryable per namespace and so a future `protected_tags` reuses the shape with zero additions. |
| **Replace (first-class action)** | `action="replace"` — an atomic, one-transaction mutation that removes the prior tag sharing the target's derived namespace and inserts the new tag. It is a real action at the op layer (SD-5), NOT a config-selected behavior; a future per-prefix `single_value` config would merely make `replace` the default for a prefix. |
| **The two retrofit seams** | (1) the value-opacity pre-write interception point where a future `evaluate(tag) -> Allowed \| Rejected` hygiene validator drops in (SD-8); (2) the `Capability::Write` gate LOCATION where a future enterprise trust-elevation check attaches (SD-9). Both are marked with a code/ADR note; **neither ships a stub, config type, or field now.** |

---

## 3. Domain Models

### 3.1 `context_tag` operation input

```
context_tag(
  id:       integer,                        // target entry id (integer, never quoted)
  action:   "add" | "remove" | "replace",   // mutation verb
  tag:      string,                         // opaque tag text, e.g. "delivery:proven" or "reviewed"
  agent_id: string (optional),              // self-declared; AUDIT-ONLY, never an authorization input
  format:   string (optional)               // response format
)
```

- `action` has exactly three variants: `add`, `remove`, `replace`. There is **no** add-vs-remove-vs-replace split at the capability layer — all three gate identically on `Capability::Write` (SD-9).
- `tag` is an opaque string to the engine. The engine assigns it no domain meaning and performs no value validation (SD-8).
- `replace` derives its target namespace from `tag` (substring before the first `:`); it removes the prior tag in that namespace and inserts `tag` atomically (§4 FR-05). A `tag` with no `:` has a `null` namespace; `replace` semantics for such a tag are defined by the architect but MUST remain atomic and MUST record `prior_value`.

### 3.2 Audit record (`AuditEvent`) — complete generic shape, shipped now

Reuses the existing `AuditEvent` (no schema change; `metadata: String` JSON already accommodates the shape — `schema.rs:360/408`). A `context_tag` mutation emits:

```
operation:       "context_tag"
target_ids:      [id]
agent_id:        <self-declared caller id>
capability_used: "write"
timestamp:       <mutation time>
metadata (JSON): { action, namespace, tag, prior_value, new_value }
```

- `namespace` = substring of `tag` before the first `:`, else `null`. **Derived and recorded, NEVER validated** (SD-7, value-opacity).
- `prior_value` is **mandatory and non-null** on every `remove` and every `replace`. For a plain `add` with no prior, `prior_value` is null/absent and `new_value` carries the added tag.
- The record MUST be sufficient to reconstruct what changed, when, and by whom. **This shape is retrofit-hard (append-only) and is specced complete NOW even though nothing enforces tag hygiene yet — a future `protected_tags` MUST add zero fields to it.**

---

## 4. Functional Requirements

Each is testable. IDs map to Acceptance Criteria in §7.

- **FR-01 — Registered MCP op.** `context_tag` MUST be registered as an MCP tool (a `#[tool(name="context_tag")]` fn + `Parameters` struct inside the existing `#[rmcp::tool_router]` block), following the `context_correct` wiring template (`tools.rs:1226-1323`): build context via `build_context_with_external_identity(...)`, then gate, throttle, write, audit. The `Parameters` struct MUST include `agent_id: Option<String>` (per convention #1301 — omitting it silently drops attribution). `action` accepts `add | remove | replace`.

- **FR-02 — Capability gate (the trust seam LOCATION).** The op MUST gate on `Capability::Write` via the existing `require_cap` path (`server.rs:631` → `infra/registry.rs:92`) — the identical baseline `context_correct` requires (`tools.rs:1245`). It MUST NOT mint a `Capability::Tag`, MUST NOT split add/remove/replace at the capability layer, and MUST NOT consult `TrustLevel`/`agent_id`/evidence for authorization (SD-9). `agent_id` is audit-only. The gate LOCATION is the preserved enterprise trust-elevation seam (§11); nothing beyond `Write` is wired now.

- **FR-03 — Direct `entry_tags` write, no `update()`, no `context_correct`.** The mutation MUST write `entry_tags` directly via single-row `INSERT` (add) / `DELETE` (remove), mirroring `write.rs:161/168`. It MUST NOT invoke the 24-column `update()` primitive (which rewrites `content_hash`/`previous_hash` — `write.rs:97/115`) and MUST NOT invoke `context_correct` (SD-2).

- **FR-04 — Content/edge/embedding invariance.** A mutation MUST leave `content_hash`, `previous_hash`, all edges, and the embedding unchanged (tags are outside the content hash — `hash.rs:7-16`; outside embedding input — `embed/src/text.rs:10`). No supersession version is minted; the entry `id` is unchanged (SD-1/SD-3).

- **FR-05 — `replace` is first-class and atomic.** `action="replace"` MUST, in **one transaction**, remove the prior tag sharing the target's derived namespace and insert the new `tag` (SD-5). A partial failure MUST leave the entry's tags unchanged (no observable intermediate state with the prior removed but the new not yet inserted). `replace` is NOT decomposed into two client calls and NOT routed through `update()`.

- **FR-06 — Learning-vector preservation.** A mutation MUST NOT modify any learning column (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`). (Contrast: `context_correct` zeroes all five — `write_ext.rs:542-561`.)

- **FR-07 — Value-opacity (the hygiene-validator seam).** The op MUST write any `tag` value without interpreting it — `delivery:proven`, `delivery:anythingelse`, and free-form `foo` all succeed on bare `Capability::Write`. There MUST be **no** allow-list, vocabulary, or shape check. Exactly **one** pre-write interception point MUST be marked (ADR + code note: "hygiene validator intercepts here") as the future value-hygiene seam. **NO stub, NO empty `ProtectedTagsConfig`, NO config type is introduced** (SD-8). The op MUST NOT reuse or conflate with `validate_outcome_tags` (`tools.rs:895-898`).

- **FR-08 — `check_write_rate` throttle.** The op MUST call `gateway.check_write_rate` (the only live throttle — `gateway.rs:166`, per-`CallerId`, in-memory, resets on restart, exempts `UdsSession`), subject to the same per-caller limit as store/correct (`store_ops.rs:114` / `store_correct.rs:29`).

- **FR-09 — `audit_write_count_since` inclusion.** `'context_tag'` MUST be added to the `audit_write_count_since` op-list (`audit.rs:84`). This is a **latent** signal (no live enforcement consumer today); the requirement future-proofs the SLN1 budget and does NOT imply live throttling.

- **FR-10 — Dedicated audit event (complete generic shape).** Every successful mutation MUST emit exactly one `AuditEvent` per §3.2 with `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used`, `timestamp`, and `metadata={action, namespace, tag, prior_value, new_value}`. `namespace` is derived from the tag prefix and is recorded but never validated. `prior_value` MUST be present and non-null on every `remove` and every `replace`. A `replace` emits **one** event carrying prior + new together, not two.

- **FR-11 — Lifecycle guards in the op.** These guards live only on the `context_correct` path today (`write_ext.rs:471-482`) and MUST be re-implemented **in** `context_tag` (they are not inherited):
  - A mutation on a **quarantined** entry MUST be refused (any tag; add, remove, or replace).
  - A tag on a **deprecated** entry MUST be **allowed** (free-form; the "refuse protected tag on deprecated" rule is a protected-tag concept and is deferred with `protected_tags`).

---

## 5. Non-Functional Requirements

- **NFR-01 — No learning-column mutation.** Verifiable by asserting all five learning columns are byte-identical before/after a mutation (measurable target: 0 columns changed). (FR-06)
- **NFR-02 — No content-hash change.** `content_hash` and `previous_hash` identical before/after (measurable: 0 change; integrity oracle `chain_verify.rs:152` yields no `ContentHashMismatch`). (FR-04)
- **NFR-03 — No `update()` / `context_correct` reuse.** No supersession version minted; entry `id` unchanged; edge set unchanged. (FR-03/FR-04)
- **NFR-04 — Read-freshness (no stale window).** Because tag-filtered reads load tags live from `entry_tags` on every query (`graph_read_filter.rs:186-209`), the mutated tag MUST be visible to the **next** tag-filtered read with no tick-rebuild/stale-index lag and no invalidation step. Measurable: a tag-filtered query issued immediately after the mutation reflects it (add present / remove absent). (SD-4)
- **NFR-05 — Atomicity of replace.** A `replace` MUST be one transaction; no observable intermediate state with the prior value removed and the new value not yet inserted. (FR-05)
- **NFR-06 — No schema / DB migration.** No hash change, no first-class `status` column, no `entry_metadata` column, no `AuditEvent` column change, no DB migration. Every control reuses an existing primitive (SD-3).
- **NFR-07 — Rust workspace rules.** File-size limits respected; extend existing fixtures/helpers (test infra is cumulative); `Grep`/`Glob` not Bash.

---

## 6. User / Agent Workflows

1. **Volatile status flip (worked example, `delivery:`).** An evaluating agent (uni-capability skill / vision-guardian) that already holds `Capability::Write` calls `context_tag(id, "replace", "delivery:proven")`. The prior `delivery:partial` (same derived namespace `delivery`) is atomically removed and `delivery:proven` inserted in one transaction; one audit record logs `{action:"replace", namespace:"delivery", tag:"delivery:proven", prior_value:"delivery:partial", new_value:"delivery:proven"}`. The entry's learning vector, content hash, and edges are untouched. Evidence-binding (`proven` requires real behavioral proof) and any `proven_by` content write remain the **evaluating agent's** responsibility via a separate `context_correct` — NOT the platform's (Non-Goals 4/5). `delivery:` is only an example; the engine never interprets the value.
2. **Free-form annotation.** An agent calls `context_tag(id, "add", "reviewed")`. The engine writes it with no value check; audit logs `namespace:null`.
3. **Value-opaque success (no vocabulary).** `context_tag(id, "add", "delivery:anythingelse")` succeeds on bare `Capability::Write` — there is no allow-list to reject it; hygiene is deferred.
4. **Tag removal.** `context_tag(id, "remove", "delivery:proven")` → removes the tag; audit record carries mandatory `prior_value`. Removal on a deprecated entry is allowed.
5. **Quarantine refusal.** `context_tag` on a quarantined entry is refused for any action.

---

## 7. Acceptance Criteria (with verification methods)

Each AC-ID traces from SCOPE (AC-01..AC-07). Verification method stated for each.

**Test-seam constraint (#5468).** The `context_tag` `#[tool]` handler is **not unit-constructible** — it needs a `RequestContext` that only the live MCP transport supplies. Orchestration and audit proofs therefore belong at the **`StoreTagService` + store-primitive + `audit_log` read-back** seams (directly constructible), with end-to-end **route/format** proofs in integration tests. Do not assert handler behavior by attempting to instantiate the `#[tool]` fn.

- **AC-01 — Registered fast-path op, direct write, no heavy path.**
  `context_tag(id, action, tag)` with `action ∈ {add, remove, replace}` exists as a registered MCP op gated on `Capability::Write`, writes `entry_tags` directly (single-row INSERT/DELETE), and never invokes `update()` or `context_correct`. A mutation leaves `content_hash`/`previous_hash` and all edges unchanged.
  *Verify:* integration — assert the tool is registered/callable and an agent lacking `Write` is rejected (route seam). Service/primitive seam — assert `content_hash`, `previous_hash`, and the full edge set are byte-identical pre/post; assert no new entry id and no supersession version (behavioral proof `update()`/`correct()` were not called). Maps FR-01..FR-04.

- **AC-02 — Learning-vector preservation + read-freshness.**
  After a mutation the five learning columns (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`) are unchanged (contrast: `context_correct` zeroes them); the mutated tag is visible to the next tag-filtered read with no invalidation step.
  *Verify:* seed an entry with non-zero `confidence`/`access_count`/`helpful_count`; mutate a tag at the service seam; assert all five columns unchanged; issue a tag-filtered query immediately after and assert the entry is (add) / is not (remove) in the result set. Maps FR-06, NFR-01, NFR-04.

- **AC-03 — `replace` atomic in one transaction.**
  `replace` removes the prior value + adds the new in one transaction; a partial failure leaves the entry's tags unchanged.
  *Verify:* store-primitive seam — `add delivery:partial`, then `replace delivery:proven`; assert exactly one `delivery:*` tag remains (`proven`) and the prior is gone; assert no observable intermediate zero/partial state (single transaction); inject a mid-transaction failure and assert the tag set is unchanged from before the replace. Maps FR-05, NFR-05.

- **AC-04 — Audit: complete generic shape (retrofit-hard).**
  Every mutation emits `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used`, `timestamp`, and `metadata={action, namespace, tag, prior_value, new_value}`. `namespace` is derived from the tag prefix (substring before first `:`, else `null`) and is **recorded but never validated**. `prior_value` is present and non-null on every remove/replace. The record is sufficient to reconstruct what changed, when, and by whom, and a future `protected_tags` adds no field to it.
  *Verify:* `audit_log` read-back seam — after add / remove / replace, query the audit log; assert the event exists with all fields; assert `namespace` derives correctly for `delivery:x` (=`delivery`) and for a colon-less tag (=`null`); assert `prior_value` non-null on remove and replace; assert exactly one event per mutation (one for a replace). End-to-end integration confirms the route emits the same shape. Maps FR-10.

- **AC-05 — Value-opacity (no vocabulary, one marked seam).**
  The handler writes any tag value without interpreting it — `delivery:proven`, `delivery:anythingelse`, and free-form `foo` all succeed on bare `Capability::Write`. There is no allow-list. Exactly one pre-write point is marked (ADR + code note) as the future hygiene-validator seam; no stub or config type is introduced.
  *Verify:* table test at the service seam — (a) `delivery:proven` accepted; (b) `delivery:anythingelse` accepted (no rejection path exists); (c) free-form `foo` accepted; (d) same `Write` agent throughout, no `TrustLevel` difference changes the outcome. Static assertion / review — the marked seam exists as a single interception point and `validate_outcome_tags` is not invoked; grep confirms no `ProtectedTagsConfig` / allow-list type shipped. Maps FR-07.

- **AC-06 — Rate + budget controls.**
  The op calls `check_write_rate` (subject to the live per-caller limit) and is counted by `audit_write_count_since` (`'context_tag'` added to its op-list).
  *Verify:* (a) exceed the per-caller `check_write_rate` limit → op throttled (route/service seam, respecting `UdsSession` exemption); (b) assert `audit_write_count_since` includes `context_tag` events (audit read-back). Maps FR-08, FR-09.

- **AC-07 — Lifecycle guards.**
  `context_tag` on a quarantined entry is refused; tagging a deprecated entry is allowed (no protected-tag concept ships).
  *Verify:* service seam — (a) quarantine an entry → any `context_tag` (add/remove/replace) refused; (b) deprecate an entry → a free-form / arbitrary tag is allowed and written. Maps FR-11.

---

## 8. Constraints

- Tags are outside the content hash (`hash.rs:7-16`) — mutation MUST NOT touch `content_hash`/`previous_hash` (SD-1/SD-3).
- MUST NOT reuse `update()` (`write.rs:97`) — write `entry_tags` directly (SD-2).
- No schema change (SD-3); no DB migration; the audit `metadata` JSON already accommodates the SD-7 shape (no `AuditEvent` column change).
- Authorization is `Capability::Write` only; `TrustLevel` is **NOT** touched/activated; `agent_id` is audit-only, never an authz input (SD-9).
- `check_write_rate` is the only live throttle (resets on restart, exempts `UdsSession`); `audit_write_count_since` is a latent, non-enforcing signal (SD-11).
- Attribution is declarative (`credential_type="none"`, `agent_id` self-declared) — accountability rests on the audit trail (SD-7), not identity prevention. This bound is documented, not closed.
- `context_correct` is unchanged; `context_tag` grants no new privilege (SD-10).
- The value-opaque pre-write seam MUST NOT conflate with `validate_outcome_tags` reserved-outcome-vocabulary logic (SD-8).
- Rust workspace rules: file-size limits; extend existing fixtures/helpers; `Grep`/`Glob` not Bash.

### Explicit Non-Goals (do NOT introduce)

- **NO `protected_tags` value-hygiene policy** — no allow-list, no vocabulary validation, no `ProtectedTagsConfig` type, no `single_value` config, no per-slug threading, no `PerSlugOverlayable` classification, no `min_trust_level`, no cadence guard (all deferred — §11).
- **`context_tag` is NOT an access-control boundary / lockdown on tags** (SD-10); `context_correct` (unchanged) can still change any tag.
- **NO identity-based / elevated-trust authorization**; **NO anti-self-attestation control**; `agent_id` is audit-only (SD-9, ass-094 R1/R2).
- **NO op-level/platform evidence enforcement** — the platform does not verify `delivery:proven` is backed by proof (Non-Goal 4).
- **NO content mutation** in `context_tag` — tags only; `proven_by`/`delivered_by` content stays a separate `context_correct` owned by the evaluating agent (Non-Goal 5).
- **NO modification of `context_correct`** (SD-10).
- **NO hard-coded tag vocabulary**; no `Capability::Tag`; no add/remove/replace capability split (SD-9).
- **NO `entry_metadata` column, NO first-class `status` column, NO status-as-edge** (SD-3, ass-093 Q4).
- **NO activation of `audit_write_count_since` as a live throttle** (future-proofing only).

---

## 9. Dependencies

- **Research:** ass-093 FINDINGS (#926, mechanism); ass-094 FINDINGS + INTERNAL/EXTERNAL (#927, authorization + anti-poison — informs the retrofit-hard seams kept: audit shape SD-7, gate location SD-9, and the deferred policy).
- **Worked-example consumer:** uni-capability skill (`SKILL.md:38,83,86,89-90`) + pattern #5505 — the `delivery:` tag, STATUS-vs-CONTENT axis. `context_tag` does NOT depend on this vocabulary (value-opaque); `delivery:` is only an example.
- **Prior features:** vnc-014 (`AuditEvent` provenance fields — the SD-7 shape lands in its `metadata`); nxs-008 #360 (`entry_tags` FK `ON DELETE CASCADE`).
- **Confirmed integration points at HEAD:** `hash.rs:7-16`; `chain_verify.rs:152`; `embed/src/text.rs:10`; `write.rs:78/97/115/161/168`; `write_ext.rs:471-482/542-561/613`; `graph_read_filter.rs:186-209`; `gateway.rs:60/166`; `store_ops.rs:114`; `store_correct.rs:29`; `audit.rs:79/84`; `schema.rs:236/360/408`; `infra/registry.rs:92`; `server.rs:631`; `tools.rs:611/884/895-898/1226/1230/1245/1323`.
- Crates: `unimatrix-store` (write/audit/hash/schema), `unimatrix-server` (tools/registry/gateway).

---

## 10. Open Questions

**None block the spec.** One item left as a **design-time decision** for the architect (not an open scope question):
- **`replace` for a colon-less (null-namespace) tag** — define the removal target when no namespace can be derived (e.g. no-op prior-removal, or refuse). MUST remain atomic and MUST record `prior_value` when a prior exists. Not a scope question; the three actions and value-opacity are settled.

---

## 11. Future Extension (out of scope) — `protected_tags` value-hygiene policy

Everything below is **deferred in full** and carries **no requirements or test obligations in vnc-045**. It retrofits cleanly onto the two seams preserved here at the same cost as building it now — which is why it is deferred (pre-building inert config plumbing that nothing consumes cannot be behaviorally tested and rots). The deferred surface: `ProtectedTagsConfig` type (`{prefix, allowed_values, single_value, min_trust_level, ...}`), five-site per-slug threading (`merge_configs`, `validate_config`, `PER_SLUG_CONFIG_CLASSIFICATION`, server-state snapshot, `build_project_server`), the value-hygiene allow-list validator, the `single_value` config, `min_trust_level` (enterprise), and the per-`(entry, namespace)` cadence guard.

**Two preserved retrofit seams (the reason they are the only forward contracts):**
1. **Value-opacity interception point (SD-8).** The single marked pre-write point (FR-07) where a future `evaluate(tag) -> Allowed | Rejected` hygiene validator drops in. Marked with an ADR + code note; ships as a comment/marker only — no stub, no config.
2. **`Capability::Write` gate LOCATION (SD-9).** The existing `require_cap` gate (FR-02) is where a future enterprise trust-elevation check (`min_trust_level`) attaches. The gate LOCATION is the retrofit-hard contract; there is nothing to wire now and no gap, because the field's owning config does not yet exist.

The audit event shape (§3.2, FR-10) is the third retrofit-hard piece, but it is **shipped complete now** (not deferred) because the audit log is append-only — a future `protected_tags` must add zero audit surface.

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task-specific) — surfaced #4451/#4450 (prior `TrustLevel`-gated write precedent — explicitly NOT activated here per SD-9), #317 (MCP handler identity/context ceremony via `build_context_with_external_identity` → FR-01), #92/#4420 (partial-write blast-radius: infra error not rolled back — informs FR-05 atomicity framing), #5217 (per-slug config classification / `merge_configs` — belongs to the DEFERRED `protected_tags`, not vnc-045). Applied as cited.
- Stored: nothing. Read-only tier — spec decisions are feature-specific; no generalizable pattern surfaced that isn't already captured. The retro can promote any interpretation (e.g. the "ship the append-only audit shape, defer the cheap-retrofit policy" split) if it generalizes.
