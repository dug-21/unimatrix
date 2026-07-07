# vnc-045 — Implementation Brief: `context_tag` (mechanism only)

> Coordination artifact for Session 2 delivery. This brief **routes and summarizes**; the load-bearing technical decisions live in the source documents below. Where this brief and a source document conflict, the source governs (SCOPE > SPECIFICATION > ARCHITECTURE/ADRs).
>
> **Scope REDUCED by human 2026-07-07.** vnc-045 ships the **`context_tag` MECHANISM only**. The `protected_tags` value-hygiene policy (config type, per-slug threading, allow-list validator, `single_value` config, `min_trust_level`, cadence guard) is **DEFERRED in full** — see "Deferred to Future `protected_tags` Feature". Vision alignment: PASS 7 / WARN 0 / VARIANCE 0.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-045/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-045/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-045/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-045/architecture/ARCHITECTURE.md |
| Risk-Test Strategy | product/features/vnc-045/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-045/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-045/ACCEPTANCE-MAP.md |

### ADRs

Active ADRs carry the "why" for the shipped mechanism. ADR-003/005/006/007 are **DEFERRED** with `protected_tags` (kept on disk, marked, deprecated in Unimatrix) — reference them only under "future extension", never as delivery decisions.

| ADR | Title | Status | File |
|-----|-------|--------|------|
| ADR-001 | `context_tag` writes `entry_tags` directly via a new single-row primitive, not `update()` | Active | architecture/ADR-001-direct-entry-tags-write.md |
| ADR-002 | No in-memory invalidation — `entry_tags` is read strictly live | Active | architecture/ADR-002-no-inmemory-invalidation.md |
| ADR-004 | `replace` is a first-class action, atomic in one transaction, counting as one audit event | Active (revised) | architecture/ADR-004-atomic-single-value-replace.md |
| ADR-008 | Authorization posture — `Capability::Write` gate IS the trust seam; `agent_id` audit-only; value-opacity is the hygiene seam | Active (revised) | architecture/ADR-008-authorization-posture-and-seams.md |
| ADR-009 | The complete, generic `context_tag` audit-event shape is a retrofit-hard contract shipped in full now | Active (new) | architecture/ADR-009-audit-event-contract.md |
| ADR-003 | `protected_tags` value-hygiene dedicated check | **DEFERRED** | architecture/ADR-003-value-hygiene-separate-check.md |
| ADR-005 | `protected_tags` per-slug threading | **DEFERRED** | architecture/ADR-005-per-slug-threading-all-paths.md |
| ADR-006 | `merge_configs` replaces the `protected_tags` rules list | **DEFERRED** | architecture/ADR-006-merge-configs-replace-not-merge.md |
| ADR-007 | Cadence guard state model | **DEFERRED** | architecture/ADR-007-cadence-guard-state-model.md |

## Goal

Add MCP op `context_tag(id, action, tag)`, `action ∈ {add, remove, replace}`, gated on `Capability::Write` — a lightweight, audited, in-place single-tag mutate on the non-hashed `entry_tags` lane — so any domain can change volatile tags **without** `context_correct` and **without** zeroing the entry's learning vector, re-hashing, re-embedding, or re-pointing edges. It is a **parallel fast path**, not a lockdown: `context_correct` is unchanged and `context_tag` grants no new privilege. The op is **value-opaque** — the engine writes a tag it never interprets; `delivery:` is merely an illustrative example, not a shipped vocabulary or policy.

## Component Map

Pseudocode and test-plan files produced in Stage 3a (2026-07-07). Components drawn from ARCHITECTURE §2; all files below exist. Stage 3b waves: Wave 1 = store-tag-primitive + audit-op-list (foundational); Wave 2 = StoreTagService; Wave 3 = context_tag handler.

