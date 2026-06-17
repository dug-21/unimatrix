## ADR-002: The `v:2` Bundle Carries Server-Composed MCP and Observe URLs — Atomic Dual-Side Change (RD-2, SR-02)

### Context

The `v:1` bundle (vnc-034 ADR-001 / #4954) is `{v, base_url, token, fp}` — cloud-wide and slug-free, encoded as `unimatrix-bundle:<base64url(json)>`. The Rust side is the sole encoder (`encode_bundle`, `client_bundle.rs:211`); the JS side decodes (`decodeBundle`, `lib/hook-client/bundle.js:58`). Both enforce a **strict exact-key schema** — Rust `validate_schema` (`client_bundle.rs:268`) and JS `keysAreExactly(EXPECTED_KEYS)` (`bundle.js`) reject any missing/extra key. The JS side also pins `obj.v !== 1`.

Because the bundle is slug-free, the client must apply route grammar to build the actual endpoint (ADR-001 sites C-1/C-2/C-3) — the exact #766 failure spot. RD-2 (superseding the earlier "single slug field" lean) resolves: the bundle carries the **fully-formed MCP URL AND observe URL the server composed**, not a bare slug. A bare slug would still leave the client applying grammar.

SR-02 rates this High: the strict exact-key guard means a naive field add breaks decode unless **both sides + the parity corpus move atomically**. Known mechanics traps (#4956): the parity corpus uses **hex, not base64** (the `unimatrix-server` crate has no base64 dev-dep available to tests), and `pub(crate)` oracle fns must be re-exported at the module surface for the `tests/` integration crate to reach them.

### Decision

**Introduce `v:2` as one atomic Rust+JS+corpus change. The `v:2` bundle is a per-project artifact carrying the two finished endpoint URLs the server composed.**

`v:2` schema (exact keys):

```json
{"v":2,"mcp_url":"https://cloud.example:8443/v1/myproj","observe_url":"https://cloud.example:8443/v1/myproj/observe","token":"<64-hex>","fp":"sha256:<64-hex>"}
```

- `base_url` is **replaced** by `mcp_url` and `observe_url` — both fully-formed, server-composed, and slug-bearing. `token` and `fp` are unchanged.
- The server composes both URLs from one helper that owns route grammar: `mcp_url = {public_base}/v1/{slug}`, `observe_url = {public_base}/v1/{slug}/observe`, where `{public_base}` comes from the existing `derive_public_url(&Env::from_process())` (`client_bundle.rs:133`) and `{slug}` is the registered project. This helper is the SAME route grammar the server routes through (ADR-004) — composition and routing share one source so the bundle URL can never disagree with the live route (the dumb-client invariant's server half, ADR-001).
- **Per-project, not cloud-wide.** `client-bundle` now takes the project slug and emits that project's bundle. (vnc-034's cloud-wide slug-free bundle is retired — RD-5 deletes the default it relied on.)

**Atomicity (the SR-02 guard):**
1. **Rust encoder** (`encode_bundle`) and `Bundle` struct change to the `v:2` field set in the same change; `validate_schema`'s exact-key set becomes `{v, mcp_url, observe_url, token, fp}`; `BUNDLE_VERSION` → `2`.
2. **JS decoder** (`decodeBundle`) `EXPECTED_KEYS` and the `obj.v !== 2` check move in the same change; it validates `mcp_url`/`observe_url` are `https://` strings and returns `{v, mcp_url, observe_url, token, fp}`. The 4 KB raw-length cap (GUARD 1) and guard ordering (length → scheme → base64url → JSON → strict schema) are preserved exactly — length cap still runs first on the raw paste.
3. **Parity corpus** is updated in the SAME change, reusing the existing codec corpus infra (#4956): hex-encoded fixtures (not base64), oracle fns re-exported at the module surface for the `tests/` crate. The corpus pins `v:2` encode/decode byte-equality on both sides. No new isolated corpus is scaffolded.

**Hard cut, no `v:1` compatibility.** There is no dual-version decode path. The strict `obj.v` check rejects `v:1` outright (RD-1/RD-5: no existing users to migrate; keeping `v:1` would preserve the slug-free-bundle hole). A `v:1` paste fails loud with "unsupported bundle version."

**URL-length headroom.** The two URLs grow the populated bundle from ~340 chars to roughly ~440–520 chars depending on host/slug length; still an order of magnitude under the 4 KB raw cap, so GUARD 1 is unaffected.

### Consequences

- **Easier:** The client substitutes nothing — it posts MCP to `mcp_url` and observe to `observe_url` byte-for-byte (ADR-001 invariant test). #766's bug class cannot reappear through the bundle.
- **Easier:** Route grammar has one server-side owner (the compose helper + `parse_project_key`); the bundle is a pure data carrier.
- **Easier:** Versioned cut (`v:1`→`v:2`) is clean — the strict version check makes a partial rollout fail fast and legibly rather than silently mis-decode.
- **Harder:** Rust, JS, and the corpus MUST land together; a partial change breaks decode (the SR-02 trap). This is enforced by treating `v:2` as a single atomic unit with the corpus update in-scope.
- **Harder:** `client-bundle` is now per-project (needs a slug argument) rather than emitting one cloud-wide bundle — a CLI surface change paired with RD-5's deletion of the cloud-wide default.

### Related

- ADR-001 (this feature): the dumb-client invariant the two finished URLs realize.
- ADR-003 (this feature): the per-slug `observe_url` route.
- ADR-004 (this feature): the route grammar the compose helper shares with `parse_project_key`.
- vnc-034 ADR-001 (#4954): the `v:1` slug-free bundle this supersedes — deprecated in Unimatrix via `context_correct` to this entry.
- #4956: the codec parity-corpus mechanics (hex, re-exported oracle fns) this reuses.
