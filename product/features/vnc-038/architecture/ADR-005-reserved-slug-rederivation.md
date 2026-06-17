## ADR-005: Reserved-Slug Re-Derivation Under the New Route Grammar (SR-05)

### Context

`RESERVED_SLUGS = ["v1", "health", "observe", "tools"]` (`config.rs:2483`), checked by `is_reserved_slug` (`config.rs:2490`) at `validate_slug` (`projects.rs:208`) and `validate_projects_config` (`config.rs:2524`). The set exists so a registerable slug cannot shadow a route literal.

The reserved set is **downstream of the route grammar** — it is not an independent constant. Deleting the `/v1/tools → Default` alias (ADR-004) and adding the per-slug `/v1/{slug}/observe` route (ADR-003) shift what each literal must reserve. SR-05: a missed update lets a registerable slug shadow a route (e.g. registering `tools` if it stops being reserved while still meaning something), or needlessly forbids a slug that is now safe. This must be an **explicit design consequence**, not an incidental edit.

### Decision

**Re-derive `RESERVED_SLUGS` from the post-cutover grammar. The set is determined by which literals, if used as a slug, would collide with a server-owned route or path segment.**

Per-literal analysis under the new grammar (`/v1/{slug}/...` only, no default alias):

| Literal | vnc-034 reason | Post-cutover status | Decision |
|---------|----------------|---------------------|----------|
| `v1` | URL version prefix | Still the fixed first path segment; a `/v1/v1/...` slug request is confusing and reserves the version namespace | **Keep reserved** |
| `health` | top-level `/health` route | `/health` stays top-level (store-independent); a `health` slug would not collide with it but the analogous confusion remains and it is cheap to keep | **Keep reserved** |
| `observe` | top-level `/observe` route | `/observe` is NO LONGER top-level; observe is now `/v1/{slug}/observe` (ADR-003). `observe` as a *slug* would route `/v1/observe/observe`. The reservation REASON changes — it is no longer "shadows the top-level route" but "the literal `observe` is a reserved per-slug sub-route segment." Reserving the slug `observe` prevents the ambiguous `/v1/observe/...` namespace. | **Keep reserved — new rationale** |
| `tools` | the `/v1/tools/...` default alias | The default alias is DELETED (ADR-004). `tools` is no longer a route literal in the slug position — `/v1/tools/...` now means "the project whose slug is `tools`," which is unambiguous and safe. | **Candidate to UN-reserve** |

**`tools` un-reservation is the load-bearing change.** With the default alias gone, `tools` is an ordinary slug. The conservative default is to **keep `tools` reserved** to avoid surprising any operator/doc that still associates `/v1/tools/...` with "the tools endpoint" during the #768 doc fast-follow window, and because un-reserving is a one-line follow-up if a real project genuinely needs the slug `tools`. The spec MUST make this an explicit, tested decision rather than letting the constant drift.

**Resulting set:** `RESERVED_SLUGS = ["v1", "health", "observe", "tools"]` is **retained as-is in value**, but its *derivation* changes: `observe` is reserved as a per-slug sub-route segment (not a top-level route), and `tools` is reserved by conservative choice (not because it is still a default alias). This is documented in the constant's doc-comment so a future reader does not re-derive it wrong.

**Explicit acceptance tie (SR-05).** An acceptance test asserts slug registration is rejected for every name in `RESERVED_SLUGS`, AND a per-slug `observe` sub-route reachability test confirms `/v1/{slug}/observe` resolves while `/v1/observe/observe` (slug `observe`) is unregisterable. This binds the reserved set to the live grammar so a future grammar change cannot silently desync the set.

### Consequences

- **Easier:** The reserved set has a documented derivation rule tied to the grammar, so future route changes have a checklist instead of a guess.
- **Easier:** Keeping the value stable (no removal) minimizes blast radius during the hard cutover and the #768 doc window.
- **Harder:** `observe`'s reservation rationale is now subtler (a sub-route segment, not a top-level route); the doc-comment must carry that or a future reader mis-reasons about it.
- **Harder:** If a real project ever needs the slug `tools`, a deliberate un-reservation + test update is required — a known, cheap follow-up, not a blocker.

### Related

- ADR-003 (this feature): the `/v1/{slug}/observe` route that re-bases `observe`'s reservation rationale.
- ADR-004 (this feature): deleting the `/v1/tools → Default` alias that re-bases `tools`'s reservation rationale.
- vnc-034 ADR-004 (#4951): the `ProjectSlug` allowlist and reserved-slug check this re-derives.
