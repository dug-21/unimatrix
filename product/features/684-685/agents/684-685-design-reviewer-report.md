# Design Review: 684-685-design-reviewer

Combined bugfix design review for GH#684 (schema nullable union) and GH#685 (harness ServerDied).

## Design Assessment

- **Bug 1 (#684): APPROVED** — test-only fix is correct; the union schema is spec-correct, truthful, and preserves AC-10's guard intent.
- **Bug 2 (#685): APPROVED WITH NOTES** — doc/Docker rename is correct; preflight check should be IN scope; one rename trap flagged.

## Verification Performed

- Read both investigator reports (GH comments) and the failing test (`crates/unimatrix-server/src/server.rs:3250-3304`), the 5 optional fields in `mcp/tools.rs`, and `mcp/serde_util.rs` (null acceptance confirmed at the visitor level).
- Confirmed stale artifact: `target/release/unimatrix-server` mtime Mar 12 (20.7 MB) vs current `target/release/unimatrix` (Jun 5). Cargo.toml: package = `unimatrix-server`, bin = `unimatrix`.
- Confirmed all stale references: `USAGE-PROTOCOL.md:11,36,92`, `Dockerfile:37,41`, `docker-compose.yml:14`. `conftest.py` auto-detection already resolves the correct name.
- Counted smoke marks: exactly 23 across `suites/` — the investigator's 23/23 green run was the full smoke set.

## Findings

### Bug 1 (#684)

1. **Non-blocking (but post a correcting comment on #684)** — The "Integration manifestation (same root cause)" comment claiming `test_get_with_string_id` / `test_deprecate_with_string_id` fail with -32602 under rmcp 1.7 is **refuted**. Both tests are `@pytest.mark.smoke` (`test_tools.py:1867,1900`) and therefore passed in the verified 23/23 run against the correct binary. Corroborating evidence: the reported error text `expected i64` matches a plain serde i64 deserializer — the current visitor's expecting string is "an integer or a string containing a base-10 integer". The -32602 run hit the stale 0.1.0 binary (built Mar 12, **before** vnc-012 added the coercion deserializers on Mar 29). String coercion lives in `serde_util.rs`, not rmcp; there is no rmcp-1.7 runtime coercion regression. The fix plan must NOT add any production change for this comment, and the comment should be corrected on the issue to prevent future misdiagnosis.
2. **Non-blocking** — When updating the test, keep per-field assertion messages and note the rmcp-version coupling in the comment block (planned). Exact-equality against `json!(["integer","null"])` is fine; schemars emission order is deterministic.
3. **(a) Test-update vs schemars-attribute: test-update is correct.** Forcing `#[schemars(with = "i64")]` on the 5 Option fields would advertise that `null` is invalid while `deserialize_opt_*` accepts it, would diverge from rmcp's MCP-spec alignment, and would not even restore the 0.16 wire shape (0.16 also emitted `nullable: true`). AC-10's intent (vnc-012 ADR-002) survives: a typo'd `with` attribute still emits `{}` and still fails the per-field assertion.
4. **ADR maintenance done** — ADR-002 (#3788) stated the snapshot test asserts bare `"integer"` for all nine fields; that is stale since vnc-023 merged. Corrected to entry **#4732** documenting the rmcp-version-dependent shape and the do-not-force-bare-type rule.

### Bug 2 (#685)

1. **Non-blocking (trap — flag to implementer)** — `Dockerfile:15` `cargo build --release --package unimatrix-server` must **NOT** be renamed: the *package* is still `unimatrix-server` (`crates/unimatrix-server/Cargo.toml:2`); only the `[[bin]]` target is `unimatrix`. Only lines 37/41 (COPY/ENV) change. An over-zealous global rename breaks the docker build.
2. **(b) Preflight check: IN scope.** The bug is environmental; without it the PR is doc-only with no regression guard — the preflight IS this bug's missing test, and test infra is cumulative per project rules. Keep minimal: in the session-scoped binary fixture, run the binary once and assert the reported version matches the workspace version (or at minimum that `serve` appears in `--help`). Implementer must verify the actual version invocation (`version` subcommand vs `--version`) rather than assume.
3. **(c) Artifact deletion: not a PR change.** `target/release/unimatrix-server` is untracked build output — it cannot appear in a diff. Delete locally, note in the PR description. The preflight check is the durable guard for other machines and CI caches holding the fossil.
4. **Non-blocking** — `ServerDied` hardening: the class already captures stderr (`client.py:50-54`, `self.stderr_output`); only the message changes. Bound the included stderr (e.g., last 3 lines) to avoid unbounded exception messages. Recommended — its absence directly caused the original misdiagnosis.
5. **Non-blocking, optional** — `client.py:58` docstring still says "unimatrix-server subprocess"; cosmetic consistency.
6. **Concur with investigator**: do NOT change the launch command — bare invocation is bridge mode (`main.rs:172-190`), not the stdio server. The issue's original suggestion would have introduced the real regression.

### Cross-cutting

- **Hot-path risks**: none. All changes are tests, docs, packaging; the client.py change is exception-path only.
- **Blast radius**: #684 worst case = miscategorized field → loud test failure, zero production effect. #685 worst case = Dockerfile typo → build fails at COPY (immediate); compose env mismatch → caught by the preflight.
- **Security**: no new trust boundaries. stderr in exception messages surfaces server logs in pytest output — local harness only, acceptable.
- **Out of scope, concur**: PR CI lacks `cargo test --workspace` (how vnc-023 merged red). File as a separate nan-phase item; do not bundle here.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 #3788 (AC-10 origin, decisive for assessing test-update vs attribute), lessons #4730/#4731 (already stored by investigators), patterns #3813/#3814 (deserialize_with pairing + string-id integration test mandate), #4699 (rmcp migration surface).
- Stored: entry #4732 "ADR-002: Preserve integer JSON Schema via #[schemars(with = \"T\")]" via context_correct of #3788 -- amended the stale snapshot-test contract to document rmcp>=1.7 nullable-union shape and the do-not-force-bare-type rule. No new ADR needed: both fixes follow existing decisions; investigator lessons #4730/#4731 cover the failure modes.
