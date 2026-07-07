# FINDINGS: In-place mutation of non-content fields without context_correct

**Spike**: ass-093
**Date**: 2026-07-06
**Approach**: investigation + evaluation
**Confidence**: directional (load-bearing question settled **empirically** with file:line evidence)

---

## Load-bearing question (settled first)

**Are tags inside the `content_hash` / `previous_hash` supersession chain, or outside it?**

**Answer: OUTSIDE. Tags are folded into no hash.** In-place tag mutation cannot break tamper-evidence.

**Evidence (the actual hash computation):**

- `compute_content_hash(title, content)` hashes **only** title and content — nothing else:
  `crates/unimatrix-store/src/hash.rs:7-16`. The hashed string is `format!("{title}: {content}")` (or a degenerate variant when one is empty). Tags are not a parameter and are not referenced.
- The single integrity oracle recomputes the hash from **title/content only** and compares to the stored `content_hash`:
  `crates/unimatrix-store/src/chain_verify.rs:152` — `let computed = compute_content_hash(&e.title, &e.content);`. A tag change can therefore never produce a `ContentHashMismatch`.
- `previous_hash` is defined as the **predecessor's `content_hash`** (title/content), not anything tag-derived:
  chain-link check `crates/unimatrix-store/src/chain_verify.rs:190` (`pred.content_hash != e.previous_hash`); correction sets `previous_hash: original.content_hash.clone()` at `crates/unimatrix-store/src/write_ext.rs:555`.
- Tags live in a **separate junction table** (`entry_tags`, ADR-006), written independently of the entries row: `INSERT INTO entry_tags (entry_id, tag)` at `write.rs:78`, `write.rs:168`, `write_ext.rs:613`; replace pattern is `DELETE FROM entry_tags WHERE entry_id = ?` then re-INSERT at `write.rs:161-174`.

**Is `entry_tags` part of any integrity chain? Is there an independent audit chain over tag writes?**

- No cryptographic chain covers tags. The audit log (`audit_log`) is a **monotonic-id append log** (counter `next_audit_id`), not a hash chain — no `prev_hash` column, columns are `event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail, credential_type, capability_used, agent_attribution, metadata` (`crates/unimatrix-store/src/audit.rs:46-64`). `operation` is a free-form string.
- Consequence: tag integrity rests on **capability gating + the append-only audit log** (Principles 2 & 3), not on Principle 1 cryptographic tamper-evidence. This is consistent with tags being volatile metadata rather than knowledge content.

**Verdict: the cheap path is open.** Nothing in the hash chain forbids an in-place tag mutation. The remaining question is purely whether an *authorized, audited* mutate op can be added cleanly — and it can.

---

## Findings

### Q1: Hash-chain membership — which fields are hashed?
**Answer**: Only `title` and `content`. `content_hash = SHA-256("{title}: {content}")` (hash.rs:7-16). `previous_hash` = predecessor's `content_hash`. Everything else on the row (`status`, `confidence`, `tags`, `access_count`, `helpful_count`, `feature_cycle`, timestamps, edges, …) is **outside** the integrity chain. The verifier only recomputes title/content (chain_verify.rs:152).
**Evidence**: hash.rs:7-16; chain_verify.rs:152, 190; write_ext.rs:555.
**Recommendation**: Treat tags as non-hashed volatile metadata. In-place mutation is integrity-safe by construction — no schema change to the hash is needed or permitted.

### Q2: Full blast radius of an in-place tag mutation
**Answer** (per field):

- **Audit log** — A dedicated op is *not schema-required* (operation is a free-form string, audit.rs:57) but a **dedicated operation label is recommended** (e.g. `context_tag`) so transitions are attributable and rate-limitable. The event carries `agent_id`, `capability_used`, `timestamp`, `target_ids`, and a `metadata` JSON where the added/removed tag (and prior value) should be recorded. Evidence: audit.rs:19-74.
- **Capability check** — Write ops gate on `require_cap(&ctx.agent_id, Capability::Write)` (tools.rs:884, 1245, 1447, 3632). A tag-mutate op should gate on the same `Capability::Write`. The `Capability` enum already exists (registry.rs). No new authz primitive needed.
- **In-memory hot path (Principle 7) — no new stale window.** Tag-filtered reads load tags **live from `entry_tags` via SQL** on every query (`load_tags_for_entries` / `apply_tags`, graph_read_filter.rs:199-209; search.rs loads tags the same way). Tags do **not** live in the `Arc<RwLock<_>>` analytics cache — that cache holds derived ranking signals (co_access/PPR/confidence), which tags don't feed. So an in-place tag write is visible to the next tag-filtered query immediately; there is no tick-rebuild lag for tag membership. Evidence: graph_read_filter.rs:186-209; server.rs:454 (analytics cache is the *ranking* serving path, not tag storage).
- **Learning signal — this is the decisive argument for in-place.** `context_correct` mints a fresh entry that **hard-resets** the learning columns to zero: `confidence: 0.0`, `access_count: 0`, `last_accessed_at: 0`, `helpful_count: 0`, `unhelpful_count: 0` (write_ext.rs:542-561; INSERT binds `0.0`, `0`, `0`, `0` at :587-606). The MCP handler inherits only *content* fields from the original (tools.rs:1306-1320); it does not re-seed confidence/usage. So routing a `partial→proven` status flip through `context_correct` **destroys the entry's entire self-learning history on every transition** — the exact churn the vision warns against. An in-place tag mutate touches none of those columns, so the learning signal is fully preserved. Confidence/usage are themselves already mutated in place elsewhere (write_ext.rs:131, 158, 174) — tags should join that family.
- **Edges** — `context_correct` must **carry edges forward** to the new id via `run_carry_forward_loop` (tools.rs:1370; vnc-035). An in-place mutate keeps the same `entry_id`, so edges are untouched — zero re-point, and it sidesteps the class of bugs that live in the carry path. Confirmed: no edge implications.
- **Search / embedding** — **No re-embed.** Embedding input is `prepare_text(title, content, sep)` — title+content only (embed/src/text.rs:10; hash.rs doc note "matches prepare_text"). Tags are not in the embedded text, so a tag change requires no re-embedding. Confirmed as expected.

