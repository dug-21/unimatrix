#!/usr/bin/env node
'use strict';

// P1 — Receiver pull and proposal job (col-007).
//
// Entered by each receiver's OWN scheduled/triggered workflow (EP-02). It resolves the
// receiver's declared opaque identity and reviewed target, pins the selected accepted mirror
// commit, establishes the PUBLIC accepted-mirror source (S8-P), composes ONE deterministic
// `.collective/` projection OFFLINE from the independently verified pinned release bytes, and
// opens ONE idempotent reviewable adoption proposal through a transport-only PR adapter. It
// emits DISTINCT trigger / start / prepare / open receipts and can never merge, approve, or
// create any collective grant.
//
//   runProposal(input, deps) -> { outcome, pair, receipts, proposal, condition, exitCode }
//     outcome ∈ opened | previously-opened | prepared | held | refused
//     input   : { receiverIdentity, mirrorIdentity, mirrorCommit, releaseIdentity, target }
//     target  : { receiver_location, canonical_branch, proposal_base, accepted_ref,
//                 reviewed_revision }
//
// Reuse (never reimplemented):
//   * release integrity — readAuthority/inspect (the same shared leaf P2 reuses) reads and
//     closes the accepted index and captures the complete verified release tree;
//   * the pure offline composer — through compose-core (a purely additive wrapper of
//     tools/project/compose.js), so byte-copy / projection-manifest / boundary logic is not
//     re-derived here;
//   * the transport-only PR adapter — tools/delivery/github-pr-adapter.js.
//
// Boundaries the job holds (locked design):
//   * Idempotency reads the host's ALL-STATE receiver proposal PR history for the pair ALONE.
//     E1's private opening receipt/ledger is NEVER a P1 input.
//   * S8-P public source: the injected `verifyCustody` establishes the accepted-mirror public
//     custody chain; an UNAVAILABLE public source HOLDS the run, an OBSERVED INVALID public
//     source REFUSES it. The FULL verifyProposal check (P2) runs later as the receiver's
//     required check on the actual proposed revision after the PR opens.
//   * The job can write only its own proposal branch/PR. It never merges or creates a
//     collective approval, and never places the #90 pass or admission result inside
//     `.collective/`.

const fs = require('node:fs');
const { isDeepStrictEqual: equal } = require('node:util');

const F = require('../release/source/format');
const { openGitRevision, requireCommit } = require('../release/source/git-revision');
const { readAuthority, inspect } = require('../release/integrity/verify');
const { composeProjection } = require('./compose-core');
const { openProposal: defaultOpenProposal } = require('./github-pr-adapter');

// Review-binding wire (contract §4). These are the exact markers/shape the receiver's required
// pre-adoption check (P2) reads back, so the produced body must match them precisely.
const BINDING_START = '<!-- collective-delivery-binding:start -->';
const BINDING_END = '<!-- collective-delivery-binding:end -->';
const REVIEW_BINDING_SCHEMA = 'collective.delivery-review-binding/1';
const PROGRAM_RE = /^prg_[0-9a-f]{32}$/;
const SEMVER = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const ADMISSION_PATH_RE = /^admission\/[0-9a-f]{64}\.yaml$/;
const isPlainObject = v => !!v && typeof v === 'object' && !Array.isArray(v);
const closedKeys = (v, keys) => isPlainObject(v) && equal(Object.keys(v).sort(), [...keys].sort());
const nonEmpty = v => typeof v === 'string' && v.length > 0;

function reviewBindingValid(o) {
  return closedKeys(o, ['schema', 'program_identity', 'release_identity', 'content_digest',
    'mirror_repository_identity', 'mirror_commit', 'admission_path']) &&
    o.schema === REVIEW_BINDING_SCHEMA && PROGRAM_RE.test(o.program_identity) &&
    SEMVER.test(o.release_identity) && DIGEST.test(o.content_digest) &&
    nonEmpty(o.mirror_repository_identity) && nonEmpty(o.mirror_commit) && ADMISSION_PATH_RE.test(o.admission_path);
}

