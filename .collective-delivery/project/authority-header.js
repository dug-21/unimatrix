'use strict';

// C2 — README authority header + per-fact local-source-path lines, and the Program-owned
// EXTENSIONS placeholder. Both are DETERMINISTIC fixed text: no wall-clock, no release
// identity, no content digest, no graded value of any kind (DR-10/DR-11). The README
// names, for each projected fact, its LOCAL source path WITHIN this `.collective/`
// surface — it points the reader at where each fact lives, and never restates the
// adopted release identity or digest (those are sole-sourced from ADOPTED.yaml).
//
// The text deliberately carries none of C7's forbidden markers, no `col-*` token, and no
// `product/features/` path, so the boundary check (C7) passes on the composer's output.

// buildReadme() -> string. Stable across every composition.
function buildReadme() {
  return [
    '# Collective participation surface',
    '',
    'This `.collective/` directory is the collective\'s projected participation surface',
    'for this Program. Files marked `authority: baseline` are collective-owned contract',
    'content; a Program changes that content only by adopting a new Release, never by',
    'editing it in place (see `contract/OBLIGATIONS.yaml`, OBL-5E08). `EXTENSIONS.md` is',
    'Program-owned and is not collective-authoritative.',
    '',
    'Authoritative local source of each fact (path within this directory):',
    '',
    '- Adopted Release identity and content digest: `ADOPTED.yaml` — the sole record of',
    '  what this Program has adopted. No other file in this surface restates it.',
    '- Per-file digests and authority marking of this surface: `MANIFEST.yaml`.',
    '- Admitted participation vocabulary, term registers and remainder causes:',
    '  `contract/VOCABULARY.yaml`.',
    '- Retained obligations: `contract/OBLIGATIONS.yaml`.',
    '- Operating contract prose: `contract/CONTRACT.md`.',
    '- Classified Program registry subset: `contract/REGISTRATIONS.yaml`.',
    '',
    'This surface carries only classified, publishable material. It states no currency,',
    'drift, proposal, or adoption-status value; the adopted Release is read solely from',
    '`ADOPTED.yaml`, and only a Program\'s own merge makes this surface an adoption record.',
    '',
    'Local integrity checks:',
    '',
    'Compare each file listed in `MANIFEST.yaml` with its recorded per-file digest.',
    'A match establishes consistency with that manifest. It does not independently',
    'authenticate the files if the manifest itself has also been changed.',
    '',
    'The adopted Release content digest covers the full released file tree, which this',
    'projection does not contain. It cannot be recomputed from `.collective/` alone.',
    'The passing pre-admission verdict is carried in the adoption proposal\'s review',
    'description, outside this directory. Compare its Release identity and content',
    'digest with `ADOPTED.yaml` to check that binding; this comparison does not',
    'recompute the Release content digest.',
    '',
  ].join('\n');
}

// buildExtensions() -> string. Program-authority placeholder; not collective-authoritative.
function buildExtensions() {
  return [
    '# Program extensions',
    '',
    'This file is Program-owned and is **not** collective-authoritative. A Program may',
    'record its own local names and detail here, and may extend an admitted term with',
    'Program-specific detail, without weakening any admitted collective meaning (see',
    '`contract/OBLIGATIONS.yaml`, OBL-91D4). The collective projects this file as an',
    'empty placeholder and neither reads nor governs its contents.',
    '',
  ].join('\n');
}

module.exports = { buildReadme, buildExtensions };
