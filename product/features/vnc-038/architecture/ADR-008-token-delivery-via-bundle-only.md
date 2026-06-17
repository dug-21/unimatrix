## ADR-008: First-Boot Token Is Delivered Only via the v:2 Bundle — Never Emitted to Stdout/Logs (RD-1 carry-item #735 CI-1, drives AC-11/NFR-06)

### Context

`crates/unimatrix-server/src/http/token.rs:101` prints the bearer token to **first-boot stdout**. This dates from a pre-cloud posture where the operator read the token off the console. Under the cloud HTTPS posture (NFR-06) a token on stdout/logs is a credential-exposure surface: container logs are aggregated, persisted, and frequently shipped to third parties.

vnc-038 reworks first boot — first boot now comes up with nothing servable and a loud "register a project to begin," and the **remote client obtains its token via the `v:2` client bundle** (ADR-002: the server-composed MCP/observe URLs **and the token** travel inside the validated `unimatrix-bundle:...` payload). The token therefore no longer needs a stdout channel for the cloud/container HTTP surface — the bundle is already its delivery vehicle. This is the same first-boot surface vnc-038 is already rewriting, so the carry-item (#735 CI-1) lands here rather than as separate work. The open question is whether suppressing the stdout print is a real architectural decision or a one-line cleanup: it is a **decision**, because it commits the bundle as the *sole* token-delivery channel for the cloud surface and removes the previously-relied-upon console channel.

### Decision

**The first-boot bearer token MUST NOT be emitted to stdout or logs. The validated `v:2` client bundle is the sole token-delivery channel for the cloud/container HTTP surface.** The print at `http/token.rs:101` is redacted/gated so the token never reaches stdout or `tracing` output.

- **Sole channel = the bundle.** The token reaches the remote client only inside the strict-schema `v:2` bundle (`{v, mcp_url, observe_url, token, fp}`, ADR-002). No parallel stdout/log path exists. There is no fallback "also print it" mode — that would re-open the exposure NFR-06 closes.
- **Scoped to the HTTP/cloud first-boot surface.** This applies to the cloud/container HTTP deployment, which is the surface vnc-038 reworks and the one with a bundle. It is the natural and only token channel there.
- **Reconciled with local STDIO/UDS (the local-unaffected guarantee, ADR-006).** Local STDIO/UDS has **no bundle** — it opens its path-hash store directly at boot and threads the `Arc<Store>` to its handlers (ADR-006), and its token handling is part of that direct, unchanged local path. This decision is scoped to the cloud/container first-boot token print and **must not** alter how local obtains/uses its token. If `token.rs:101`'s print is reachable on the local path, the redaction/gating must be conditioned on the cloud/container first-boot context so the local surface is left functionally unchanged (consistent with AC-10's "local unaffected, no operator action"). Delivery confirms the print site is HTTP-first-boot-scoped; if shared, gate by deployment context rather than removing the local affordance outright.

### Consequences

- **Easier:** The cloud HTTPS posture (NFR-06) is honored by construction — there is one credential channel (the bundle) and it is already strict-validated, length-capped, and decoded only at the client trust boundary (ADR-001/002). No secondary surface to audit.
- **Easier:** One token story for cloud: "the token is in the bundle." Operators and docs stop relying on console scraping (and #768's doc fast-follow can state this cleanly).
- **Harder:** Loss of a debugging affordance — operators can no longer eyeball the token on the console. Recovery is via re-emitting/inspecting the bundle, not the log. Acceptable given the security posture.
- **Harder:** Delivery must verify the `token.rs:101` print is not load-bearing for the local STDIO/UDS surface before suppressing it (see Decision point 5 / ADR-006); a naive unconditional removal could regress local if that print is shared.

### Related

- ADR-002 (this feature): the `v:2` bundle that carries the token — the channel this ADR makes sole.
- ADR-006 (this feature): the local STDIO/UDS direct-binding / no-bundle path this ADR must not disturb.
- AC-11 / NFR-06 (SCOPE): the no-token-to-stdout requirement this ADR drives.