// Control-flow signal, mirroring the P2 posture: `held` (public source/observation unavailable)
// and `refused` (observed public/local mismatch or invalid) both exit non-zero and never become
// an opening; `usage` exits 2.
class RunError extends Error {
  constructor(outcome, condition, item, message) {
    super(message);
    Object.assign(this, { name: 'RunError', outcome, condition, item });
  }
}
const held = (condition, item, message) => { throw new RunError('held', condition, item, message); };
const refused = (condition, item, message) => { throw new RunError('refused', condition, item, message); };
const usage = (item, message) => { throw new RunError('usage', 'usage-invalid', item, message); };

function admissionPathFor(contentDigest) {
  if (typeof contentDigest !== 'string' || !DIGEST.test(contentDigest))
    refused('verdict-invalid', 'content_digest', 'Accepted content digest is malformed');
  return `admission/${contentDigest.slice('sha256:'.length)}.yaml`;
}

// Parse the review-binding block from a receiver PR body, exactly as the required check reads it:
// one marker pair, one fenced JSON object, closed schema. Returns the block or null (a body with
// no/again-ambiguous block is not a match for this pair).
function parseReviewBinding(body) {
  if (typeof body !== 'string') return null;
  if (body.split(BINDING_START).length !== 2 || body.split(BINDING_END).length !== 2) return null;
  const startAt = body.indexOf(BINDING_START); const endAt = body.indexOf(BINDING_END);
  if (endAt < startAt) return null;
  const between = body.slice(startAt + BINDING_START.length, endAt);
  const fences = [...between.matchAll(/```json\n([\s\S]*?)\n```/g)];
  if (fences.length !== 1) return null;
  let block;
  try { block = JSON.parse(fences[0][1]); } catch { return null; }
  return reviewBindingValid(block) ? block : null;
}

// ---- Step 1: identity and reviewed target agreement ---------------------------------------
// Independently read the receiver's CURRENT installed declaration from its canonical branch and
// verify it still agrees with the already-reviewed static R3 target. A pending/unmerged
// bootstrap, stale location or mismatched target refuses; loss of the current private landing
// attestation alone does not invalidate these static inputs.
async function resolveIdentityAndTarget(input, deps) {
  const { receiverIdentity, mirrorIdentity, target } = input;
  if (!PROGRAM_RE.test(receiverIdentity || '')) usage('receiver_identity', 'Opaque prg_ receiver identity required');
  if (!nonEmpty(mirrorIdentity)) usage('mirror_identity', 'Mirror repository identity required');
  if (!isPlainObject(target)) usage('target', 'Reviewed target descriptor required');
  for (const field of ['receiver_location', 'canonical_branch', 'proposal_base', 'accepted_ref'])
    if (!nonEmpty(target[field])) usage(`target.${field}`, `Reviewed target ${field} required`);
  if (typeof deps.readInstallation !== 'function') usage('read_installation', 'Installation reader adapter required');

  let read;
  try { read = await deps.readInstallation({ programIdentity: receiverIdentity,
    receiverLocation: target.receiver_location, canonicalBranch: target.canonical_branch }); }
  catch (error) { held('source-unavailable', 'installation', `Receiver installation unreadable: ${error && error.message || error}`); }
  if (!read || typeof read !== 'object') held('source-unavailable', 'installation', 'Receiver installation read returned nothing');
  if (read.observedLocation !== target.receiver_location)
    refused('target-mismatch', 'installation.location', 'Installed receiver location differs from the reviewed target');
  let observedRevision;
  try { observedRevision = requireCommit(read.observedRevision); }
  catch { refused('target-mismatch', 'installation.revision', 'Installation observed revision is not a full immutable commit'); }

  let installation;
  try { installation = F.parseDocument(Buffer.from(read.bytes)); }
  catch (error) { refused('target-mismatch', 'installation', `Installation is not strict JSON: ${error.message}`); }
  if (!closedKeys(installation, ['schema', 'program_identity', 'mirror', 'canonical_branch', 'proposal_base']) ||
      installation.schema !== 'collective.delivery-installation/1' ||
      !closedKeys(installation.mirror, ['repository_identity', 'accepted_ref']))
    refused('target-mismatch', 'installation', 'Installation violates its closed schema');
  if (installation.program_identity !== receiverIdentity)
    refused('identity-mismatch', 'installation.program_identity', 'Installed identity differs from the checked receiver');
  if (installation.mirror.repository_identity !== mirrorIdentity || installation.mirror.accepted_ref !== target.accepted_ref)
    refused('target-mismatch', 'installation.mirror', 'Installed mirror identity or accepted ref differs from the reviewed target');
  if (installation.canonical_branch !== target.canonical_branch || installation.proposal_base !== target.proposal_base)
    refused('target-mismatch', 'installation.branches', 'Installed branch bindings differ from the reviewed target');
  return { installation, installationRevision: observedRevision };
}

