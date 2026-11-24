'use strict';

// C2 — build/emit the projection manifest (`collective.projection-manifest/1`,
// `.collective/MANIFEST.yaml`). It records, for each OTHER projected file, its
// `.collective/`-relative path, its sha256 digest, and its authority
// (`baseline` for collective-owned content, `program` for `EXTENSIONS.md`). It EXCLUDES
// itself (Contract §3.2). Because each `authority: baseline` `contract/` copy is
// byte-identical to its released file, its digest here EQUALS the digest in the released
// MANIFEST.yaml (DR-02). The manifest carries NO release identity or content digest —
// the graded value is confined to ADOPTED.yaml (DR-10).
//
// `files` is sorted by path with Buffer.compare (Architecture: no map-ordering
// nondeterminism), so the same inputs yield byte-identical manifest bytes (DR-11).

const F = require('../release/source/format');

const MANIFEST_PATH = '.collective/MANIFEST.yaml';

// buildProjectionManifest(files) -> the manifest document object.
//   files : [{ path: '.collective/...', bytes: Buffer, authority: 'baseline'|'program' }]
//           — the projected set, WITHOUT the manifest itself.
function buildProjectionManifest(files) {
  const records = files
    .filter((f) => f.path !== MANIFEST_PATH)
    .map((f) => ({ path: f.path, digest: F.digest(f.bytes), authority: f.authority }))
    .sort((a, b) => F.order(a.path, b.path));
  return {
    schema: 'collective.projection-manifest/1',
    authority: 'baseline',
    files: records,
  };
}

module.exports = { buildProjectionManifest, MANIFEST_PATH };
