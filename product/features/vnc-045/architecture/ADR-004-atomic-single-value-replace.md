## ADR-004: `replace` is a first-class action, atomic in one transaction, counting as one audit event

### Context
`context_tag(id, action, tag)` takes `action ∈ {add, remove, replace}`. `replace` sets a new value for a namespace while removing the prior value in that namespace, so exactly one tag per namespace survives (the archetypal use is a status flip such as `delivery:partial` → `delivery:proven`). The mechanism must decide (a) whether `replace` is a client-supplied action or a server-side inference, and (b) how the implied remove + add are sequenced.

Two hazards: if the implied remove and add are NOT one transaction, a crash/interleave mid-replace leaves the entry with ZERO tags in that namespace — lost status. And if a `replace` logs two audit events (one remove, one add), a single legitimate status flip reads as two mutations, muddying the append-only transition history that is the primary control (SD-7).

Note: the **`single_value` per-prefix CONFIG** that would make `replace` the *default* behavior for a prefix is DEFERRED with `protected_tags` (SCOPE "Deferred / Future Extension"). This ADR covers only the `replace` **action**, which ships in vnc-045 as a client-supplied action and is value-opaque — the server never consults an allow-list to decide it.

### Decision
`replace` is a **first-class, client-supplied action** (`action = "replace"`), realized as one server-side atomic operation `replace_tag(entry_id, namespace, new_tag) -> Option<prior>`. Its "what to replace" key is **self-contained: the namespace derived from the tag prefix** — no config dependency, so the deferred `protected_tags` feature later only flips *replace-as-default-per-prefix*; it does not change what `replace` DOES.

- **Namespace is derived structurally** from the new tag: the substring before the first `:` (else `null`). This derivation is value-opaque — it reads the tag's shape, never interprets its meaning, and applies no allow-list (SD-8). `replace delivery:proven` = remove existing `delivery:*`, insert `delivery:proven`, atomic, one audit event with `prior_value` = the removed value.
- **One SQL transaction:** `DELETE FROM entry_tags WHERE entry_id=? AND tag LIKE 'namespace:%'` then `INSERT` the new tag; commit once (mirror the single-tx tag write at `write.rs:161/168`, but scoped to one namespace, NOT the 24-column `update()` and NOT a DELETE-all). No window in which the entry has zero tags for that namespace (AC-03).
- **Colon-less / null-namespace tag (edge case — resolved: degrade to `add`).** A tag with no `:` (e.g. `urgent`) has no derivable namespace group, so there is nothing to scope a prior removal against. `replace` on such a tag **degrades to a pure insert** (`add` semantics): no prior removed, `prior_value = null`. This is least-surprise — `replace` never hard-errors on a well-formed tag — and keeps the contract self-contained. (Chosen over refusing a colon-less `replace`, which would surface a confusing error for a valid tag.)
- **It returns the prior value** (or `None` when the namespace held nothing / was null), so the audit event logs prior + new **together in ONE record**: `action:"replace"`, `prior_value` (the evicted value, or `null` when nothing was evicted), `new_value`. A `replace` is therefore **exactly one audit event**, never two. `prior_value` is always emitted on `replace`; it is non-null whenever a prior existed in the namespace, `null` only in the degenerate no-prior case (ADR-009).
- **Partial failure leaves tags unchanged:** the single transaction rolls back atomically (AC-03).

`add` and `remove` are separate first-class actions writing a single row each (ADR-001); `replace` is the only action that touches two rows, and it does so atomically.

### Consequences
- Easier: status transitions are crash-safe and legible as a single audit record — the append-only history shows one `replace` event with both `prior_value` and `new_value`, not two disconnected rows.
- Easier: the future `single_value` CONFIG (deferred) retrofits with zero new action surface — it merely makes `replace` the server-chosen default for a configured prefix; the `replace` action, transaction shape, and audit contract are already here.
- Cost: the store primitive must guarantee the DELETE + INSERT share one `txn` handle; the namespace-scoped DELETE (`LIKE 'namespace:%'`) must not regress into the DELETE-all pattern of `write.rs:161`.
- Cross-references ADR-001 (the single-row primitives), the audit-event contract (ADR-009).
