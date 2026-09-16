## ADR-001: Persist `source_domain` at ingest, stamped provider-first, fail-loud

### Context
AC-03 / SR-07 / SR-11 require that a stored, queryable OpenCode record shows `source_domain=opencode`,
not the `claude-code` hook-path default. Codebase verification shows this is impossible with the
current mechanism:

- The `observations` table (`crates/unimatrix-store/src/db.rs`) has **no** `source_domain`,
  `provider`, or `model` column. Columns are `id, session_id, ts_millis, hook, tool, input,
  response_size, response_snippet, topic_signal, phase, topic_source`.
- `source_domain` is derived at **read** time from `event_type` via
  `DomainPackRegistry::resolve_source_domain(event_type)` (`unimatrix-observe/src/domain/mod.rs:180`),
  falling back to `DEFAULT_HOOK_SOURCE_DOMAIN = "claude-code"` (`services/observation.rs:572`).
- OpenCode emits the **same canonical event names** as claude-code (`PreToolUse`, `PostToolUse`, …).
  Event-derived resolution therefore cannot distinguish OpenCode from claude-code; a domain pack that
  claims the same `event_types` collides non-deterministically (EC-07). `--provider opencode` sets only
  the in-flight `ImplantEvent.provider`, which is **dropped** at the INSERT (no column).

So a provider flag + a domain pack yields "provider stamped, source_domain still `claude-code`" —
accepted-but-inert, green-but-holed. This is SR-11 precisely.

### Decision
Make attribution a **persisted, ingest-time** property, not a read-time inference:

1. Add a nullable `source_domain TEXT` column to `observations` (`db.rs` CREATE + `migration.rs`
   ALTER with `pragma_table_info` idempotency pre-check, per pattern #1264; bump to the **next
   sequential schema version, resolved against `CURRENT_SCHEMA_VERSION` at delivery** — 32 as of
   `migration.rs:26` today. The migration is version-independent (`ALTER observations ADD source_domain,
   model_id`); do NOT hardcode a number — a concurrent in-flight PR may advance `CURRENT_SCHEMA_VERSION`
   before vnc-049 lands).
2. On the write path (`uds/listener.rs` `insert_observation:3383`, `insert_observations_batch:3416`)
   stamp `source_domain` from `ImplantEvent.provider` **only when `provider == "opencode"`** →
   `"opencode"` (DECIDED opencode-only, 2026-09-15 human ruling — see Consequences). For every other
   provider (`claude-code`, `gemini-cli`, `codex-cli`) and for `None`, leave the column **NULL**; the
   read path (step 3) then falls back to today's read-derived resolution, preserving existing behavior
   **byte-for-byte** (zero T-SEC-12/13 change). The provider is the authoritative harness signal (it
   comes from `--provider`, not from the spoofable event name).
3. Read path (`services/observation.rs` `parse_observation_rows`, store SELECTs in
   `unimatrix-store/src/observations.rs`) **prefers the stored column** when non-NULL; only when NULL
   (pre-vnc-049 rows) does it fall back to today's `resolve_source_domain`/`DEFAULT_HOOK_SOURCE_DOMAIN`.
   Backward compatible; no backfill required.
4. **Fail-loud, never silent-default** (SR-11): an event carrying `provider=opencode` that reaches the
   write path with no derivable `source_domain` is an error/`debug_assert` canary (mirroring the
   vnc-013 `debug_assert!(event.provider.is_some())` pattern, ADR #4306), not a quiet fall to
   `claude-code`.

The `opencode` `DomainPack` (ADR-004 / `domain/mod.rs`) is still registered for category registration
and for resolving legacy NULL rows, but it is **not** the AC-03 mechanism — persistence is.

### Consequences
Easier: AC-03 is asserted on the **stored** record from a real OpenCode event, not a seam; the
event-name collision disappears for hook-path records; the model carrier (ADR-002) rides the same migration.
Harder: a schema migration + write/read plumbing is larger than "add a flag" (real blast radius —
db.rs, migration.rs, listener.rs, observation.rs, observations.rs).

**Stamp scope — DECIDED opencode-only (2026-09-15 human ruling; resolves OQ-2 / risk R-05 / the vision
WARN).** The write-time stamp applies only to `provider == "opencode"`; `claude-code`/`gemini-cli`/
`codex-cli` rows stay NULL and read back via today's read-derived resolution — existing behavior
preserved **byte-for-byte**, zero T-SEC-12/13 regression. `model_id` (ADR-002) is naturally
opencode-only. Generalizing the persisted-at-ingest stamp to all harnesses is a **different outcome**
(blast radius on integrity goal #5681 / C11 / T-SEC-12/13 for harnesses this feature isn't about), not
C18's last mile — no C18 AC needs it. If ever wanted, it earns its own cycle with a T-SEC re-baseline
and an explicit intended-semantic-change assertion, never a side effect here. The generalization door
stays open (the read path already prefers stored-when-non-NULL).