// ---- Step 2: verified release bytes + public S8-P source ----------------------------------
function verifyReleaseBytes(mirrorSource, input) {
  const { releaseIdentity } = input;
  if (!F.validVersion(releaseIdentity)) usage('release_identity', 'Explicit semver release identity required');
  let authority;
  try { authority = readAuthority(mirrorSource); }
  catch (error) {
    if (error instanceof F.ReleaseError && error.state === 'source-unavailable')
      held('source-unavailable', 'mirror-index', error.message);
    refused('source-invalid', 'mirror-index', error && error.message ? error.message : String(error));
  }
  const selected = authority.known.get(releaseIdentity);
  if (!selected) refused('source-unapproved', releaseIdentity, 'Release identity absent from the accepted mirror index');
  if (selected.verdict.result !== 'pass' || selected.verdict.gate !== 'pre-promotion')
    refused('verdict-invalid', releaseIdentity, 'Promotion verdict is not an authorizing pre-promotion pass');
  let diagnostics;
  try { diagnostics = inspect(selected.entries, selected.entry); }
  catch (error) { refused('source-invalid', releaseIdentity, error && error.message ? error.message : String(error)); }
  if (diagnostics.length)
    refused(diagnostics.some(d => d.state === 'release-unsupported') ? 'release-unsupported' : 'source-invalid',
      diagnostics[0].item, diagnostics[0].message);
  // Registered receiver identity must appear in this release's REGISTRATIONS.yaml.
  const regEntry = selected.entries.find(e => e.path === 'REGISTRATIONS.yaml');
  let registrations;
  try { registrations = F.parseDocument(regEntry.bytes, 'registration-set', 'REGISTRATIONS.yaml'); }
  catch (error) { refused('source-invalid', 'REGISTRATIONS.yaml', error.message); }
  if (!Array.isArray(registrations.programs) || !registrations.programs.some(p => p.program_identity === input.receiverIdentity))
    refused('identity-mismatch', 'REGISTRATIONS.yaml', 'Receiver identity is not registered in this release');
  // Off-payload public #90 pass MUST exist and bind the accepted index; the full profile check
  // is the required check's (P2) job. An absent pass holds (unavailable), a mismatched one refuses.
  const contentDigest = selected.entry.content_digest;
  const passPath = admissionPathFor(contentDigest);
  let passBytes;
  try { passBytes = mirrorSource.readFile(passPath).bytes; }
  catch (error) {
    if (error instanceof F.ReleaseError && error.state === 'source-unavailable')
      held('source-unavailable', passPath, 'Public #90 pass absent from the mirror snapshot');
    refused('verdict-invalid', passPath, error && error.message ? error.message : String(error));
  }
  let pass;
  try { pass = JSON.parse(F.utf8(passBytes)); } catch { refused('verdict-invalid', passPath, 'Public #90 pass is not valid JSON'); }
  if (!isPlainObject(pass) || pass.schema !== 'collective.gate-verdict/1' || pass.gate !== 'pre-admission' ||
      pass.result !== 'pass' || !isPlainObject(pass.binding) ||
      pass.binding.subject_release_identity !== releaseIdentity || pass.binding.content_digest !== contentDigest)
    refused('verdict-invalid', passPath, 'Public #90 pass does not bind the accepted index identity/digest');
  return { verifiedRelease: { releaseIdentity, contentDigest, entries: selected.entries }, contentDigest, passPath };
}

