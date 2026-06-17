## ADR-007: `register <slug>` Writes `[[projects]]` Routing Intent; Restart Applies — Atomic, Re-Attach-Safe, Binary-Only (RD-4, SR-07)

### Context

Today `register <slug>` (`projects.rs:264`) creates the per-slug data dir + store but only **prints** config instructions (`projects.rs:334-335`: `eprintln!("add to config.toml ... [[projects]] slug = ...")`). The operator must then hand-edit `config.toml` and restart. `[[projects]]` is read once at boot (`main.rs:627` via `load_config_and_build_allowlist`, then the resolver swap at `main.rs:1004`). This register→hand-edit→restart asymmetry is *why* the no-slug default exists — the real path is heavyweight (SCOPE Problem Statement, #5079).

`register` already has a correct State A/B/C model: State A (data + routed) = loud error; State B (data exists, not routed) = **re-attach (open), never genesis-clobber**; State C (no data) = genesis. The hash chain is sacred — a re-register must never clobber a preserved store.

RD-4: `register <slug>` WRITES the `[[projects]]` stanza instead of printing it; a restart applies it via the existing boot read. No dynamic registry, admin endpoint, or in-process reload. Same one command for project 1 and project N.

SR-07 rates this High: a malformed/partial config write or a non-idempotent re-register could corrupt routing or genesis-clobber an existing store. The distroless runtime has **no shell** — provisioning is Rust-binary-only.

### Decision

**`register <slug>` writes the `[[projects]]` routing intent to `config.toml` as an atomic, idempotent operation, preserving the State A/B/C store-safety model. A restart applies it through the unchanged boot read. The same command serves project 1 and project N.**

1. **Replace the print with a write.** The `eprintln!("add to config.toml ... [[projects]]")` lines at `projects.rs:303-304` (State B) and `projects.rs:335` (State C) are replaced by a function that **appends/ensures** the `[[projects]] slug = "<slug>"` stanza in `config.toml`. The print-instructions behavior is gone (AC-03).

2. **Atomic config write (SR-07).** The write is **read-modify-write to a temp file + atomic rename** over `config.toml`:
   - Parse the existing `config.toml` (preserve all other config).
   - If the `[[projects]]` stanza for `<slug>` is already present, the write is a **no-op** (idempotent — re-register does not duplicate the stanza).
   - Otherwise add the stanza.
   - Serialize to a temp file in the same directory, `fsync`, then `rename` over `config.toml` (atomic on the same filesystem). A crash mid-write leaves either the old or the new complete file — never a partial/corrupt config (SR-07).
   - All via the Rust binary (`std::fs` + the existing TOML lib) — no shell, distroless-safe.

3. **Store-safety model unchanged (hash chain is sacred).** The State A/B/C branching is preserved and the config write is layered onto it:
   - **State C (fresh):** create dir + genesis store, THEN write the stanza. (Order: store first, then routing intent — a stanza without a store would route to a missing store; the boot read fails loud, but store-first avoids it.)
   - **State B (re-attach):** OPEN the preserved store (never genesis), THEN write the stanza. Re-attaching an existing slug re-routes to the preserved hash chain — never clobbers it.
   - **State A (data + already in `[[projects]]`):** loud error, no write, no clobber.
   This means `register` against an existing slug **re-attaches and re-routes** (open, never genesis) per the State B precedent (SR-07, hash-chain constraint).

4. **Restart applies (RD-4).** No live reload. After `register` writes the stanza, the operator restarts the daemon; the existing boot read (`main.rs:627`/`:1004`, now the unified resolver per ADR-004) picks up the new slug. Restart-to-apply is accepted for this single-dev, not-always-on deployment (SCOPE assumption). No admin endpoint, no in-process registry — the prior "live-reload / central build risk" is removed.

5. **Uniform command (Goal 2/4).** Project 1 and project N are the IDENTICAL `register <slug>` invocation. There is no first-project special path: empty `[[projects]]` + first `register` writes the first stanza exactly as the Nth does. This is what eliminates the zero-step-default-vs-heavyweight-Nth asymmetry (AC-02/AC-04).

### Consequences

- **Easier:** One command provisions any project, with no hand-edit of `config.toml` (AC-02/03/04). The asymmetry that justified the no-slug default is gone, so deleting the default (ADR-004) does not worsen devex.
- **Easier:** Atomic temp+rename makes a partial/corrupt config impossible; idempotent re-register is safe (SR-07).
- **Easier:** State B re-attach preserves the hash chain on re-register — no genesis-clobber path (hash-chain constraint).
- **Easier:** No live-reload mechanism to build — the largest de-risking from RD-4; restart-to-apply reuses the existing boot read.
- **Harder:** Restart-to-apply is a real (accepted) cost — a newly registered project is not routable until the next restart. Acceptable only because the deployment is single-dev / not-always-on (SCOPE assumption; if that assumption fails, this is a regression).
- **Harder:** `register` now mutates `config.toml`, so it needs write access to the config path and must preserve unrelated config faithfully (read-modify-write, not blind append) — slightly more than the old print.

### Related

- ADR-004 (this feature): the unified resolver the boot read feeds; empty `[[projects]]` ⇒ nothing servable (first boot loud message).
- ADR-002 (this feature): `client-bundle <slug>` emits the per-project bundle for a registered slug.
- vnc-034 ADR-004 (#4951): the register/attach model and `ProjectSlug` allowlist this extends.
- #5079: the register-prints-not-writes / boot-read-once facts this resolves.
