'use strict';

// C2 accepted-release reader (ADR-007: a release is resolved THROUGH the index, never
// from a version string carried in an artifact). This module resolves the immutable
// accepted release from collective authority (the installed collective ROOT's
// `baseline/`) and returns the released file bytes plus the released MANIFEST.yaml, so
// the composer can copy the cleared bytes verbatim (DR-02) and confine the graded
// release identity/digest to ADOPTED.yaml (DR-10).
//
// Import surface (release-authority LEAF primitives only — the same reads C1 and C7
// use; C2 depends on {release authority, registry, C7} per Architecture §4):
//   ../../release/integrity/verify  (readAuthority — full index+verdict closure)
//   ../../release/source/local      (TOCTOU-safe filesystem snapshot)
//   ../../release/source/format     (strict parse / digest)
// It imports no carrier/observe code and mutates no accepted byte.

const path = require('node:path');
const { readAuthority } = require('../../release/integrity/verify');
const { snapshotDirectory, readLocalFile } = require('../../release/source/local');
const F = require('../../release/source/format');
const { ComposeError } = require('./errors');

// tools/project/lib -> tools/project -> tools -> <collective ROOT>
const ROOT = path.resolve(__dirname, '../../..');

// A minimal filesystem-backed accepted-authority source shaped as readAuthority expects
// (readFile -> {bytes}; readTree -> [{path, mode, bytes}] with paths relative to root).
function filesystemSource(root) {
  return {
    readFile(rel) { return readLocalFile(path.join(root, rel)); },
    readTree(rel, options = {}) {
      try {
        return snapshotDirectory(path.join(root, rel));
      } catch (e) {
        if (options.optional && e && e.state === 'source-unavailable') return [];
        throw e;
      }
    },
  };
}

// resolveRelease(releaseIdentity) -> {
//   release_identity : the index entry's release_identity (e.g. '0.2.0'),
//   content_digest   : the index entry's content_digest (the graded value),
//   files            : Map<relPath, Buffer> of the released tree bytes (all 7 files),
//   manifest         : the parsed released MANIFEST.yaml (per-file {path,mode,size,digest}),
// }
// The identity and content_digest come from the CLOSED accepted index, not from any
// file inside the release tree (ADR-007/ADR-008). Throws ComposeError(code 3) if the
// accepted authority is unreadable/inconsistent or does not carry the release.
function resolveRelease(releaseIdentity = '0.2.0', options = {}) {
  const root = options.root || ROOT;
  let authority;
  try {
    authority = readAuthority(filesystemSource(root));
  } catch (e) {
    throw new ComposeError('source-unavailable', 'baseline/RELEASES.yaml',
      `accepted release authority unavailable: ${e && e.message ? e.message : e}`, 3);
  }
  const record = authority.known.get(releaseIdentity);
  if (!record) {
    throw new ComposeError('release-unresolved', releaseIdentity,
      `release ${releaseIdentity} has no promoted entry in the accepted index`, 3);
  }
  const files = new Map(record.entries.map((e) => [e.path, e.bytes]));
  const manifestBytes = files.get('MANIFEST.yaml');
  if (!manifestBytes) {
    throw new ComposeError('source-unavailable', 'MANIFEST.yaml',
      'accepted release is missing its MANIFEST.yaml', 3);
  }
  let manifest;
  try {
    manifest = F.parseDocument(manifestBytes);
  } catch (e) {
    throw new ComposeError('source-unavailable', 'MANIFEST.yaml',
      `released MANIFEST.yaml unparseable: ${e && e.message ? e.message : e}`, 3);
  }
  return {
    release_identity: record.entry.metadata.release_identity,
    content_digest: record.entry.content_digest,
    files,
    manifest,
  };
}

module.exports = { ROOT, filesystemSource, resolveRelease };