// ---- Step 4: idempotency from ALL-STATE host PR history (E1 receipt is never a P1 input) ---
async function decideIdempotency(pair, deps) {
  let history;
  try { history = await deps.readProposalHistory({ ...pair }); }
  catch (error) { held('observation-unavailable', 'proposal-history', `All-state receiver PR history unreadable: ${error && error.message || error}`); }
  if (!Array.isArray(history)) held('observation-unavailable', 'proposal-history', 'All-state receiver PR history is unreadable');

  const matches = [];
  for (const pr of history) {
    if (!isPlainObject(pr) || !Number.isSafeInteger(pr.number)) continue;
    const block = parseReviewBinding(pr.body);
    if (!block) continue;
    // The pair key is the contract §3 3-tuple (program_identity, release_identity,
    // content_digest) ALONE — matching E1's reconcile keying. mirror_commit is NOT part of the
    // idempotency match: the append-only mirror tip may advance while content_digest is
    // unchanged, and that same pair must still resolve to its existing proposal, never a
    // duplicate. (mirror_commit stays in the opened/prepared diagnostics and binding content.)
    if (block.program_identity !== pair.program_identity || block.release_identity !== pair.release_identity ||
        block.content_digest !== pair.content_digest) continue;
    if (typeof pr.head?.sha !== 'string' || !pr.head.sha) refused('proposal-invalid', String(pr.number), 'Matching PR has no immutable proposed revision');
    matches.push({ number: pr.number, state: pr.state, headSha: pr.head.sha, url: pr.url || null });
  }
  const open = matches.filter(m => m.state === 'open');
  const closed = matches.filter(m => ['closed', 'merged', 'declined'].includes(m.state));
  if (open.length > 1) refused('proposal-invalid', String(open[0].number), 'Multiple conflicting open proposals for the pair');
  if (open.length === 1) {
    // A matching valid open PR is an idempotent retry, not a new opening.
    return { decision: 'previously-open', proposal: open[0] };
  }
  if (closed.length >= 1) {
    // A closed matching proposal is a prior delivery: do not reopen or duplicate.
    return { decision: 'previously-opened', proposal: closed.sort((a, b) => a.number - b.number)[0] };
  }
  // Any match whose state is neither open nor a recognised closed state is ambiguous.
  if (matches.length) refused('proposal-invalid', String(matches[0].number), 'Ambiguous receiver PR state for the pair');
  return { decision: 'none' };
}

function buildReviewBody(block, releaseIdentity, passPath) {
  // The #90 pass reference is rendered as ordinary review text OUTSIDE the binding markers, and
  // the closed binding block sits between the exact markers as one fenced JSON object.
  return `Adoption proposal for release ${releaseIdentity}.\n\nPre-admission #90 pass: ${passPath}\n\n` +
    `${BINDING_START}\n\`\`\`json\n${JSON.stringify(block, null, 2)}\n\`\`\`\n${BINDING_END}\n`;
}

