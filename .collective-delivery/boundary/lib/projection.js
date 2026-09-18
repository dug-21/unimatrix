'use strict';
// col-005 C7 — Projection boundary classification (I-PROJCHK).
//
// Default-deny per field (ADR-016). Extends the col-004 tools/boundary/ base
// (lib/publication.js posture: exactly-one closed classification row per artifact,
// a closed public_fields allow-list, and forbidden_markers) with a projection-specific
// classification of the receiver `.collective/` surface (Contract §3).
//
// C7 is INDEPENDENT of the composer (C2): checkProjection takes the bundle itself and
// re-derives every fact from the bundle bytes + the release record. A leak produced by a
// buggy or adversarial composer is caught because C7 does not trust the generator (DR-09).
// It reuses only the format leaf primitives (strict YAML 1.2, UTF-8, safe-path, digest);
// it imports no composer/carrier/observe/registry code and mutates no accepted row.
const fs = require('node:fs');
const path = require('node:path');
const F = require('../../release/source/format');

const CLASSIFICATION_PATH = path.join(__dirname, '..', 'projection-classification.yaml');
const ADOPTED = '.collective/ADOPTED.yaml';
const PROJECTION_MANIFEST = '.collective/MANIFEST.yaml';
const escape = s => String(s).replace(/~/g, '~0').replace(/\//g, '~1');
// Same col-* posture the col-004 base applies: repository-local machinery never travels.
const COL_RE = /(?:^|[\s/`])col-[a-z0-9][a-z0-9-]*(?:\b|\/)/i;
// Document types that must never appear under `.collective/**`, regardless of filename
// (Contract §3 "never projected", §4.2 step 6). Detected by parsed `schema`, so renaming
// the file to an allowed path does not smuggle the content across.
const FORBIDDEN_DOC_SCHEMAS = new Set([
  'collective.admission-result/1',
  'collective.gate-verdict/1',
  'collective.gate-verdict/2',
  'collective.clearances/2',
  'collective.release/2',
  'collective.manifest/2',
  'collective.release-index/2',
  'collective.release-intent/1',
]);

function loadClassification(file = CLASSIFICATION_PATH) {
  const doc = F.parseDocument(fs.readFileSync(file)); // strict YAML, no schema binding
  if (!doc || !Array.isArray(doc.rows) || !Array.isArray(doc.forbidden_markers)) {
    throw new F.ReleaseError('projection-classification-invalid', file, 'Classification must carry rows[] and forbidden_markers[]', 3);
  }
  return doc;
}

// Read a `--bundle <dir>` directory as the receiver `.collective/` set; every relative
// path is reported under the `.collective/` prefix so it matches the closed classification.
function readBundle(dir) {
  const out = [];
  (function walk(rel) {
    for (const name of fs.readdirSync(path.join(dir, rel))) {
      const childRel = rel ? rel + '/' + name : name;
      const st = fs.lstatSync(path.join(dir, childRel));
      if (st.isDirectory()) walk(childRel);
      else if (st.isFile()) { F.safePath(childRel); out.push({ path: '.collective/' + childRel, bytes: fs.readFileSync(path.join(dir, childRel)) }); }
      else throw new F.ReleaseError('source-unavailable', childRel, 'Unsupported non-regular file in bundle', 3);
    }
  })('');
  return out;
}

// Structurally forbidden material recognised by path shape, with NO directory-based
// exception: the same basename is refused whether at the root or nested under an
// allowed-looking directory.
function forbiddenMaterial(p) {
  const segs = p.split('/');
  const base = segs[segs.length - 1];
  if (segs.some(s => /^col-[a-z0-9]/i.test(s))) return 'repository-local col-* artifact must never cross the boundary';
  if (segs.includes('release-control')) return 'release-control material must never cross the boundary';
  if (base === 'CLEARANCES.yaml') return 'clearance record (private receiver evidence) must never cross the boundary';
  if (base === 'RELEASE.yaml') return 'release-level record must never cross the boundary';
  if (/^RELEASE-INTENT/i.test(base)) return 'release-intent material must never cross the boundary';
  if (base === 'MANIFEST.yaml' && p !== PROJECTION_MANIFEST) return 'release-level manifest must never cross the boundary';
  if (/admission-result/i.test(base)) return 'admission-failure detail must never cross the boundary';
  if (/(gate-)?verdict/i.test(base)) return 'gate/admission verdict detail must never cross the boundary';
  return null;
}

// checkProjection(bundle, releaseRecord) -> { ok, violations[] }
//   bundle        : [{ path: '.collective/...', bytes: Buffer }] — as produced, or hand-built.
//   releaseRecord : the accepted index entry (release_identity + content_digest), for
//                   cross-checking projected facts against authority.
function checkProjection(bundle, releaseRecord, classification = loadClassification()) {
  const violations = [];
  const add = (p, reason, field) => violations.push(field ? { path: p, field, reason } : { path: p, reason });
  const byPath = new Map(classification.rows.map(r => [r.artifact, r]));
  const allowedPaths = new Set(classification.rows.map(r => r.artifact));
  const markers = classification.forbidden_markers;
  const gradedDigest = releaseRecord && releaseRecord.content_digest;
  const recordIdentity = releaseRecord && (releaseRecord.release_identity ?? (releaseRecord.metadata && releaseRecord.metadata.release_identity));

  for (const entry of bundle) {
    const p = entry.path;

    // (2) structurally-absent forbidden material — no directory-based exception.
    const fm = forbiddenMaterial(p);
    if (fm) add(p, fm);

    // (1) closed file set — only the eight classified projected paths may appear.
    if (!allowedPaths.has(p)) add(p, 'file not in the closed projected set');
    const row = byPath.get(p);

    // Decode + strict parse; a leak that fails to parse is a violation, never a crash.
    let text, doc = null;
    try { text = F.utf8(entry.bytes); } catch (e) { add(p, 'file is not valid UTF-8: ' + e.message); continue; }
    if (/\.ya?ml$/i.test(p)) { try { doc = F.parseDocument(entry.bytes); } catch (e) { add(p, 'unparseable strict-YAML document: ' + e.message); } }

    // Forbidden document type regardless of filename (schema-content check).
    if (doc && typeof doc === 'object' && FORBIDDEN_DOC_SCHEMAS.has(doc.schema) && (!row || row.schema !== doc.schema)) {
      add(p, 'forbidden document type (' + doc.schema + ') must never cross the boundary', '/schema');
    }

    // (4) graded-value confinement (DR-10): only ADOPTED.yaml may carry the release content digest.
    if (gradedDigest && p !== ADOPTED && text.includes(gradedDigest)) {
      add(p, 'graded release value (content_digest) is confined to ADOPTED.yaml (DR-10)');
    }

    // forbidden_markers + col-* scan on the raw bytes (reused from the col-004 base).
    for (const m of markers) if (text.includes(m)) add(p, 'forbidden private marker present: ' + m);
    if (COL_RE.test(text)) add(p, 'repository-local col-* reference present');

    if (!row) continue; // unknown path already flagged; no allow-list to walk.

    // Classified schema-id must be the one the row declares (belt-and-suspenders for §5).
    if (row.schema && doc && doc.schema !== row.schema) {
      add(p, 'document schema ' + JSON.stringify(doc.schema) + ' does not match its classification ' + row.schema, '/schema');
    }

    // (3) per-field default-deny walk over the closed allow-list.
    const allowed = new Set(row.public_fields);
    const value = row.kind === 'text' ? { text } : doc;
    if (value === null || typeof value !== 'object') continue;
    (function walk(v, ptr) {
      if (ptr && !allowed.has(ptr)) { add(p, 'unclassified field denied by default (default-deny per field)', ptr); return; }
      if (Array.isArray(v)) v.forEach(x => walk(x, ptr + '/*'));
      else if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) walk(x, ptr + '/' + escape(k));
    })(value, '');

    // (5) authority cross-check: ADOPTED.yaml must agree with the release record.
    if (p === ADOPTED && doc && doc.adopted_release) {
      if (gradedDigest && doc.adopted_release.content_digest !== gradedDigest) add(p, 'adopted content_digest disagrees with release authority', '/adopted_release/content_digest');
      if (recordIdentity && doc.adopted_release.release_identity !== recordIdentity) add(p, 'adopted release_identity disagrees with release authority', '/adopted_release/release_identity');
    }
  }
  return { ok: violations.length === 0, violations };
}

module.exports = { checkProjection, loadClassification, readBundle, forbiddenMaterial };