**Recommendation**: An in-place tag mutate is cheap and safe on every axis: no re-hash, no re-embed, no edge re-point, no learning reset, no new stale-index window. The only real work is authorization + audit.

### Q3: Peer non-content fields — what else shares this churn problem / is already mutated in place?
**Answer**: A whole family of volatile fields is **already mutated in place today** on the same `entry_id` (no supersession, no new entry):

| Field | In-place today? | Site |
|-------|-----------------|------|
| `confidence` | Yes | write_ext.rs:131 (`record_usage_with_confidence`), :174 (`update_confidence`) |
| `access_count` | Yes (increment) | write_ext.rs:158 |
| `last_accessed_at` | Yes (usage tick) | write_ext.rs:47-160 |
| `status` (lifecycle) | Yes | write.rs:209 (`update_status`); write_ext.rs:377/390 (deprecate/quarantine) |
| `pre_quarantine_status` | Yes | server.rs:2514, 2627 |
| `superseded_by`, `correction_count` | Yes | write_ext.rs:505 |
| `embedding_dim` | Yes | write_ext.rs:235 |
| `helpful_count` / `unhelpful_count` | Column-level, in place | schema columns; reset only by correct |
| `co_access` (derived) | Yes (upsert, separate table) | analytics.rs:409 |
| **`tags`** | **Only via full-entry `update()` (DELETE-all + re-INSERT), NOT exposed as a standalone MCP op** | write.rs:161-174 |

**The outlier is tags.** Confidence, usage, lifecycle status, and co-access all have lightweight in-place mutation paths; tags are the only volatile-metadata field whose sole MCP mutation route is the heavy `context_correct`. The store even *has* an in-place tag primitive already (the `update()` tag DELETE/re-INSERT), it is simply not surfaced as a targeted operation.
**Recommendation**: The mechanism should be the generic "mutate a non-hashed volatile field in place" pattern that already governs confidence/usage/status. Tags slot into it; do not special-case "status." Note the store `update()` primitive rewrites all 24 columns including `content_hash` — the new op must write `entry_tags` directly (single-row INSERT/DELETE), not reuse `update()`.

### Q4: Mechanism comparison (domain-agnostic only)
Scored 1-5 (5 = best) against the six criteria.

| Criterion | (a) In-place tag mutate op | (b) Status as typed edge/annotation | (c) New "mutable-metadata" lane |
|-----------|:--:|:--:|:--:|
| Integrity preservation (no hash impact) | 5 | 5 | 5 |
| Audit / provenance | 5 (audit op + metadata) | 4 (edge writes audited, but transition history is edge churn) | 5 |
| Authorization / poison-resistance | 4 (Capability::Write + optional vocab) | 3 (edges weight-manipulable; graph pollution) | 4 |
| Learning-signal correctness | 5 (touches no learning column) | 5 | 5 |
| Index consistency (no stale window) | 5 (live SQL on entry_tags) | 4 (edge caches / PPR feed on graph) | 3 (net-new read-path integration to prove) |
| Generality across domains | 5 (tags already the generic lane) | 2 (status is a node property, not a relation; needs sentinel target nodes) | 4 (general but duplicates tags) |
| **Total** | **29** | **23** | **26** |

