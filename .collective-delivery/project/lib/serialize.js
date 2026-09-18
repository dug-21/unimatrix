'use strict';

// C2 — deterministic serialization + bundle materialization helpers.
//
// The composer AUTHORS exactly two structured documents (ADOPTED.yaml and the
// projection MANIFEST.yaml). They are emitted as canonical pretty JSON with a trailing
// newline: JSON is a subset of YAML 1.2 core, so the released strict-YAML parser and C7
// read them back unchanged, and object key order is fixed by construction — the same
// (release, registration) yields byte-identical bytes every time (DR-11). The four
// `contract/` copies are NEVER serialized here; they are copied verbatim (DR-02).
//
// No wall-clock, no map-ordering nondeterminism, no environment input enters these bytes.

const fs = require('node:fs');
const path = require('node:path');
const { ComposeError } = require('./errors');

const COLLECTIVE_PREFIX = '.collective/';

// Serialize an authored document to deterministic bytes.
function serialize(doc) {
  return Buffer.from(JSON.stringify(doc, null, 2) + '\n', 'utf8');
}

// Write a ProjectionBundle ({ files: [{path: '.collective/...', bytes}] }) into `outDir`,
// treating `outDir` as the receiver `.collective/` directory itself (the `.collective/`
// prefix is stripped, matching how C7's --bundle <dir> reads it back). Returns outDir.
function writeBundle(bundle, outDir) {
  const root = path.resolve(outDir);
  fs.mkdirSync(root, { recursive: true });
  for (const file of bundle.files) {
    if (!file.path.startsWith(COLLECTIVE_PREFIX)) {
      throw new ComposeError('bundle-path', file.path,
        `bundle file path must be under ${COLLECTIVE_PREFIX}`, 1);
    }
    const rel = file.path.slice(COLLECTIVE_PREFIX.length);
    const dest = path.join(root, rel);
    // Contain writes to outDir (defensive; composed paths are already safe).
    if (path.relative(root, dest).startsWith('..')) {
      throw new ComposeError('bundle-path', file.path, 'bundle path escapes the output directory', 1);
    }
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, file.bytes);
  }
  return root;
}

module.exports = { serialize, writeBundle, COLLECTIVE_PREFIX };
