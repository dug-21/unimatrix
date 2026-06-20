# Test Plan — `README.md` bundle-example fix

> Component: converge EVERY `init --remote unimatrix-bundle:<blob>` / `init --remote <bundle>`
> bundle-fed-to-`--remote` form (lines ~123, ~587/130, and any others) on canonical
> `init --bundle <blob>`; mark `--remote` legacy. Risks: R-10 (High), R-09, R-16 (legacy marker).
> AC: AC-02. NOT in scope: `README:62` ONNX (owned by #767).

## Test Vehicle

Grep assertions over `README.md` (pre-merge-provable). Multi-occurrence, regex-based — NOT
line-pinned (R-10's core failure is converging line 123 but missing 587/130 or a third occurrence).

## R-10 / AC-02 — exhaustive multi-occurrence convergence (grep)

- `test_readme_zero_remote_bundle_blob`: zero occurrences of `init --remote unimatrix-bundle:`
  anywhere in README (regex, all lines).
- `test_readme_zero_remote_bundle_generic`: zero `init --remote <bundle>`-style forms where a bundle
  blob is fed to `--remote` (covers the line 587/130 phrasing). The grep covers BOTH known phrasings;
  a single-line check is insufficient.
- `test_readme_canonical_bundle_present`: canonical `init --bundle <blob>` IS present in README.

**Coverage requirement:** the AC-02 grep is multi-occurrence + regex (not line-pinned); zero broken
bundle-via-`--remote` forms remain; canonical form present. A THIRD undiscovered phrasing must also
be caught — assert via the regex, not enumerated line numbers.

## R-09 / AC-02 — no `--slug` with `--bundle`

- `test_readme_no_slug_with_bundle`: zero occurrences of `--slug` paired with `--bundle` in README
  (consistent with `docs/client-setup.md`; `init.js:353`).

## R-16 (accepted residual) / AC-02 — legacy `--remote` MARKED legacy

- `test_readme_remote_marked_legacy`: the `--remote <url> --token <tok>` form in README carries an
  explicit **"legacy"** marker. The only owed R-16 mitigation; no `--remote` round-trip owed.

## R-15 cross-ref — scope fence

- `test_readme_onnx_untouched` (inspection): `README:62`'s ONNX claim is NOT modified (owned by
  #767; out of nan-020 scope). Confirms the rewrite stayed inside its blast radius.

## Self-Check

- [x] Multi-occurrence regex greps (not line-pinned) for both broken phrasings — R-10.
- [x] Canonical `init --bundle <blob>` present — AC-02 positive.
- [x] No `--slug` with `--bundle` — R-09.
- [x] `--remote` MARKED legacy — R-16 sole mitigation; no round-trip owed.
- [x] `README:62` ONNX left untouched (scope fence).