- **(a) wins.** Tags already **are** the domain-agnostic non-hashed mutable-metadata lane: outside the hash, in a junction table, already mutated in place by `update()`, already the generic filter axis for retrieval. The only gap is the absence of a lightweight, authorized, audited *single-tag* MCP op. Add one (e.g. `context_tag(id, add|remove, tag)`), gated by `Capability::Write`, writing `entry_tags` directly and emitting an audit event with a dedicated `operation` label.
- **(b)** is over-engineered: status is a property of one node, not a relation between two. Modeling it as an edge needs sentinel target nodes per status value, pollutes the traversal/PPR graph with non-knowledge nodes, and turns "list all proven capabilities" into a graph query. Edges are weighted relations built for traversal; hijacking them for a scalar property misuses the lane.
- **(c)** is a net-new `entry_metadata` key/value table with its own migration and read-path integration — and it duplicates what `entry_tags` already is. Reserve this only if a future need for *typed*, *multi-valued* structured metadata emerges that tags genuinely cannot express; today it is unjustified.

**Recommendation**: **(a) — an authorized, audited in-place tag-mutate op on the generic tag lane.** Do NOT keep routing status flips through `context_correct`.

### Q5: Provenance of transitions — is history preserved?
**Answer**: Dropping the correction chain for tags loses the *content-chain* version history of a transition (`partial→proven` is no longer a new versioned entry). **That loss is acceptable and the correct home for transition history is the audit log, not the content chain.** The append-only audit log already records every mutation with `agent_id`, `timestamp`, `operation`, `target_ids` (audit.rs:46-64). If the tag-mutate op writes the added/removed tag (and, ideally, the prior status tag) into the audit `metadata` JSON, the full transition sequence is queryable per entry — a lightweight transition log with no bespoke table. Status is volatile *signal*, not knowledge *content*; provenance of "who flipped it when" belongs in the audit log (Principle 2), which is exactly where it will land.
**Recommendation**: Make recording prior + new tag value in the audit `metadata` a **hard requirement** of the op. No separate transition table is needed. Do not preserve status transitions as content-chain versions.

---

## Unanswered Questions

None. All five scope items are answered with code evidence. Two items are *design choices* to settle at implementation (not research blockers): (1) whether the tag-mutate op consumes the same write-rate budget as store/correct or a separate one; (2) whether a reserved "status" tag namespace should be server-validated against the controlled vocabulary. Both are noted under risks below.

---

## Out-of-Scope Discoveries

- **`context_correct` resets the full learning vector (confidence, access_count, last_accessed_at, helpful/unhelpful) to zero on every correction** (write_ext.rs:542-561). This is broader than status-via-tag: any legitimate *content* correction also wipes the entry's accumulated self-learning signal and usage history. Worth a separate spike on whether corrections should carry forward (or decay rather than zero) confidence/usage — it directly affects the self-learning vision. One-line rationale: silent learning-history loss on every content edit may be under-considered.
- **`audit_write_count_since` counts only `context_store` and `context_correct`** (audit.rs:79-92) for the SLN1 write-budget. Any new mutate op is invisible to this budget unless explicitly added — a poison-budget gap to close deliberately, not by omission.
- **The store `update()` primitive rewrites all 24 entry columns including `content_hash` and `previous_hash`** (write.rs:116-158) while also DELETE-all/re-INSERTing tags. It is a blunt instrument; a tag-only op must not reuse it, or it risks disturbing chain fields. Flag for the implementer.

---

## Recommendations Summary

- **Q1 (hash membership)**: Tags are OUTSIDE the content hash — `content_hash` covers only title+content (hash.rs:7-16, chain_verify.rs:152). In-place tag mutation is integrity-safe. Cheap path is open.
- **Q2 (blast radius)**: In-place tag mutate needs no re-hash, no re-embed, no edge re-point, no learning reset, and opens no new stale-index window (tags read live from SQL). Only real work: capability gate + audit event.
- **Q3 (peer fields)**: confidence, access_count, last_accessed_at, lifecycle status, co_access are already mutated in place; tags are the lone volatile field stuck on the heavy `context_correct` path. Generalize the existing in-place pattern to tags; don't special-case "status."
- **Q4 (mechanism)**: Choose **(a)** — an authorized (`Capability::Write`), audited in-place tag-mutate op on the generic tag lane (e.g. `context_tag`). Reject (b) edge/annotation (graph pollution, status isn't a relation) and (c) new metadata lane (duplicates tags). Do NOT run status flips through `context_correct` — it zeroes the learning vector every transition.
- **Q5 (transition history)**: Preserve transitions in the **audit log metadata** (record prior + new tag), not the content chain. No bespoke transition table needed; content-chain version loss for a volatile signal is acceptable.
- **Integrity/poison gates for implementation**: (1) gate on `Capability::Write`; (2) emit a dedicated audit `operation` with old/new tag in metadata; (3) fold the op into the SLN1 write-rate budget (extend `audit_write_count_since`); (4) optionally validate a reserved status-tag namespace against the controlled vocabulary (missing/partial/proven/claimed) so the op cannot inject arbitrary retrieval-steering tags; (5) write `entry_tags` directly — do NOT reuse the 24-column `update()` primitive.
