'use strict';

// C2 — Projection composer (#18). Interface I-COMPOSE:
//   compose(release, receiverRegistration) -> ProjectionBundle
//
// Deterministically builds ONE per-receiver `.collective/` bundle from the immutable
// accepted release + that receiver's registration. First-delivery only (no update
// preflight — drift is Step E). The bundle is the CLOSED projected file set (Contract §3):
//   .collective/README.md            (baseline; local source path of each fact; no graded value)
//   .collective/MANIFEST.yaml         (collective.projection-manifest/1; excludes itself)
//   .collective/ADOPTED.yaml          (collective.adopted-release/1; the ONLY file with the
//                                      adopted release identity + content digest — DR-10)
//   .collective/contract/CONTRACT.md      \
//   .collective/contract/VOCABULARY.yaml   } BYTE-IDENTICAL copies of the released files
//   .collective/contract/OBLIGATIONS.yaml  } (copied, never reserialized — DR-02)
//   .collective/contract/REGISTRATIONS.yaml/
//   .collective/EXTENSIONS.md         (program authority; placeholder)
// Plus, for a release declaring program-facing payload (ADR-031 #168), each such release
// file projected BYTE-IDENTICALLY to `.collective/<release_path>` at authority: baseline
// (e.g. onboarding/DIRECTIVE.md, skills/collective-participation/SKILL.md). 0.2.0 declares
// none, so its projected surface stays exactly the eight files above.
//
// Honored invariants:
//   DR-02  each contract/ copy is byte-identical; its manifest digest == the released
//          MANIFEST.yaml digest. Byte-identity proves the projected bytes ARE the cleared
//          release bytes; it is NOT meaning-preservation (that is the human MP-REVIEW).
//   DR-10  only ADOPTED.yaml carries the release identity/content digest; README/MANIFEST
//          carry none. An unadopted receiver (no ADOPTED.yaml) exposes no adopted fact.
//   DR-11  composing the same (release, registration) twice yields byte-identical bytes;
//          no wall-clock or nondeterministic field in any authored file.
//   ADR-018 the projected kernel keeps two registers; Declaration/declares stay reserved;
//          no mechanism promotes reserved->admitted (checked deterministically).
//
// Failure is fail-loud/closed: any missing/unreadable release file, digest mismatch,
// register bypass, graded-value/determinism self-check failure, or C7 violation aborts
// composition, opens NO bundle, and returns non-zero. C2 writes to no receiver and
// performs no merge.

const fs = require('node:fs');
const path = require('node:path');

const F = require('../release/source/format');
const { ComposeError } = require('./lib/errors');
const { resolveRelease } = require('./lib/release-authority');
const { checkRegisters } = require('./lib/registers');
const { serialize, writeBundle } = require('./lib/serialize');
const { buildReadme, buildExtensions } = require('./authority-header');
const { buildProjectionManifest, MANIFEST_PATH } = require('./projection-manifest');
// C2 depends on C7 and validates its OWN composed bundle before returning
// (belt-and-suspenders). C7 remains the INDEPENDENT checker of record.
const { checkProjection, loadClassification } = require('../boundary/lib/projection');

// The four released files copied byte-identically into `.collective/contract/`.
const CONTRACT_SRCS = Object.freeze([
  'CONTRACT.md', 'VOCABULARY.yaml', 'OBLIGATIONS.yaml', 'REGISTRATIONS.yaml',
]);
// Kernel + release-level files already handled above (the byte-identical contract/ copies
// and register-faithful VOCABULARY) or intentionally NOT projected (RELEASE.yaml /
// MANIFEST.yaml are release-level and C7 `forbiddenMaterial` refuses them; CLEARANCES.yaml
// is private and never enters the projected surface). Every OTHER path in the release
// manifest is a program-facing payload file (ADR-031 #168) projected byte-identically to
// `.collective/<release_path>` at authority: baseline. Derived per release from the
// manifest, so 0.2.0 (whose manifest lists no non-kernel path) projects no payload.
const KERNEL_AND_LEVEL = Object.freeze([
  'CONTRACT.md', 'VOCABULARY.yaml', 'OBLIGATIONS.yaml', 'REGISTRATIONS.yaml',
  'CLEARANCES.yaml', 'RELEASE.yaml', 'MANIFEST.yaml',
]);
const IDENTITY_RE = /^prg_[0-9a-f]{32}$/;
// A wall-clock / date-time field of any kind must never appear in an authored file (times
// live only in the UNPROJECTED gate-verdict/observation — DR-11).
const CLOCK_RE = /\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/;

const ADOPTED_PATH = '.collective/ADOPTED.yaml';
// Files C2 AUTHORS (as opposed to the byte-identical release copies). Graded-value and
// no-clock confinement are C2's responsibility for exactly these: the release copies
// faithfully carry whatever the cleared release bytes carry (e.g. CONTRACT.md's own prose
// mentions the proposed version), which is byte-identity (DR-02), not a C2 emission.
const AUTHORED_PATHS = Object.freeze([
  '.collective/README.md', '.collective/EXTENSIONS.md', MANIFEST_PATH,
]);