// ---- Orchestration -----------------------------------------------------------------------
async function runProposal(input, deps = {}) {
  const clock = typeof deps.clock === 'function' ? deps.clock : () => new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const receipts = {
    trigger: { at: clock(), exit: 0 },
    start: { at: clock(), exit: 0 },
    prepare: null,
    open: null,
  };
  const pairBase = {
    program_identity: input && input.receiverIdentity,
    release_identity: input && input.releaseIdentity,
    content_digest: null,
    mirror_commit: input && input.mirrorCommit,
  };
  const done = (outcome, extra) => Object.freeze({ outcome, pair: Object.freeze({ ...pairBase }), receipts,
    proposal: null, ...extra });

  try {
    if (!isPlainObject(input)) usage('input', 'runProposal input object required');
    requireCommitOrUsage(input.mirrorCommit, 'mirror_commit');

    // 1. Identity + reviewed target agreement (independent installation read).
    await resolveIdentityAndTarget(input, deps);

    // 2. Verified release bytes + off-payload #90 pass reference.
    const mirrorSource = deps.mirrorSource || openGitRevision(requireGitDir(deps.mirrorGitDir, 'mirror'), input.mirrorCommit);
    const { verifiedRelease, contentDigest, passPath } = verifyReleaseBytes(mirrorSource, input);
    pairBase.content_digest = contentDigest;
    const pair = { program_identity: input.receiverIdentity, release_identity: input.releaseIdentity,
      content_digest: contentDigest, mirror_commit: input.mirrorCommit };

    // 2b. Public S8-P accepted-mirror custody: unavailable HOLDS, observed-invalid REFUSES.
    if (typeof deps.verifyCustody !== 'function') usage('verify_custody', 'S8-P custody verifier adapter required');
    try {
      const custody = await deps.verifyCustody({ mirrorIdentity: input.mirrorIdentity, mirrorCommit: input.mirrorCommit,
        acceptedRef: input.target.accepted_ref, releaseIdentity: input.releaseIdentity, contentDigest });
      if (!custody || custody.accepted !== true) refused('source-invalid', 'mirror-custody', 'Public accepted-mirror custody not established');
    } catch (error) {
      if (error instanceof RunError) throw error;
      if (error && (error.result === 'refused' || error.refused))
        refused(error.condition || 'source-invalid', 'mirror-custody', error.message || 'Observed invalid public source');
      held('source-unavailable', 'mirror-custody', (error && error.message) || 'Public accepted-mirror source unavailable');
    }

    // 4. Idempotency from ALL-STATE host PR history (before any compose/open).
    const idem = await decideIdempotency(pair, deps);
    if (idem.decision === 'previously-open')
      return done('opened', { proposal: Object.freeze({ number: idem.proposal.number, url: idem.proposal.url,
        proposedRevision: idem.proposal.headSha, baseCommit: null, idempotent: true }), exitCode: 0 });
    if (idem.decision === 'previously-opened')
      return done('previously-opened', { proposal: Object.freeze({ number: idem.proposal.number, url: idem.proposal.url,
        proposedRevision: idem.proposal.headSha, baseCommit: null }), condition: 'previously-opened', exitCode: 0 });

    // 3/5 prepare: compose the deterministic projection OFFLINE from verified bytes and build
    // the review body; reject an empty/no-diff replay as new-delivery evidence.
    let bundle;
    try { bundle = composeProjection(verifiedRelease, input.receiverIdentity); }
    catch (error) { refused('proposal-invalid', 'projection', `Composition refused: ${error && error.message ? error.message : error}`); }
    const projection = bundle.files.map(f => ({ path: f.path, bytes: Buffer.from(f.bytes), mode: '0644' }));
    if (typeof deps.readCanonicalProjection === 'function') {
      let canonical;
      try { canonical = await deps.readCanonicalProjection(); }
      catch (error) { held('observation-unavailable', 'canonical', `Canonical projection unreadable: ${error && error.message || error}`); }
      if (canonical instanceof Map && canonical.size === projection.length &&
          projection.every(f => canonical.has(f.path) && Buffer.from(canonical.get(f.path)).equals(f.bytes)))
        refused('proposal-invalid', 'no-diff', 'Composed projection is identical to the adopted canonical state (no-diff replay)');
    }
    const block = {
      schema: REVIEW_BINDING_SCHEMA,
      program_identity: input.receiverIdentity,
      release_identity: input.releaseIdentity,
      content_digest: contentDigest,
      mirror_repository_identity: input.mirrorIdentity,
      mirror_commit: input.mirrorCommit,
      admission_path: passPath,
    };
    if (!reviewBindingValid(block)) refused('proposal-invalid', 'review-binding', 'Constructed review binding violates its closed schema');
    const body = buildReviewBody(block, input.releaseIdentity, passPath);
    receipts.prepare = { at: clock(), exit: 0 };

    // 6 open: transport-only PR adapter opens ONE reviewable PR. A push that succeeds but a PR
    // open that fails is `prepared/failed`, not opened.
    const openProposal = deps.openProposal || (proposal => defaultOpenProposal(proposal, deps.transport || {}));
    let coordinates;
    try {
      coordinates = await openProposal({
        receiverRepository: input.target.receiver_location,
        proposalBase: input.target.proposal_base,
        branch: deps.branch || `col-007/adopt-${input.releaseIdentity}`,
        projection, body,
      });
    } catch (error) {
      const pushed = !!(error && error.pushed);
      receipts.open = { at: clock(), exit: error && Number.isInteger(error.code) ? error.code : 1 };
      if (pushed)
        return done('prepared', { condition: 'open-failed',
          message: error && error.message ? error.message : String(error), exitCode: 3 });
      refused((error && error.condition) || 'proposal-invalid', 'open', error && error.message ? error.message : String(error));
    }
    receipts.open = { at: clock(), exit: 0 };
    return done('opened', { proposal: Object.freeze({ number: coordinates.number, url: coordinates.url,
      proposedRevision: coordinates.proposedRevision, baseCommit: coordinates.baseCommit, idempotent: false }), exitCode: 0 });
  } catch (error) {
    if (error instanceof RunError) {
      if (error.outcome === 'usage')
        return done('refused', { condition: 'usage-invalid', message: error.message, evidence_reference: error.item, exitCode: 2 });
      // Attribute the failing receipt: a preparation-stage failure marks prepare, otherwise start.
      if (receipts.prepare === null) receipts.start.exit = 1; else receipts.prepare.exit = 1;
      return done(error.outcome, { condition: error.condition, message: error.message,
        evidence_reference: error.item ? String(error.item) : undefined, exitCode: 3 });
    }
    // Unexpected: fail closed as a hold, never as an opening.
    receipts.start.exit = 1;
    return done('held', { condition: 'source-unavailable', message: error && error.message ? error.message : String(error),
      evidence_reference: 'unexpected', exitCode: 3 });
  }
}