| Component | Wave | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| Direct tag-write primitives (`unimatrix-store`, new fns beside `write.rs:161`) — atomic single-row `add_tag`/`remove_tag` + namespace-scoped `replace_tag` | 1 | pseudocode/store-tag-primitive.md | test-plan/store-tag-primitive.md |
| Audit op-list update (`unimatrix-store` `audit.rs:84`) — add `'context_tag'` | 1 | pseudocode/audit-op-list.md | test-plan/audit-op-list.md |
| `StoreTagService` (`unimatrix-server` `services/store_tag.rs`, new) — `check_write_rate` → store write → audit | 2 | pseudocode/store-tag-service.md | test-plan/store-tag-service.md |
| `context_tag` handler (`unimatrix-server` `mcp/tools.rs`) — identity, `Write` gate, action/tag parse, namespace derivation, lifecycle guard, marked value-opacity seam (no validator), delegate | 3 | pseudocode/context-tag-handler.md | test-plan/context-tag-handler.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| How to write the tag | New store single-row primitive (`add_tag`/`remove_tag`/`replace_tag`); never `update()`, never `context_correct` | SD-1, SD-2, SD-3 | architecture/ADR-001-direct-entry-tags-write.md |
| Cache/index invalidation on tag write | None — all tag reads are live SQL; nothing caches tags (full blast radius enumerated, ARCHITECTURE §3) | SD-4, SR-01, A1 | architecture/ADR-002-no-inmemory-invalidation.md |
| `replace` action semantics | First-class client-supplied action; atomic ONE SQL transaction (namespace-scoped DELETE + INSERT new); ONE audit event; value-opaque (no allow-list consulted) | SD-5, SR-02 | architecture/ADR-004-atomic-single-value-replace.md |
| `replace` on a colon-less / null-namespace tag | Degrades to `add` (pure insert, no prior removed); never hard-errors; audit records `prior_value:null` | ARCHITECTURE §4.3, SPEC §10 | architecture/ADR-004-atomic-single-value-replace.md |
| Authorization posture | `Capability::Write` only; `agent_id` audit-only; `TrustLevel` NOT consulted; gate LOCATION is the enterprise trust-elevation seam (no field wired); value-opacity is the hygiene seam (no validator) | SD-8, SD-9, SD-10 | architecture/ADR-008-authorization-posture-and-seams.md |
| Audit event shape | Complete generic shape shipped in full now (retrofit-hard, append-only): `operation="context_tag"`, `metadata {action, namespace, tag, prior_value, new_value}`; `prior_value` mandatory on remove/replace; `namespace` derived-and-recorded, never validated; exactly one event per mutation | SD-7 | architecture/ADR-009-audit-event-contract.md |
| Live controls | `check_write_rate` (the one live throttle) wired in the service; `'context_tag'` added to `audit_write_count_since` op-list (latent signal, not a live throttle) | SD-11 | architecture/ADR-008 / §6 |
| Lifecycle guards | Re-implemented in-op (not inherited): quarantined → any action refused; deprecated → free-form tag ALLOWED (no protected-tag rule ships) | SD-12 | ARCHITECTURE §1 step 5 |

## Delivery-Critical Items (READ FIRST)

These are the highest-priority failure modes. There is **no Critical risk** in the reduced feature; R-01/R-02/R-03 are High.