// Extract the receiver's opaque program_identity from its registration. Accepts either a
// registry record ({registration:{program_identity}}) or a flat {program_identity}.
function receiverProgramIdentity(receiverRegistration) {
  const reg = receiverRegistration || {};
  const id = (reg.registration && reg.registration.program_identity) || reg.program_identity;
  if (typeof id !== 'string' || !IDENTITY_RE.test(id)) {
    throw new ComposeError('registration-invalid', 'program_identity',
      'receiver registration does not carry a valid opaque program_identity', 2);
  }
  return id;
}

// DR-10 self-assertion: only ADOPTED.yaml may carry the release identity/digest; the
// files C2 authors carry neither. Every non-ADOPTED file is additionally checked for the
// content_digest (the graded value C7 confines across the whole surface).
function assertGradedConfinement(files, release) {
  for (const file of files) {
    if (file.path === ADOPTED_PATH) continue;
    const text = file.bytes.toString('utf8');
    if (text.includes(release.content_digest)) {
      throw new ComposeError('graded-value-leak', file.path,
        `content_digest is confined to ADOPTED.yaml but appears in ${file.path} (DR-10)`, 1);
    }
    if (AUTHORED_PATHS.includes(file.path) && text.includes(release.release_identity)) {
      throw new ComposeError('graded-value-leak', file.path,
        `release identity is confined to ADOPTED.yaml but appears in composer-authored ${file.path} (DR-10)`, 1);
    }
  }
}

// DR-11 self-assertion: no authored file carries a wall-clock/date-time field.
function assertNoClock(files) {
  for (const file of files) {
    if (!AUTHORED_PATHS.includes(file.path) && file.path !== ADOPTED_PATH) continue;
    if (CLOCK_RE.test(file.bytes.toString('utf8'))) {
      throw new ComposeError('nondeterministic-field', file.path,
        `a wall-clock/date-time field appears in composer-authored ${file.path} (DR-11)`, 1);
    }
  }
}

// compose(release, receiverRegistration[, options]) -> ProjectionBundle.
//   release             : { release_identity, content_digest, files:Map<name,Buffer>, manifest }
//                         (from resolveRelease — resolved through the accepted index).
//   receiverRegistration: the accepted registration supplying program_identity.
// Returns { files:[{path,bytes,authority}], program_identity, release_identity, content_digest }.
function compose(release, receiverRegistration, options = {}) {
  if (!release || !(release.files instanceof Map) || !release.manifest) {
    throw new ComposeError('release-input', 'release', 'compose requires a resolved release object', 3);
  }
  const programIdentity = receiverProgramIdentity(receiverRegistration);
  const classification = options.classification || loadClassification();

  // 1. byte-identical projection (DR-02, DR-11) — copy, never re-serialize.
  const releasedDigests = new Map();
  for (const rec of release.manifest.files) releasedDigests.set(rec.path, rec.digest);
  const files = [];
  for (const src of CONTRACT_SRCS) {
    const bytes = release.files.get(src);
    if (!Buffer.isBuffer(bytes)) {
      throw new ComposeError('release-file-missing', src, `released ${src} is unavailable`, 3);
    }
    const copied = Buffer.from(bytes); // exact byte copy; no YAML round-trip
    const digest = F.digest(copied);
    const expected = releasedDigests.get(src);
    if (digest !== expected) {
      throw new ComposeError('byte-identity-mismatch', src,
        `copied ${src} digest ${digest} != released MANIFEST digest ${expected} (DR-02: no reserialization)`, 1);
    }
    files.push({ path: `.collective/contract/${src}`, bytes: copied, authority: 'baseline' });
  }

  // 2. register-faithful projection is intrinsic to the byte copy (ADR-018); re-assert it
  //    structurally on the copied VOCABULARY bytes. No human judgment.
  checkRegisters(release.files.get('VOCABULARY.yaml'));

  // 3. adopted-release pointer (DR-10) — the ONLY file carrying release identity/digest.
  const adopted = {
    schema: 'collective.adopted-release/1',
    authority: 'baseline',
    program_identity: programIdentity,
    adopted_release: {
      release_identity: release.release_identity,
      content_digest: release.content_digest,
    },
  };
  files.push({ path: ADOPTED_PATH, bytes: serialize(adopted), authority: 'baseline' });

  // 3b. program-facing payload projection (ADR-031 #168): each release-tree path outside
  //     the kernel/release-level set is projected BYTE-IDENTICALLY to
  //     `.collective/<release_path>` at authority: baseline — copied, never reserialized,
  //     and digest-checked against the released MANIFEST (the same posture as the contract/
  //     copies). The payload set is derived per release from the manifest, so 0.2.0 adds
  //     nothing. Graded values stay confined to ADOPTED.yaml: each payload file is
  //     non-ADOPTED, so assertGradedConfinement below already asserts it carries no
  //     content_digest (CC-3 no-smuggling). buildProjectionManifest picks up each row
  //     automatically; the independent C7 check must find each path in its closed allow-list.
  for (const rec of release.manifest.files) {
    const src = rec.path;
    if (KERNEL_AND_LEVEL.includes(src)) continue;
    const bytes = release.files.get(src);
    if (!Buffer.isBuffer(bytes)) {
      throw new ComposeError('release-file-missing', src, `program-facing ${src} is unavailable`, 3);
    }
    const copied = Buffer.from(bytes); // exact byte copy; no reserialization
    const digest = F.digest(copied);
    const expected = releasedDigests.get(src);
    if (digest !== expected) {
      throw new ComposeError('byte-identity-mismatch', src,
        `copied ${src} digest ${digest} != released MANIFEST digest ${expected} (byte-identity to the released payload file)`, 1);
    }
    files.push({ path: `.collective/${src}`, bytes: copied, authority: 'baseline' });
  }

  // 4. README + EXTENSIONS placeholder (carry NO release identity/digest, no wall-clock).
  files.push({ path: '.collective/README.md', bytes: Buffer.from(buildReadme(), 'utf8'), authority: 'baseline' });
  files.push({ path: '.collective/EXTENSIONS.md', bytes: Buffer.from(buildExtensions(), 'utf8'), authority: 'program' });

  // 5. projection manifest (excludes itself; carries NO release identity/digest).
  const manifest = buildProjectionManifest(files);
  files.push({ path: MANIFEST_PATH, bytes: serialize(manifest), authority: 'baseline' });

  // 6. determinism/confinement self-checks, then the INDEPENDENT boundary check (C7) on
  //    C2's OWN output. Any violation aborts and opens no bundle.
  assertGradedConfinement(files, release);
  assertNoClock(files);
  const record = { release_identity: release.release_identity, content_digest: release.content_digest };
  const { ok, violations } = checkProjection(
    files.map((f) => ({ path: f.path, bytes: f.bytes })), record, classification,
  );
  if (!ok) {
    throw new ComposeError('boundary-violation', violations[0] && violations[0].path,
      `C7 projection boundary refused the composed bundle: ${JSON.stringify(violations)}`, 1);
  }

  return {
    files,
    program_identity: programIdentity,
    release_identity: release.release_identity,
    content_digest: release.content_digest,
  };
}