function requireCommitOrUsage(value, item) {
  try { return requireCommit(value); } catch { usage(item, 'Expected a full immutable commit ID'); }
}
function requireGitDir(value, which) {
  if (typeof value !== 'string' || !value) usage(`${which}_git_dir`, `Configured ${which} Git directory required`);
  return value;
}

// ---- CLI ---------------------------------------------------------------------------------
// The receiver workflow invokes this entry point. Host readers (installation, S8-P custody,
// all-state PR history) and the PR transport are deployment glue wired from configuration; the
// deterministic core is `runProposal(input, deps)` above, exercised offline in the component
// tests with injected adapters.
function parseArgv(argv) {
  const args = {};
  const flags = ['--receiver', '--mirror-identity', '--mirror-commit', '--release', '--target'];
  for (let i = 0; i < argv.length; i += 2) {
    const flag = argv[i];
    if (!flags.includes(flag) || args[flag] !== undefined || argv[i + 1] === undefined) return null;
    args[flag] = argv[i + 1];
  }
  return args;
}
async function main(argv) {
  const args = parseArgv(argv);
  if (!args || !args['--target']) { process.stderr.write('Invalid receiver-run arguments\n'); return 2; }
  let target;
  try { target = F.parseDocument(fs.readFileSync(args['--target'])); }
  catch (error) {
    process.stdout.write(JSON.stringify({ outcome: 'held', condition: 'source-unavailable',
      message: `target unreadable: ${error.message}` }) + '\n');
    return 3;
  }
  const input = {
    receiverIdentity: args['--receiver'], mirrorIdentity: args['--mirror-identity'],
    mirrorCommit: args['--mirror-commit'], releaseIdentity: args['--release'], target,
  };
  // Deployment wiring of the host readers/transport is intentionally not fabricated here; a
  // real workflow supplies them. Without them the run holds rather than inventing success.
  const result = await runProposal(input, {
    mirrorGitDir: process.env.COLLECTIVE_MIRROR_GIT_DIR,
    transport: { gitDir: process.env.COLLECTIVE_RECEIVER_GIT_DIR || process.cwd() },
  });
  process.stdout.write(JSON.stringify(result) + '\n');
  return result.exitCode;
}
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; });

module.exports = { runProposal, parseReviewBinding, buildReviewBody, reviewBindingValid, admissionPathFor,
  BINDING_START, BINDING_END, REVIEW_BINDING_SCHEMA };
