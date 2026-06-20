# Component: README.md — converge bundle examples onto init --bundle <blob>

> Content edit, not algorithm. Binding shared contract: OVERVIEW §H. Same canonical form +
> legacy marking as `docs-client-setup.md` — the two MUST agree.

## Purpose

Converge EVERY README occurrence of a bundle fed to `--remote` onto the canonical
`init --bundle <blob>` (no `--slug`); mark the legacy `--remote <url> --token` form "legacy"
(FR-4/FR-5/AC-02). Resolves OQ-B exhaustively.

## ENUMERATION — every occurrence (grepped this design pass; OQ-B was non-exhaustive)

OQ-B named two lines (123 and 587/130). Design grep found **FOUR** bundle-via-`--remote`
occurrences plus the legacy form. All listed so the fix is exhaustive (R-10):

| Line | Current text (verbatim fragment) | Action |
|------|----------------------------------|--------|
| **123** | `npx @dug-21/unimatrix init --remote unimatrix-bundle:<blob>` | **CONVERGE** → `npx @dug-21/unimatrix init --bundle <blob>` (the AC-02 broken example) |
| **130** | "re-run `init --remote` on each client with the new bundle" | **CONVERGE** → "re-run `init --bundle <blob>` on each client" (cert-rotation prose, bundle context) |
| **585** | "Wired automatically into `.mcp.json` by `init --remote <bundle>`" (mcp-bridge row) | **CONVERGE** → "…by `init --bundle <blob>`" |
| **587** | "Consume the bundle on the client with `init --remote <bundle>`." (client-bundle row) | **CONVERGE** → "Consume the bundle on the client with `init --bundle <blob>`." |
| **113** | `npx @dug-21/unimatrix init --remote https://uni.example.com --token <token>` | **KEEP but MARK LEGACY** — this is the legitimate legacy `--remote <url> --token` form, NOT a bundle. Do NOT converge; add a "legacy" annotation (see below). |

> Line ~116 prose already says "The legacy `--remote`/`--token` env-HTTPS path …" — good; the
> EXPLICIT "legacy" marker on the line-113 example itself should be added/strengthened so the
> AC-02 "legacy marked" inspection passes on the example, not only buried in prose.

## Edit plan

1. **Lines 123, 130, 585, 587:** replace the `--remote <bundle>` / `--remote unimatrix-bundle:`
   forms with `--bundle <blob>`. No `--slug` added (R-09). Keep surrounding narrative intact.
   - Line 118 heading "Bundle-driven attach (pinned TLS)" stays; only the fenced example at
     123 changes.
2. **Line 113 (legacy):** keep the `--remote <url> --token` example; add an explicit "legacy"
   label adjacent to it (e.g. an inline note or a "(legacy)" tag on the heading/line) so a
   reader sees `--remote` is the legacy path and `--bundle` is canonical (AG-1/R-16). Do not
   delete it (both modes are documented — FR-4).
3. **Out of scope (do NOT touch):** `README:62` ONNX claim (owned by #767); README's other
   self-healed defects.

## Verification grep (AC-02 / R-10 — multi-occurrence, regex, not line-pinned)

| Must reach ZERO | Grep |
|-----------------|------|
| bundle fed to `--remote` (both phrasings) | `grep -nE 'init --remote (unimatrix-bundle:|<bundle>)' README.md` → none |
| `--slug` paired with `--bundle` | `grep -nE '--bundle.*--slug|--slug.*--bundle' README.md` → none |

| Must be PRESENT | Grep |
|-----------------|------|
| canonical form | `grep -n 'init --bundle <blob>' README.md` → ≥1 |
| legacy `--remote` form retained + labeled | `grep -n 'init --remote .*--token' README.md` → present, with "legacy" nearby |

## Constraints / gotchas

- The two doc files (`README.md`, `docs/client-setup.md`) MUST use the identical canonical
  string and identical legacy marking — they are the same contract (OVERVIEW §H).
- Do not introduce `--slug` on any `--bundle` example (R-09).
- `client-bundle <slug>` (server-side) is correct and untouched — `--slug` retirement applies
  ONLY to the `init --bundle` path, not to `client-bundle`/`project register` (R-09 sc.4).

## Key test scenarios (hints)

- Both zero-occurrence greps above return nothing.
- Both presence greps return ≥1.
- Lines 585/587 (CLI-table rows) read `init --bundle <blob>`, not `--remote <bundle>`.
- Line 113 legacy form retained AND labeled legacy.