module.exports = {
  compose,
  CONTRACT_SRCS,
  ADOPTED_PATH,
  AUTHORED_PATHS,
  receiverProgramIdentity,
};

// ---- CLI: compose one bundle deterministically and write it to --out. ----------------
//   node compose.js --release <identity> --registration <file> --out <dir>
// Emits one JSON result line; exit 0 success | 1 denied | 2 usage | 3 source/unexpected.
function parseArgv(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i += 2) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (value === undefined) throw new ComposeError('usage', flag, `missing value for ${flag}`, 2);
    if (flag === '--release') opts.release = value;
    else if (flag === '--registration') opts.registration = value;
    else if (flag === '--out') opts.out = value;
    else throw new ComposeError('usage', flag, `unexpected argument: ${flag}`, 2);
  }
  if (!opts.registration || !opts.out) {
    throw new ComposeError('usage', null,
      'usage: node compose.js --release <identity> --registration <file> --out <dir>', 2);
  }
  return opts;
}

function emit(value, code) { process.stdout.write(JSON.stringify(value) + '\n'); process.exitCode = code; }

function main(argv = process.argv.slice(2)) {
  let opts;
  try { opts = parseArgv(argv); }
  catch (e) { return emit({ tool: 'compose', state: e.state || 'usage', message: e.message }, e.code || 2); }

  let registration;
  try {
    registration = F.parseDocument(fs.readFileSync(path.resolve(opts.registration)));
  } catch (e) {
    return emit({ tool: 'compose', state: 'source-unavailable', message: `registration unreadable: ${e && e.message ? e.message : e}` }, 3);
  }

  let release;
  try { release = resolveRelease(opts.release || '0.2.0'); }
  catch (e) {
    return emit({ tool: 'compose', state: e.state || 'source-unavailable', message: e.message }, e.code || 3);
  }

  let bundle;
  try { bundle = compose(release, registration); }
  catch (e) {
    if (e instanceof ComposeError) return emit({ tool: 'compose', state: e.state, item: e.item, message: e.message }, e.code);
    return emit({ tool: 'compose', state: 'unexpected', message: e && e.message ? e.message : String(e) }, 3);
  }

  try { writeBundle(bundle, opts.out); }
  catch (e) {
    if (e instanceof ComposeError) return emit({ tool: 'compose', state: e.state, item: e.item, message: e.message }, e.code);
    return emit({ tool: 'compose', state: 'unexpected', message: `cannot write bundle: ${e && e.message ? e.message : e}` }, 3);
  }

  return emit({
    tool: 'compose',
    state: 'composed',
    program_identity: bundle.program_identity,
    release_identity: bundle.release_identity,
    files: bundle.files.map((f) => ({ path: f.path, authority: f.authority })),
    out: path.resolve(opts.out),
  }, 0);
}

if (require.main === module) main();
module.exports.main = main;
