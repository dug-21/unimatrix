'use strict';

// compose-core — a PURE projection adapter for the receiver job (P1, col-007).
//
// The receiver composes ONE `.collective/` adoption projection offline from the
// INDEPENDENTLY VERIFIED pinned release bytes it already read from the accepted mirror
// commit — never from the composer CLI's own defaults and never by rereading an unrelated
// local release. This adapter is a thin, PURE wrapper around the existing offline composer
// `tools/project/compose.js` (`compose`), which is already a pure function of
// (resolvedRelease, receiverRegistration). It is additive: it changes NO behaviour or
// signature of compose.js and reimplements none of its byte-copy, projection-manifest or
// projection-boundary logic.
//
//   composeProjection(verifiedRelease, programIdentity) -> composer bundle
//
// `verifiedRelease` is exactly what the shared release-integrity read produced for the
// selected accepted release:
//   { releaseIdentity, contentDigest, entries: [{ path, bytes, mode }] }
// where `entries` is the complete raw release tree captured by readAuthority/inspect (the
// same shape P2 feeds to `compose`). The adapter builds the resolved-release object the
// composer expects and delegates; the composer performs its own digest/boundary self-checks
// and refuses fail-closed, so this adapter never fabricates a bundle.

const F = require('../release/source/format');
const { compose } = require('../project/compose');

const PROGRAM_RE = /^prg_[0-9a-f]{32}$/;
const DIGEST_RE = /^sha256:[0-9a-f]{64}$/;

class ComposeCoreError extends Error {
  constructor(state, item, message) {
    super(message);
    Object.assign(this, { name: 'ComposeCoreError', state, item });
  }
}

// Build the composer's resolved-release object from the verified release bytes. No network,
// no disk read of an unrelated release: purely a function of the captured entries.
function resolveVerifiedRelease(verifiedRelease) {
  if (!verifiedRelease || typeof verifiedRelease !== 'object' || Array.isArray(verifiedRelease))
    throw new ComposeCoreError('release-input', 'verifiedRelease', 'A verified-release descriptor is required');
  const { releaseIdentity, contentDigest, entries } = verifiedRelease;
  if (!F.validVersion(releaseIdentity))
    throw new ComposeCoreError('release-input', 'release_identity', 'Explicit semver release identity required');
  if (typeof contentDigest !== 'string' || !DIGEST_RE.test(contentDigest))
    throw new ComposeCoreError('release-input', 'content_digest', 'Verified release content digest is malformed');
  if (!Array.isArray(entries) || entries.length === 0)
    throw new ComposeCoreError('release-input', 'entries', 'Verified release entries required');

  const files = new Map();
  let manifestEntry;
  for (const entry of entries) {
    if (!entry || typeof entry.path !== 'string' || !Buffer.isBuffer(Buffer.from(entry.bytes || [])))
      throw new ComposeCoreError('release-input', 'entries', 'Malformed verified release entry');
    if (files.has(entry.path))
      throw new ComposeCoreError('release-input', entry.path, 'Duplicate verified release entry');
    files.set(entry.path, Buffer.from(entry.bytes));
    if (entry.path === 'MANIFEST.yaml') manifestEntry = entry;
  }
  if (!manifestEntry)
    throw new ComposeCoreError('release-input', 'MANIFEST.yaml', 'Verified release has no MANIFEST.yaml');
  let manifest;
  try { manifest = F.parseDocument(Buffer.from(manifestEntry.bytes), 'manifest', 'MANIFEST.yaml'); }
  catch (error) { throw new ComposeCoreError('release-input', 'MANIFEST.yaml', `Verified release manifest unreadable: ${error.message}`); }

  return { release_identity: releaseIdentity, content_digest: contentDigest, files, manifest };
}

// composeProjection(verifiedRelease, programIdentity) -> the composer bundle
//   { files: [{ path, bytes, authority }], program_identity, release_identity, content_digest }
// The composer's own DR-02/DR-10/DR-11 self-checks and the independent projection-boundary
// check (C7) run inside compose(); any violation throws and opens no bundle.
function composeProjection(verifiedRelease, programIdentity) {
  if (typeof programIdentity !== 'string' || !PROGRAM_RE.test(programIdentity))
    throw new ComposeCoreError('registration-invalid', 'program_identity', 'Opaque prg_ receiver identity required');
  const resolved = resolveVerifiedRelease(verifiedRelease);
  // The composer accepts a flat { program_identity } registration; pass exactly the opaque
  // identity so no extra receiver state can leak into the deterministic projection.
  const bundle = compose(resolved, { program_identity: programIdentity });
  if (!bundle || !Array.isArray(bundle.files) || bundle.files.length === 0)
    throw new ComposeCoreError('projection-empty', 'projection', 'Composer returned no projected files');
  return bundle;
}

module.exports = { composeProjection, resolveVerifiedRelease, ComposeCoreError };