- **R-02 / ADR-004 — `replace` is ONE atomic transaction.** `replace_tag` must run the namespace-scoped DELETE-prior + INSERT-new in a single SQLite transaction; a forced INSERT failure must roll back the DELETE (prior value survives), never a zero-tag entry — **prove with a store-level rollback test**. The historical non-transactional partial-write posture (#4420) is a live temptation; mirror `insert_in_txn` (#267). A **colon-less / null-namespace** tag degrades to `add` (pure insert, `prior_value:null`), never a hard error. A `replace` is exactly **one** audit event carrying prior + new together (not one-per-DELETE/INSERT half).

- **R-03 / ADR-009 — audit is the PRIMARY, retrofit-hard control.** Emit the complete generic metadata `{action, namespace, tag, prior_value, new_value}`. `prior_value` **mandatory and non-null** on every `remove` and every `replace` (with a prior); `null`/absent on plain `add`. `namespace` is derived from the tag prefix (substring before first `:`, else `null`) and is **recorded but NEVER validated** — value-opacity holds even in the audit path. Metadata must be a well-formed JSON object and **never the `"{}"` sentinel** (#5468 — on a serialize error, warn and SKIP the event, do not emit `"{}"`). Exactly **one** event per mutation. Capture `session_id` from `ctx.audit_ctx` **before** `tokio::spawn` (#4388/#4389); serialize `Outcome`/enums as variant strings, not integers (#4366); fire-and-forget audit needs a settle delay (~50ms) before read-back to avoid flakes (#4377). A future `protected_tags` must add ZERO audit fields — get this shape right now.

- **R-01 — invariance after mutation + read-freshness (core-value guard).** Assert learning columns (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`), `content_hash`, `previous_hash`, full edge set, and entry `id` all byte-identical pre/post; no supersession version minted. Read-freshness: a tag-filtered query immediately after `add` → present, after `remove` → absent (no stale window, no invalidation step — ADR-002). The derived-state blast radius (ARCHITECTURE §3) asserts every `entry_tags` reader is live SQL — this guard proves that claim.

- **Value-opacity — the two seams are SEAMS ONLY.** The handler writes the tag **uninterpreted**: `delivery:proven`, `delivery:anythingelse`, and free-form `foo` all succeed on bare `Capability::Write`; no allow-list, no vocabulary, no shape check. The two preserved retrofit points ship as **marked code/ADR notes only** — (1) the value-opacity pre-write interception point where a future `evaluate(tag) -> Allowed | Rejected` hygiene validator drops in; (2) the `Capability::Write` gate LOCATION where a future enterprise trust-elevation check attaches. **NO stub, NO empty `ProtectedTagsConfig`, NO validator, NO `min_trust_level`, NO config type, NO cadence guard** ships. Do NOT conflate the value-opacity seam with `validate_outcome_tags` (`tools.rs:895-898`).

- **R-08 — LIKE-metacharacter over-match on the replace DELETE.** `replace_tag` scopes its DELETE by the **derived** namespace: `DELETE ... WHERE tag LIKE 'namespace:%'`. If the derived namespace contains SQL `LIKE` metacharacters (`%`/`_`), the delete over-matches and removes unintended sibling tags. Either **reject** such a derived prefix as malformed, or **LIKE-escape** the metacharacters so the delete matches only true `namespace:` rows. All INSERT/DELETE MUST use bound parameters (no string interpolation) — `tag` is stored and matched literally.

- **Test-seam constraint (#5468).** The `context_tag` `#[tool]` handler is NOT unit-constructible (needs a `RequestContext` only the live MCP transport supplies). Prove orchestration + audit at the `StoreTagService` + store-primitive + `audit_log` read-back seams (directly constructible) over `make_server()`; end-to-end **route/format** proofs go in the Stage-3c integration suite. Do NOT assert handler behavior by instantiating the `#[tool]` fn.

## Files to Create / Modify

**Create:**
- `crates/unimatrix-server/src/services/store_tag.rs` — `StoreTagService` mirroring `store_correct.rs`: `check_write_rate` → store write → fire-and-forget audit.

**Modify:**
- `crates/unimatrix-server/src/mcp/tools.rs` — new `#[tool(name="context_tag")]` fn + `TagParams` struct in the `#[rmcp::tool_router]` block (`:611`), following the `context_correct` wiring template (`:1226-1323`): build context, `Write` gate, action/tag parse, namespace derivation, lifecycle guard, marked value-opacity pre-write seam (comment only), delegate to service.
- `crates/unimatrix-store/src/write.rs` (or beside `:161`) — new `add_tag` / `remove_tag` / `replace_tag` primitives (each atomic, direct `entry_tags` write, touches no `entries` column).
- `crates/unimatrix-store/src/audit.rs` — add `'context_tag'` to the `audit_write_count_since` op-list (`:84`).

No config file, no `server.rs` state field, no `main.rs`/`http_provision.rs` threading, no `gateway.rs` change — those belonged to the deferred `protected_tags`.

## Data Structures

```
// mcp/tools.rs
struct TagParams { id: i64,
                   action: String /* "add" | "remove" | "replace" */,
                   tag: String,
                   agent_id: Option<String> /* self-declared; AUDIT-ONLY, never authz */,
                   format: Option<String> }

// audit metadata (JSON on existing AuditEvent — no schema change; metadata: String, default "{}")
{ action,       // "add" | "remove" | "replace"
  namespace,    // substring of `tag` before first ':', else null — DERIVED, NEVER validated
  tag,          // full tag string written, verbatim
  prior_value,  // non-null on remove/replace-with-prior; null on add / no-prior replace
  new_value }   // written value; null on remove
```

No `ProtectedTagsConfig`, no `ProtectedTagRule`, no `TagDisposition`, no `min_trust_level` — deferred.

## Function Signatures (new interfaces — use exactly; do not invent)

```
// mcp/tools.rs
#[tool(name="context_tag")]
async fn context_tag(&self, Parameters(TagParams), RequestContext) -> ...

// unimatrix-store (beside write.rs:161) — each one atomic tx, direct entry_tags write, touches no entries column
fn add_tag(entry_id: u64, tag: &str) -> Result<()>
fn remove_tag(entry_id: u64, tag: &str) -> Result<()>
fn replace_tag(entry_id: u64, namespace: &str, new_tag: &str) -> Result<Option<String> /* evicted prior */>

// services/store_tag.rs
StoreTagService::tag(id, action, tag, namespace, audit_ctx, caller_id) -> Result<TagResult>
```

`action` is a first-class client value (add/remove/replace). `namespace` is derived by the handler (substring before first `:`, else null) and passed to service/audit — recorded, never validated.

**Reused contracts (do not re-implement):** `build_context_with_external_identity(...)` (`tools.rs:1236`); `require_cap(&ctx.agent_id, Capability::Write)` (`tools.rs:1245`); `gateway.check_write_rate(caller_id)` (`gateway.rs:166`); `entry_store.get(id)` (`read.rs:163-178`); `load_tags_for_entries(pool, &[id])` (`read.rs:111-150`); `audit.log_event_async(event)` (fire-and-forget, `store_correct.rs:98-102`); lifecycle-guard precedent (`write_ext.rs:471-482`). See ARCHITECTURE §4.1.

## Constraints

- Tags are outside the content hash (`hash.rs:7-16`) — mutation MUST NOT touch `content_hash`/`previous_hash` (SD-1/SD-3).
- MUST NOT reuse `update()` (`write.rs:97`, rewrites hash at `:115`) — write `entry_tags` directly via single-row INSERT/DELETE (SD-2). MUST NOT invoke `context_correct` (unchanged, SD-10).
- No schema change (SD-3); no DB migration; the audit `metadata: String` JSON already accommodates the SD-7 shape (no `AuditEvent` column change).
- Authorization is `Capability::Write` only; `TrustLevel` is NOT touched/activated; `agent_id` is audit-only, never an authz input (SD-9). Mint no `Capability::Tag`; do NOT split add/remove/replace at the capability layer.
- The value-opaque pre-write seam MUST NOT conflate with `validate_outcome_tags` (`tools.rs:895-898`); `validate_outcome_tags` behavior on the `context_store` path is unchanged (no regression).
- `check_write_rate` is the only live throttle (resets on restart, exempts `UdsSession`); `audit_write_count_since` is a latent, non-enforcing signal.
- Lifecycle guards (`write_ext.rs:471-482`) are NOT inherited — re-implement in-op: quarantined → any action refused; deprecated → free-form tag ALLOWED (no protected-tag concept ships).
- The `replace` namespace-scoped DELETE must LIKE-escape or reject `%`/`_` in the derived prefix (R-08).
- Rust workspace rules: file-size limits; extend existing fixtures/helpers (test infra is cumulative); Grep/Glob not Bash.

## Dependencies

- **Research:** ass-093 FINDINGS (`product/research/ass-093/FINDINGS.md`, #926, mechanism); ass-094 FINDINGS + INTERNAL/EXTERNAL (`product/research/ass-094/FINDINGS.md`, #927, authorization + anti-poison — informs the retrofit-hard seams kept: audit shape SD-7, gate location SD-9, and the deferred policy).
- **Worked-example consumer:** uni-capability skill (`.claude/skills/uni-capability/SKILL.md:38,83,86,89-90`) + pattern #5505 — the `delivery:` tag, STATUS-vs-CONTENT axis. `context_tag` does NOT depend on this vocabulary (value-opaque); `delivery:` is only an example.
- **Prior features:** vnc-014 (`AuditEvent` provenance fields — the SD-7 shape lands in its `metadata`); nxs-008 #360 (`entry_tags` FK `ON DELETE CASCADE`). *(vnc-040 Feature A / #799 per-slug config and vnc-034 per-slug DB are dependencies of the DEFERRED `protected_tags` feature, not vnc-045.)*
- **Crates:** `unimatrix-store` (write/audit/hash/schema), `unimatrix-server` (tools/registry/gateway).
- **Confirmed integration points at HEAD** are enumerated in SPECIFICATION §9 and ARCHITECTURE §4.

## NOT in Scope

- `context_tag` as an access-control boundary / lockdown on tags (SD-10) — it is a fast path; `context_correct` (unchanged) can still change any tag.
- Identity-based / elevated-trust authorization; anti-self-attestation control; `agent_id` is audit-only (SD-9).
- Op-level / platform evidence enforcement — the platform does NOT verify `delivery:proven` is backed by proof (Non-Goal 4).
- Content mutation in `context_tag` — tags only; `proven_by`/`delivered_by` content stays a separate `context_correct` owned by the evaluating agent (Non-Goal 5).
- Modifying `context_correct` (unchanged, SD-10).
- Hard-coded tag vocabulary; `Capability::Tag`; add/remove/replace capability split.
- `entry_metadata` column; first-class `status` column; status-as-edge (SD-3).
- Activating `audit_write_count_since` as a live throttle (future-proofing only).

## Deferred to Future `protected_tags` Feature (do NOT build in vnc-045)

The `protected_tags` value-hygiene policy is **deferred in full** and carries **no code or test requirement** in vnc-045. Delivery must NOT build any of the following — they retrofit cleanly onto the two preserved seams + the audit shape at the same cost later (which is why they are deferred; pre-building inert plumbing that nothing consumes cannot be behaviorally tested and rots — the `::default()` trap):

- `ProtectedTagsConfig` / `ProtectedTagRule` config type (`{prefix, allowed_values, single_value, min_trust_level, ...}`).
- Five-site per-slug threading (new nested `UnimatrixConfig` type, `merge_configs` arm, `validate_config` extension, `PER_SLUG_CONFIG_CLASSIFICATION` entry, server-state snapshot, `build_project_server` threading) and the daemon-vs-per-slug divergence.
- The value-hygiene allow-list validator module (rejecting typo'd values like `delivery:provn`) — drops into the marked value-opacity seam later.
- `single_value` CONFIG — would make `replace` the *default* for a prefix. (The `replace` **action** ships now, ADR-004.)
- `min_trust_level` — enterprise trust-elevation at the existing `Write` gate.
- `PerSlugOverlayable` classification; per-`(entry, namespace)` cadence guard (ADR-007 deferred).

**Do NOT** write a test that requires a validator/`evaluate(tag)` rejection path, a `min_trust_level` accept/reject difference, or a config type — none ships (RISK-TEST-STRATEGY R-04 negative proof; Security §; Residual #4).

## Alignment Status

Vision guardian: **PASS 7 / WARN 0 / VARIANCE 0 / FAIL 0** (ALIGNMENT-REPORT.md, 2026-07-07, re-run against reduced sources). **No variances requiring approval. No open questions.** The reduction tightens milestone discipline — the only forward-looking surface (`protected_tags`) is deferred rather than pre-built. Four scrutinized axes confirmed aligned:

1. **Self-learning preservation** (#5518) — preserving the learning vector on volatile tag changes removes the churn the self-learning vision warns against (FR-06/NFR-01/AC-02; R-01 core-value guard).
2. **Audit-as-primary-control** (#5474 integrity) — the complete generic append-only audit shape (ADR-009) is the primary control and the one genuinely retrofit-hard piece; declarative-attribution bound (`credential_type="none"`) documented as accepted risk, not elevated to a vision claim.
3. **Value-opacity / domain-agnostic** (#5517) — the engine writes any tag without interpreting it; `delivery:` is an example only, no vocabulary hard-coded, no `Capability::Tag`.
4. **Architect-for-enterprise, build-for-OSS** — the retrofit-HARD seams (audit shape, `Write`-gate LOCATION, value-opacity interception point) are kept; the retrofit-CHEAP `protected_tags` plumbing is deferred, not pre-built inert. Refines guardian pattern #5607: deferring inert plumbing entirely is also PASS — and is preferred over shipping it inert when nothing consumes it (retro-flagged reconciliation of #5607's now-stale `min_trust_level` R-10/AC-09b instance).

**Carry-forward items (all correctly OUT of scope, no test requirements imposed):** `context_correct` learning-vector reset on content corrections (ass-093 OoS); `audit_write_count_since` dormancy / SLN1 budget wiring (ass-094 OoS); metadata-filter-bypass tenant isolation — inert under 1-client:1-project + per-slug DB (vnc-034), flag if cross-project retrieval is introduced (A3 guard); enterprise per-agent identity that would activate a future `min_trust_level`.
