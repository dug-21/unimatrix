# vnc-045 Architecture — `context_tag`: Domain-Agnostic In-Place Tag Mutation (mechanism only)

> Scope: SCOPE.md (12 Settled Decisions; scope REDUCED 2026-07-07 — `protected_tags` value-hygiene policy DEFERRED). This feature ships the **`context_tag` MECHANISM only**. Research: ass-093 (mechanism), ass-094 (authorization — informs the retrofit-hard seams kept). Wiring template: `context_correct` (tools.rs:1226-1334 → services/store_correct.rs).
>
> This document is "what/where". The active ADRs carry the "why": **ADR-001, ADR-002, ADR-004, ADR-008, ADR-009**. ADR-003/005/006/007 are **DEFERRED** with `protected_tags` (kept on disk, marked, deprecated in Unimatrix) — they are NOT part of vnc-045 delivery.

---

## 1. System Overview

`context_tag(id, action, tag)` is a new MCP op that mutates the generic `entry_tags` junction lane **in place**, tag-only, on the same `entry_id`. It is a **parallel fast path** to `context_correct`, which is left entirely unchanged (SD-10). The op is **value-opaque**: it writes a tag it never interprets — no allow-list, no vocabulary. The op:

- writes `entry_tags` directly (single-row `INSERT`/`DELETE`), never through the 24-column `update()` primitive and never through `context_correct` (SD-1, SD-2);
- touches **no** hashed column (`content_hash`/`previous_hash` cover only title+content — hash.rs:7-16) and **no** learning column (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`), so the entry's self-learning vector and supersession chain are preserved (contrast: `context_correct` zeroes the vector — write_ext.rs:542-561);
- gates on `Capability::Write` only — the same baseline `context_correct` requires; `agent_id` is audit-only, never an authorization input (SD-9);
- refuses tagging a **Quarantined** entry; **allows** tagging a **Deprecated** entry (SD-12);
- emits a dedicated `operation="context_tag"` audit event as the **primary control** (SD-7, ADR-009), and is subject to the live `check_write_rate` throttle (SD-11).

It sits in the `unimatrix-server` MCP tool layer (handler) → services layer (rate + audit) → `unimatrix-store` write layer (direct `entry_tags` write). No schema change, no DB migration (SD-3). **No `protected_tags` config, no value-hygiene validator, no per-slug threading, no cadence guard, no server-state policy field ships in vnc-045** — those are the deferred `protected_tags` feature (§8).

### Where it fits (request lifecycle)

```
MCP client
  │  context_tag(id, action, tag, agent_id, format)
  ▼
[HANDLER]  mcp/tools.rs  #[tool(name="context_tag")]  (router block, tools.rs:611)
  1. build_context_with_external_identity(...)      → ctx.agent_id / ctx.audit_ctx / ctx.caller_id
  2. require_cap(&ctx.agent_id, Capability::Write)   → SD-9 baseline gate
        ▲── RETROFIT SEAM #2: enterprise trust-elevation attaches HERE, at the existing Write gate
  3. validate action ∈ {add, remove, replace}; parse tag; derive namespace = prefix before first ':' (else null)
  4. load entry (lifecycle + author for audit)
  5. lifecycle guard: refuse if Quarantined; ALLOW if Deprecated   (SD-12 / ass-094 B5)
  ┌───────────────────────────────────────────────────────────────────────────┐
  │  ── RETROFIT SEAM #1: VALUE-OPACITY PRE-WRITE INTERCEPTION POINT ──         │
  │  vnc-045 writes the tag WITHOUT interpreting its value (no validator).      │
  │  A future protected_tags policy drops exactly one call here:                │
  │      evaluate(tag) -> Allowed | Rejected                                    │
  │  NO stub, NO empty config, NO call in vnc-045 — a marked seam ONLY.         │
  └───────────────────────────────────────────────────────────────────────────┘
  ▼
[SERVICE]  services/store_tag.rs  (new; mirrors store_correct.rs)
  6. gateway.check_write_rate(&caller_id)            → SD-11 live throttle (gateway.rs:166)
  ▼
[STORE]  unimatrix-store  (new single-row primitives; NOT update())
  7. atomic tx:
       action=add     → INSERT INTO entry_tags(entry_id, tag)
       action=remove  → DELETE FROM entry_tags WHERE entry_id=? AND tag=?        (single-row, NOT DELETE-all)
       action=replace → DELETE FROM entry_tags WHERE entry_id=? AND tag LIKE 'namespace:%'  +  INSERT new   (ONE tx — ADR-004)
  ▼
[AUDIT]  infra/audit.rs — fire-and-forget after commit  (mirror store_correct.rs:98-102)
  8. AuditEvent { operation="context_tag", target_ids=[id], agent_id, capability_used,
                  metadata = {action, namespace, tag, prior_value, new_value} }   (ADR-009; prior_value on remove/replace)
```

No re-hash, no re-embed, no edge re-point, no learning reset, no in-memory cache invalidation (ADR-002).

---

## 2. Component Breakdown

| Component | Crate / file (HEAD) | Responsibility | New / Changed |
|-----------|--------------------|----------------|---------------|
| `context_tag` handler | `unimatrix-server` `mcp/tools.rs` (router block `:611`) | identity, `Write` gate, action/tag parse, namespace derivation, lifecycle guard, the marked value-opacity seam (no validator), delegate | **New** `#[tool]` fn + `TagParams` struct |
| `StoreTagService` | `unimatrix-server` `services/store_tag.rs` (new, mirrors `store_correct.rs`) | `check_write_rate` → store write → audit | **New** |
| Direct tag-write primitives | `unimatrix-store` (new fns beside `write.rs:161-174`) | single-row `add_tag`/`remove_tag`; atomic namespace-scoped `replace_tag` | **New** (ADR-001, ADR-004, SD-2) |
| Audit op-list | `unimatrix-store` `audit.rs:84` | add `'context_tag'` to `audit_write_count_since` | **Changed** (latent signal; SD-11) |

`context_correct`, `update()`, the hash chain, the embedding path, and edges are **untouched** (SD-10, SD-3, ass-093 Q2).

**Explicitly NOT in vnc-045** (deferred to `protected_tags`, §8): any `ProtectedTagsConfig` type, `evaluate_protected_tag` validator module, per-slug config threading (the five-site plumbing), `merge_configs` arm, `validate_config` extension, `PER_SLUG_CONFIG_CLASSIFICATION` entry, `PerSlugOverlayable` disposition, server-state policy field, `min_trust_level`, and the per-`(entry, namespace)` cadence guard.

---

## 3. `entry_tags` Derived-State Blast Radius (completeness statement)

The fast path's integrity rests on this being complete. Every surface derived from or reading `entry_tags` was enumerated. **All tag reads are live SQL; nothing caches tags. A direct `entry_tags` write requires NO in-memory refresh or invalidation** (ADR-002).

| Surface | Location | Read mode | Refresh needed? |
|---------|----------|-----------|-----------------|
| Canonical hydration | `unimatrix-store` `read.rs:111-159` `load_tags_for_entries` + `apply_tags` | Live SQL per call | No |
| MCP read-path tag filter | `unimatrix-server` `mcp/graph_read_filter.rs:186-209` | Calls `load_tags_for_entries` per query | No |
| Store read methods (get / list / by-tag) | `read.rs:173-176,205,225,271,298,318,337,464,1184,1206`; `query_by_tags:231` | Live per call | No |
| Graph queries | `graph_queries.rs:197,289,357`; `graph_read_subgraph.rs:671`; `graph_read_inverse.rs:143` | Live per call | No |
| Search results | `services/search.rs:718,841,984,1133` via `entry_store.get()` → `read.rs:163-178` | Live per result | No |
| Analytics / ranking caches (`Arc<RwLock<_>>`) | `server.rs:454`; co-access `search.rs:1163`; PPR; contradiction cache `infra/coherence.rs:73` | Hold **derived ranking signals only — no tags** | N/A |
| Tag-derived edges | GRAPH_EDGES | **None** — edges are not computed from `entry_tags` | N/A |
| SQL indices | `idx_entry_tags_tag`, `idx_entry_tags_entry_id`, `idx_entry_tags_tag_entry_id` | Maintained automatically by SQLite on `INSERT`/`DELETE` | Automatic |
| Snapshot/backup | `snapshot.rs` | Raw table copy for backup only; not a serving cache | No |

**Invariant this rests on (A1):** tags are outside the content hash (hash.rs:7-16) and outside embedding input (embed/src/text.rs:10). If any future integrity/embedding path begins consuming tags, this conclusion is invalidated — flagged as a carry-forward guard.

---

## 4. Component Interactions & Contracts (Integration Surface)

Downstream implementers MUST use these exact names/signatures; do not invent.

### 4.1 Existing interfaces reused (contracts)

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Identity + audit context | `build_context_with_external_identity(&agent_id, &format, &None, &request_context, None) -> ToolContext` (yields `ctx.agent_id`, `ctx.audit_ctx`, `ctx.caller_id`) | tools.rs:1236 |
| Capability gate | `self.require_cap(&ctx.agent_id, Capability::Write).await?` | tools.rs:1245; server.rs:631 → infra/registry.rs:92 |
| Live write throttle | `self.gateway.check_write_rate(caller_id) -> Result<(), ServiceError>` (per `CallerId`, 60/3600s, in-memory, `UdsSession`-exempt) | services/gateway.rs:166; caller.rs:60 |
| Entry fetch (lifecycle + author) | `self.entry_store.get(id).await -> Result<EntryRecord>` (carries `status`, `created_by`, `modified_by`, live `tags`) | read.rs:163-178 |
| Live tag hydration | `load_tags_for_entries(pool, &[id]) -> HashMap<u64, Vec<String>>` | read.rs:111-150 |
| Audit event | `AuditEvent { event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail, credential_type, capability_used, agent_attribution, metadata }`; `metadata: String` JSON (default `"{}"`) | schema.rs:360; infra/audit.rs |
| Audit sink | `self.audit.log_event_async(event).await` (fire-and-forget via `tokio::spawn`) | store_correct.rs:98-102 |
| Persistent write counter | `audit_write_count_since(agent_id, since)` — op-list at audit.rs:83-84 | audit.rs:79-92 |
| Lifecycle guard precedent | quarantine/deprecate refusal on the correct path | write_ext.rs:471-482 |

### 4.2 New interfaces introduced by this feature

| New Interface | Signature / Shape | Owner |
|---------------|-------------------|-------|
| MCP tool | `#[tool(name="context_tag")] async fn context_tag(&self, Parameters(TagParams), RequestContext)` | mcp/tools.rs |
| Tool params | `struct TagParams { id: i64, action: String /* "add" \| "remove" \| "replace" */, tag: String, agent_id: Option<String>, format: Option<String> }` | mcp/tools.rs |
| Store primitives | `add_tag(entry_id: u64, tag: &str)` / `remove_tag(entry_id: u64, tag: &str)` / `replace_tag(entry_id: u64, namespace: &str, new_tag: &str) -> Option<String> /* prior */` — each atomic, direct `entry_tags` write, touches NO `entries` row | unimatrix-store (beside write.rs:161) |
| Service | `StoreTagService::tag(id, action, tag, namespace, audit_ctx, caller_id) -> Result<TagResult>` | services/store_tag.rs (new) |

Note: `action` is a first-class client value — `add`, `remove`, and `replace` are all client-supplied (ADR-004). `namespace` is derived by the handler from the tag prefix (substring before first `:`, else null) and passed to the service/audit; it is **recorded, never validated** (value-opacity).

### 4.3 Data flow (per action) — value-opaque, no allow-list

- **add**: parse → lifecycle guard → [value-opacity seam: write as-is] → `check_write_rate` → `add_tag` → audit `{action:"add", namespace:<derived|null>, tag, prior_value:null, new_value:tag}`.
- **remove**: parse → lifecycle guard → [seam] → `check_write_rate` → `remove_tag` → audit `{action:"remove", namespace:<derived|null>, tag, prior_value:tag, new_value:null}` (`prior_value` = the removed tag, always non-null).
- **replace (namespaced tag)**: derive namespace → lifecycle guard → [seam] → `check_write_rate` → `replace_tag` (ONE tx: `DELETE ... tag LIKE 'namespace:%'`, `INSERT` new) → audit `{action:"replace", namespace, tag, prior_value:<evicted>, new_value:tag}` (ADR-004; `prior_value` non-null when a prior existed).
- **replace (colon-less / null-namespace tag)**: no namespace group to scope → **degrades to `add`** (pure insert, no prior removed) → audit `{action:"replace", namespace:null, tag, prior_value:null, new_value:tag}` (ADR-004 edge case; least-surprise — never hard-errors on a valid tag).

### 4.4 Error boundaries

| Origin | Error | Surfaced as |
|--------|-------|-------------|
| Missing `Capability::Write` | authz | `require_cap` error (tools.rs:1245 pattern) |
| Unknown `action` / malformed `tag` | validation | `rmcp::ErrorData::invalid_params` |
| Quarantined entry | lifecycle | `invalid_params` (SD-12) |
| Rate exceeded | throttle | `ServiceError::RateLimited` (gateway.rs:91) |
| Store write failure | store | `ServiceError::Core(CoreError::Store(_))` |

Tagging a **Deprecated** entry is **allowed** — it is not an error (SD-12). There is **no hygiene/allow-list error path** in vnc-045 (value-opaque).

---

## 5. Audit Event Contract (retrofit-hard — shipped in full now)

The audit log is append-only; reshaping it later is the retrofit-HARD change. So the **complete, generic** event shape is specced and emitted in full NOW even though nothing enforces tag hygiene yet — a future `protected_tags` adds **zero** audit surface (ADR-009).

```
operation   = "context_tag"
target_ids  = [id]
agent_id, capability_used (Write), timestamp, session_id, credential_type ("none"), outcome
metadata = {
  action,       // "add" | "remove" | "replace"
  namespace,    // substring of `tag` before first ':', else null — DERIVED, NEVER validated
  tag,          // full tag string written
  prior_value,  // emitted on remove/replace; non-null when a prior existed; null on add
  new_value     // written value; null on remove
}
```

Rules (binding): `namespace` derived from the prefix, recorded but never validated; `prior_value` present on every remove/replace (non-null whenever a prior existed — always on remove; on replace unless the degenerate no-prior case); no schema change (lives in the existing `metadata: String` JSON); `'context_tag'` added to `audit_write_count_since` op-list (audit.rs:84, latent signal). See ADR-009 for the full contract and the retrofit-lock rationale.

---

## 6. Live Controls

- **`check_write_rate`** (gateway.rs:166) — the ONE live throttle, per `CallerId`, 60/3600s, in-memory, resets on restart, `UdsSession`-exempt. Wired in `StoreTagService` before the store write (mirror store_correct.rs:29). (SD-11)
- **`audit_write_count_since`** (audit.rs:79-92) — `'context_tag'` added to its op-list (audit.rs:84). A cheap **latent** signal, NOT a live throttle.

The per-`(entry, namespace)` **cadence guard is DEFERRED** (it is a namespace-scoped `protected_tags` concept; ADR-007 deferred). `check_write_rate` is the rate limit that ships.

---

## 7. The Two Preserved Retrofit Seams

vnc-045 ships the mechanism such that the deferred `protected_tags` feature bolts on without reshaping anything. Two seams are preserved and marked:

1. **Value-opacity pre-write interception point** (data-flow §1 step 5→7, ADR-008 point 4). The handler writes the tag WITHOUT interpreting its value. Exactly one pre-write point is marked in code as where a future `evaluate(tag) -> Allowed | Rejected` hygiene validator drops in. **vnc-045 ships the marked seam ONLY — no stub, no empty `ProtectedTagsConfig`, no validator call.** A rejected write would simply never reach the store/audit; the mechanism does not change.

2. **`Capability::Write` gate location** (data-flow §1 step 2, ADR-008 points 1-2). The gate is reused verbatim from `context_correct`. The retrofit-hard contract is the gate LOCATION, not a trust field: an enterprise identity provider later attaches a trust-elevation check at this same gate, where the principal is already resolved. `min_trust_level` is NOT shipped (its owning config does not exist) — there is nothing to wire and no gap.

Both seams retrofit at the same cost as building now, which is why `protected_tags` is deferred rather than pre-plumbed: inert threading with nothing consuming it cannot be behaviorally tested and rots (the `::default()` trap). The audit shape (§5, ADR-009) is the one genuinely retrofit-HARD piece and is therefore shipped in full now.

---

## 8. Deferred: `protected_tags` value-hygiene policy (NOT in vnc-045)

The following is **out of scope** and carries no code or test requirement here. It retrofits onto the two seams (§7) and the audit shape (§5) at the same cost as building now:

- `protected_tags` config type (`Vec<ProtectedTagRule>` of `{prefix, allowed_values, single_value, ...}`).
- Five-site per-slug threading (new nested `UnimatrixConfig` type, `merge_configs` arm, `validate_config` extension, `PER_SLUG_CONFIG_CLASSIFICATION` entry, server snapshot + `build_project_server` threading) — the daemon-vs-per-slug divergence.
- Value-hygiene allow-list validator (rejecting typo'd values like `delivery:provn`) — drops into the marked value-opacity seam (§7 #1).
- `single_value` CONFIG — makes `replace` the *default* for a prefix. (The `replace` **action** ships now, ADR-004.)
- `min_trust_level` — enterprise trust-elevation at the existing `Write` gate (§7 #2).
- `PerSlugOverlayable` classification so separate slugs carry different tag policies.
- Per-`(entry, namespace)` cadence guard (deferred ADR-007).

Reasoning preserved in the DEFERRED ADR files (ADR-003/005/006/007) on disk; those Unimatrix entries are deprecated with reason pointing here.

---

## 9. Assumption A3 Confirmation (tenant-isolation carve-out)

**Confirmed inert-and-safe at HEAD.** The metadata-filter-bypass tenant-isolation risk (ass-094 OoS) is inert **only while no cross-project retrieval exists**. Cross-project isolation today is **structural**: 1-client:1-project + per-slug DB (vnc-034 — each slug owns `{base}/{slug}/` DB tree). An agent bound to project X's DB physically cannot reach project Y's entries; tags are never a cross-project access-control filter. **Guard:** if cross-project retrieval is ever introduced, tags-as-ranking-hint must not become tags-as-access-control — flag for a spike (carry-forward). Documented, accepted boundary, not a closed one.

---

## 10. Resolved Decisions (ADR index)

| ADR | Title | Status |
|-----|-------|--------|
| ADR-001 | `context_tag` writes `entry_tags` directly via a new single-row primitive, not `update()` | **Active** |
| ADR-002 | No in-memory invalidation — `entry_tags` is read strictly live | **Active** |
| ADR-004 | `replace` is a first-class action, atomic in one transaction, one audit event (namespace-derived; single_value CONFIG deferred) | **Active (revised)** |
| ADR-008 | Authorization posture: `Capability::Write` gate IS the trust seam; `agent_id` audit-only; value-opacity is the hygiene seam (min_trust_level removed) | **Active (revised)** |
| ADR-009 | The complete generic `context_tag` audit-event shape is a retrofit-hard contract shipped in full now | **Active (new)** |
| ADR-003 | `protected_tags` value-hygiene dedicated check | **DEFERRED** → future `protected_tags` |
| ADR-005 | `protected_tags` per-slug threading | **DEFERRED** → future `protected_tags` |
| ADR-006 | `merge_configs` replaces the `protected_tags` rules list | **DEFERRED** → future `protected_tags` |
| ADR-007 | Cadence guard state model | **DEFERRED** → future `protected_tags` |

---

## 11. Open Questions

**None blocking.** All settled decisions respected; no schema change; no placeholder/stub. The `replace` colon-less edge case is resolved (degrade to `add`, ADR-004). The only carry-forward guards are the A1 (tags-outside-hash) and A3 (cross-project retrieval) invalidation triggers, both documented above.
