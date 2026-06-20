# Component: docs/client-setup.md — full rewrite to the bundle/observe model

> This is a PROSE rewrite — content-structure, not algorithm. C-3 forbids generate-from-
> contract; this is a manual rewrite. Binding shared contract: OVERVIEW §H (canonical form,
> legacy marking). The executable-claim lines here are doc-tested by Gates 5–7.

## Purpose

Replace the obsolete curl-based / 501-until-W2-7 telemetry model with the current model:
`init --bundle` (vnc-034) wires the pure-JS HTTP hook client automatically; telemetry flows
over the per-slug route `POST /v1/{slug}/observe` (vnc-038). Both attach modes documented
correctly; legacy `--remote` MARKED legacy; verified-on footer added.

## What is WRONG today (the #768 drift to remove)

Current `docs/client-setup.md` (179 lines) contains the obsolete model:
- Line 3: "All three clients … use curl-based shell hooks for telemetry. No local binary…"
- Three near-identical `### Shell Hook (curl-based)` blocks (Claude Code / Codex / Gemini),
  each with `curl -s -X POST "${UNIMATRIX_URL}/observe"` (~lines 27–142).
- Six `501 / W2-7` callouts (the "returns 501 until W2-7 ships" Notes) at lines 35, 54, 85,
  98, 129, 142.
- The premise that "no local binary is required and curl shell hooks are the telemetry path."

## Removal checklist (AC-01 — verifiable by grep)

| Must reach ZERO | Grep |
|-----------------|------|
| literal `501` | `grep -c '501' docs/client-setup.md` → 0 |
| literal `W2-7` | `grep -c 'W2-7' docs/client-setup.md` → 0 |
| hand-rolled curl-to-observe hook blocks | no fenced block matching `curl .*/observe` |
| "no local binary required / curl-based shell hooks" telemetry premise | absent (FR-3) |

> The `curl https://<host>:8443/health` example (current line ~170) is a HEALTH check, NOT a
> `/observe` hook — it is legitimate narrative and may stay (it does not match `curl .*/observe`).

## Target structure (post-rewrite)

```
# Connecting a Client to a Remote Unimatrix Server   (title)

  Narrative intro: clients attach over HTTPS; telemetry via the init-wired pure-JS HTTP hook
  client (no curl scripts, no local platform binary). [prose]

## Prerequisites
  - Node >= 18 on the client machine; npx access to @dug-21/unimatrix. [prose]
  - A connection bundle emitted by the operator (see below). [prose]

## Attach modes

### Bundle attach (canonical)                                  [EXECUTABLE CLAIM section]
  - Operator emits on the server:   unimatrix client-bundle <slug>   [exec claim — Gate 5]
      Narrative: stdout is the opaque unimatrix-bundle: (v:2) blob; slug baked in. [prose]
  - Client attaches:   npx @dug-21/unimatrix init --bundle <blob>    [exec claim — Gate 6–7]
      *** NO --slug *** (retired on the bundle path; the blob encodes the slug). (FR-5/OQ-A)
      Narrative: init wires the JS HTTP hook client + the MCP bridge automatically; writes the
      out-of-tree credential to ~/.unimatrix/<projectHash>/remote.json (mode 0600). [prose]
  - Telemetry: the wired hook client POSTs to the server-composed /v1/<slug>/observe. [exec claim — Gate 7]

### Direct attach (LEGACY)                                     [MARKED LEGACY — NOT doc-tested]
  > **Legacy.** This mode is documented for completeness, is effectively unused, will not be
  > invested in, and does NOT support cloud MCP (bundle-only). Prefer bundle attach above.
  -   npx @dug-21/unimatrix init --remote <url> --token <tok>      [documented, not tested]
      Narrative: observe/telemetry only; emits the LEGACY_* migrate-to-bundle guidance. [prose]

## How telemetry works                                         [NARRATIVE PROSE — verified-on stamp]
  - The init-wired pure-JS hook client reads observe_url + fingerprint from the credential
    store and POSTs HookRequest frames over fingerprint-pinned HTTPS; fail-open. [prose]
  - Fingerprint pinning rationale; TLS-only 8443; GET /health unauthenticated. [prose]

## Certificate rotation                                        [NARRATIVE — link docs/cert-rotation.md]
  - Re-emit the bundle (client-bundle <slug>) and re-run init --bundle on each client. [prose]
      (Update the current line-130 "re-run init --remote" wording to "re-run init --bundle".)

## Verifying the connection                                    [NARRATIVE]
  - curl https://<host>:8443/health  (health check — legitimate, keep). [prose]

---
_Verified on v0.x.y_    <- single footer stamp; prose convention, NOT machine-checked (FR-6/D-3)
```

## Executable-claim vs narrative classification (ADR-003 worked example — R-08)

| Line | Classification | Guarded by |
|------|---------------|-----------|
| `unimatrix client-bundle <slug>` | executable claim | doc-test Gate 5 |
| `npx @dug-21/unimatrix init --bundle <blob>` | executable claim | doc-test Gate 6–7 |
| hook client POSTing `/v1/<slug>/observe` | executable claim | doc-test Gate 7 |
| fingerprint-pinning rationale, TLS/port notes, token-rotation | narrative prose | manual + verified-on stamp |
| `init --remote <url> --token` (legacy) | documented, NOT doc-tested (AG-1) | "legacy" label only |

Boundary discipline: do NOT add a doc-test gate per command; the tested set is the single
canonical chain. If a NEW non-reducible command is added, raise to design — do not leave untested.

## Constraints / gotchas

- No `--slug` paired with `--bundle` anywhere (AC-02 / R-09).
- `--remote` form must carry an explicit "legacy" marker in this file AND README (AG-1/R-16).
- Terminology (NFR-8): "Unimatrix", `context_*`, `/v1/{slug}/observe`, `client-bundle`,
  `init --bundle`.
- Do not invent flags/behavior — document only shipped surfaces (no CLI change in nan-020).

## Key test scenarios (hints — mostly grep/inspection)

- `grep -c -E '501|W2-7'` → 0; no `curl .*/observe` fenced block; obsolete premise gone.
- Positive: `init --bundle` and `/v1/{slug}/observe` present; `client-bundle <slug>` present.
- `--remote` present AND labeled legacy; zero `init --remote unimatrix-bundle:`; no `--slug`+`--bundle`.
- Single `_Verified on …_` footer present.
