#!/usr/bin/env node
'use strict';

// P2 — Source and local pre-adoption verifier (col-007).
//
// Entered from each receiver's REAL required pre-adoption check (EP-03) on the immutable
// proposed revision. It independently reads the reviewed public mirror snapshot and binds:
// release index, authorizing promotion verdict, public #90 pass, manifest, raw content
// digest, receiver identity, projected `.collective/` bytes, and compatibility policy.
// It holds/refuses unavailable, unapproved, incompatible, tampered, or locally invalid
// material. A receiver workflow can never mint a collective approval here.
//
//   verifyProposal({mirrorIdentity, mirrorCommit, releaseIdentity, receiverIdentity,
//                   proposedRevision, installation, targets}, deps) -> verification result
//
// P2 reads ONLY public mirror custody (locator/check/PR/review/protected-ref/merge) plus the
// receiver's own public PR and the pinned Git objects. It never reads private collective
// custody records (bindings, attestations) and holds no private credential: the named App's
// completed success is P2's PUBLIC historical assertion that private review and independent
// #90 were performed before success. A hidden private-only defect behind coherent public
// evidence is R2's concern, not P2's.
//
// Reuse (never reimplemented): release integrity (readAuthority/inspect), pinned Git leaf
// reader (openGitRevision), projection composer (compose) and the projection boundary check.
// Public host reads and the receiver PR read are injected adapters (deps.mirrorHost /
// deps.receiverHost) so the required check runs deterministically and offline in test.

const fs = require('node:fs');
const path = require('node:path');
const {isDeepStrictEqual: equal} = require('node:util');

const F = require('../release/source/format');
const {openGitRevision, requireCommit} = require('../release/source/git-revision');
const {readAuthority, inspect} = require('../release/integrity/verify');
const {compose} = require('../project/compose');
const {checkProjection, loadClassification} = require('../boundary/lib/projection');

// The col-007 delivery tools carry no third-party dependency of their own; they reuse only
// the format leaf (F) and validate these closed wire schemas structurally (the same posture
// as the mirror-check-locator adapter and publish-release's validatePass). Unknown fields and
// unsupported schema IDs refuse.
const LOCATOR_SCHEMA_ID = 'collective.mirror-check-locator/1';
const SEMVER = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const MS_TS = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;
const ADMISSION_PATH_RE = /^admission\/[0-9a-f]{64}\.yaml$/;
const isPlainObject = v => !!v && typeof v === 'object' && !Array.isArray(v);
const closedKeys = (v, keys) => isPlainObject(v) && equal(Object.keys(v).sort(), [...keys].sort());
const nonEmpty = v => typeof v === 'string' && v.length > 0;

function installationValid(o) {
  return closedKeys(o, ['schema', 'program_identity', 'mirror', 'canonical_branch', 'proposal_base']) &&
    o.schema === 'collective.delivery-installation/1' && REGISTRY_IDENTITY_RE.test(o.program_identity) &&
    closedKeys(o.mirror, ['repository_identity', 'accepted_ref']) &&
    nonEmpty(o.mirror.repository_identity) && nonEmpty(o.mirror.accepted_ref) &&
    nonEmpty(o.canonical_branch) && nonEmpty(o.proposal_base);
}
function targetsValid(o) {
  if (!closedKeys(o, ['schema', 'reviewed_revision', 'mirror', 'targets']) ||
      o.schema !== 'collective.delivery-targets/1' || !nonEmpty(o.reviewed_revision) ||
      !closedKeys(o.mirror, ['repository_identity', 'accepted_ref']) ||
      !nonEmpty(o.mirror.repository_identity) || !nonEmpty(o.mirror.accepted_ref) ||
      !Array.isArray(o.targets) || o.targets.length !== 2) return false;
  return o.targets.every(t => closedKeys(t, ['program_identity', 'receiver_location', 'canonical_branch', 'proposal_base', 'bootstrap']) &&
    REGISTRY_IDENTITY_RE.test(t.program_identity) && nonEmpty(t.receiver_location) &&
    nonEmpty(t.canonical_branch) && nonEmpty(t.proposal_base) && ['merged', 'pending'].includes(t.bootstrap));
}
function passProfileValid(o) {
  if (!closedKeys(o, ['schema', 'gate', 'result', 'binding', 'executed', 'recorded_by', 'recorded_at']) ||
      o.schema !== 'collective.gate-verdict/1' || o.gate !== 'pre-admission' || o.result !== 'pass' ||
      !closedKeys(o.binding, ['subject_release_identity', 'content_digest']) ||
      !SEMVER.test(o.binding.subject_release_identity) || !DIGEST.test(o.binding.content_digest) ||
      !Array.isArray(o.executed) || o.executed.length !== 4 ||
      !nonEmpty(o.recorded_by) || !MS_TS.test(o.recorded_at || '')) return false;
  return o.executed.every((e, i) => closedKeys(e, ['check', 'result']) && e.check === `PAG-0${i + 1}` && e.result === 'pass');
}
function locatorValid(o) {
  return closedKeys(o, ['schema', 'mirror_repository_identity', 'mirror_pr_number', 'mirror_base_commit',
    'mirror_head_repository_identity', 'mirror_head_commit', 'test_merge_commit', 'test_merge_tree', 'check_run_id']) &&
    o.schema === LOCATOR_SCHEMA_ID && Number.isSafeInteger(o.mirror_pr_number) && o.mirror_pr_number >= 1 &&
    Number.isSafeInteger(o.check_run_id) && o.check_run_id >= 1 &&
    nonEmpty(o.mirror_repository_identity) && nonEmpty(o.mirror_head_repository_identity) &&
    [o.mirror_base_commit, o.mirror_head_commit, o.test_merge_commit, o.test_merge_tree].every(c => COMMIT_RE.test(c || ''));
}
function reviewBindingValid(o) {
  return closedKeys(o, ['schema', 'program_identity', 'release_identity', 'content_digest',
    'mirror_repository_identity', 'mirror_commit', 'admission_path']) &&
    o.schema === 'collective.delivery-review-binding/1' && REGISTRY_IDENTITY_RE.test(o.program_identity) &&
    SEMVER.test(o.release_identity) && DIGEST.test(o.content_digest) &&
    nonEmpty(o.mirror_repository_identity) && nonEmpty(o.mirror_commit) && ADMISSION_PATH_RE.test(o.admission_path);
}
// The produced result must satisfy the closed collective.delivery-verification/1 contract:
// required identity fields, the result/condition/adoption-mode invariants, no unknown key.
function verificationShapeValid(v) {
  const allowed = ['schema', 'program_identity', 'release_identity', 'content_digest', 'checked_at',
    'result', 'condition', 'adoption_mode', 'source_revision', 'proposed_revision', 'evidence_reference'];
  if (!isPlainObject(v) || Object.keys(v).some(k => !allowed.includes(k))) return false;
  if (v.schema !== 'collective.delivery-verification/1' || !REGISTRY_IDENTITY_RE.test(v.program_identity || '') ||
      !SEMVER.test(v.release_identity || '') || !DIGEST.test(v.content_digest || '') ||
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(v.checked_at || '')) return false;
  if (v.result === 'verified')
    return v.condition === 'valid' && ['automatic-eligible', 'human-decision-required'].includes(v.adoption_mode) &&
      nonEmpty(v.source_revision) && nonEmpty(v.proposed_revision);
  if (v.result === 'held')
    return ['source-unavailable', 'observation-unavailable'].includes(v.condition) && v.adoption_mode === undefined;
  if (v.result === 'refused')
    return ['source-unapproved', 'source-invalid', 'release-unsupported', 'identity-mismatch', 'target-mismatch',
      'verdict-invalid', 'proposal-invalid', 'local-invalid', 'compatibility-invalid'].includes(v.condition) &&
      v.adoption_mode === undefined;
  return false;
}

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;
const BINDING_START = '<!-- collective-delivery-binding:start -->';
const BINDING_END = '<!-- collective-delivery-binding:end -->';
const COMMIT_RE = /^[0-9a-f]{40}(?:[0-9a-f]{24})?$/;
const REGISTRY_IDENTITY_RE = /^prg_[0-9a-f]{32}$/;
const COMPATIBILITY = Object.freeze({compatible: 'automatic-eligible', extending: 'human-decision-required',
  breaking: 'human-decision-required'});

// Control-flow signal: a non-verified outcome. `held` (public source/observation
// unavailable) and `refused` (observed public mismatch/invalid) both exit non-zero and
// never become a pass; `usage` exits 2.
class VerifyError extends Error {
  constructor(result, condition, item, message) {
    super(message);
    Object.assign(this, {result, condition, item});
  }
}
const held = (condition, item, message) => { throw new VerifyError('held', condition, item, message); };
const refused = (condition, item, message) => { throw new VerifyError('refused', condition, item, message); };
const usage = (item, message) => { throw new VerifyError('usage', 'usage-invalid', item, message); };

// Host times can be second- or millisecond-precision; normalize both to UTC milliseconds so
// the seven-day deadline is computed identically on every reader. Anything else is null and
// a chain that depends on it HOLDS (equal/missing times cannot prove order).
function normalizeTime(value) {
  if (typeof value !== 'string') return null;
  let s = value;
  if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(s)) s = s.slice(0, -1) + '.000Z';
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(s)) return null;
  if (!Number.isFinite(Date.parse(s)) || new Date(s).toISOString() !== s) return null;
  return s;
}
// Strictly-ordered chain over host times. Missing or EQUAL adjacent times HOLD (cannot prove
// custody order); a strictly reversed pair REFUSES (observed invalid order).
function assertStrictOrder(labelledTimes, holdItem, refuseItem) {
  for (let i = 0; i < labelledTimes.length - 1; i++) {
    const a = labelledTimes[i], b = labelledTimes[i + 1];
    if (a.ms === null || b.ms === null)
      held('source-unavailable', holdItem, `Missing host time (${a.label} or ${b.label})`);
    if (a.ms === b.ms)
      held('source-unavailable', holdItem, `Host times ${a.label} and ${b.label} are equal; order unprovable`);
    if (a.ms > b.ms)
      refused('source-invalid', refuseItem, `Host time ${a.label} is not strictly before ${b.label}`);
  }
}

function admissionPath(contentDigest) {
  if (typeof contentDigest !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(contentDigest))
    refused('verdict-invalid', 'content_digest', 'Accepted content digest is malformed');
  return `admission/${contentDigest.slice('sha256:'.length)}.yaml`;
}

// ---- Step 1a: closed installation and reviewed-target binding, receiver identity ----------
function bindIdentityAndTarget(input) {
  const {mirrorIdentity, receiverIdentity, installation, targets} = input;
  if (!REGISTRY_IDENTITY_RE.test(receiverIdentity || ''))
    usage('receiver_identity', 'Expected an opaque prg_ receiver identity');
  if (typeof mirrorIdentity !== 'string' || !mirrorIdentity)
    usage('mirror_identity', 'Expected a mirror repository identity');
  if (!installationValid(installation))
    refused('target-mismatch', 'installation', 'Receiver installation violates its closed schema');
  if (!targetsValid(targets))
    refused('target-mismatch', 'targets', 'Reviewed target map violates its closed schema');
  if (installation.program_identity !== receiverIdentity)
    refused('identity-mismatch', 'installation.program_identity', 'Installation identity differs from checked receiver');
  if (installation.mirror.repository_identity !== mirrorIdentity)
    refused('target-mismatch', 'installation.mirror', 'Installation mirror identity differs from pinned mirror');
  const matched = targets.targets.filter(t => t.program_identity === receiverIdentity);
  if (matched.length !== 1)
    refused('target-mismatch', 'targets', 'Reviewed target map does not bind exactly one entry for the receiver');
  const target = matched[0];
  if (targets.mirror.repository_identity !== mirrorIdentity ||
      targets.mirror.accepted_ref !== installation.mirror.accepted_ref)
    refused('target-mismatch', 'targets.mirror', 'Target mirror identity or accepted ref differs from installation');
  if (target.canonical_branch !== installation.canonical_branch ||
      target.proposal_base !== installation.proposal_base)
    refused('target-mismatch', 'targets.target', 'Target branch bindings differ from installation');
  if (!/^refs\/heads\/[A-Za-z0-9][A-Za-z0-9._/-]*$/.test(installation.mirror.accepted_ref) ||
      installation.mirror.accepted_ref.includes('..'))
    refused('target-mismatch', 'installation.mirror.accepted_ref', 'Malformed mirror accepted ref');
  return {target, receiverLocation: target.receiver_location,
    acceptedRef: installation.mirror.accepted_ref};
}

// ---- Step 2: release integrity from the pinned mirror commit ------------------------------
function verifyReleaseIntegrity(mirrorSource, input) {
  const {releaseIdentity, receiverIdentity} = input;
  if (!F.validVersion(releaseIdentity)) usage('release_identity', 'Expected explicit semver release identity');
  let authority;
  try { authority = readAuthority(mirrorSource); }
  catch (error) {
    if (error instanceof F.ReleaseError && error.state === 'source-unavailable')
      held('source-unavailable', 'mirror-index', error.message);
    refused('source-invalid', 'mirror-index', error && error.message ? error.message : String(error));
  }
  const selected = authority.known.get(releaseIdentity);
  if (!selected) refused('source-unapproved', releaseIdentity, 'Release identity absent from the accepted mirror index');
  // Authorizing promotion verdict (readAuthority already bound it to the entry; re-assert it
  // is an authorizing pass, never a candidate or failing verdict).
  if (selected.verdict.result !== 'pass' || selected.verdict.gate !== 'pre-promotion')
    refused('verdict-invalid', releaseIdentity, 'Promotion verdict is not an authorizing pre-promotion pass');
  // Complete raw release tree: bytes, modes, sizes, manifest and whole-content digest.
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
  if (!Array.isArray(registrations.programs) ||
      !registrations.programs.some(p => p.program_identity === receiverIdentity))
    refused('identity-mismatch', 'REGISTRATIONS.yaml', 'Receiver identity is not registered in this release');
  // Public #90 pass profile, OUTSIDE the release tree, bound to the accepted index.
  const passPath = admissionPath(selected.entry.content_digest);
  let passBytes;
  try { passBytes = mirrorSource.readFile(passPath).bytes; }
  catch (error) {
    if (error instanceof F.ReleaseError && error.state === 'source-unavailable')
      held('source-unavailable', passPath, 'Public #90 pass absent from mirror snapshot');
    refused('verdict-invalid', passPath, error && error.message ? error.message : String(error));
  }
  let pass;
  try { pass = JSON.parse(F.utf8(passBytes)); }
  catch { refused('verdict-invalid', passPath, 'Public #90 pass is not valid JSON'); }
  if (!passProfileValid(pass))
    refused('verdict-invalid', passPath, 'Public #90 pass violates the closed gate-verdict profile');
  if (pass.binding.subject_release_identity !== releaseIdentity ||
      pass.binding.content_digest !== selected.entry.content_digest)
    refused('verdict-invalid', passPath, 'Public #90 pass binding differs from the accepted index identity/digest');
  return {authority, selected, passPath, contentDigest: selected.entry.content_digest,
    compatibility: selected.entry.metadata.compatibility};
}

// ---- Step 1b: public mirror accepted custody (locator/check/PR/review/ref/merge) ----------
async function readMirrorCustody(input, binding, deps) {
  const {mirrorIdentity, mirrorCommit} = input;
  const {acceptedRef} = binding;
  const {owner, checkName, appId, actor} = deps.policy || {};
  if (!owner || !checkName || !Number.isSafeInteger(appId) || appId < 1 || !actor)
    usage('policy', 'Mirror owner, check name, App ID and locator actor are required public policy');
  requireCommit(mirrorCommit);
  const branch = acceptedRef.slice('refs/heads/'.length);
  const enc = branch.split('/').map(encodeURIComponent).join('/');
  const get = async suffix => {
    try { return await deps.mirrorHost.get(suffix); }
    catch (error) {
      if (error && error.malformed) refused('source-invalid', suffix, 'Malformed mirror host response');
      held('source-unavailable', suffix, (error && error.message) || 'Mirror host read unavailable');
    }
  };
  const paged = async build => {
    const out = [];
    for (let page = 1; page <= 20; page++) {
      const batch = await get(build(page));
      if (!Array.isArray(batch)) refused('source-invalid', build(page), 'Malformed mirror host list');
      out.push(...batch);
      if (batch.length < 100) return out;
      if (page === 20) held('source-unavailable', build(page), 'Mirror host history exceeds read limit');
    }
    return out;
  };

  // Protected accepted ref head.
  const ref = await get(`git/ref/heads/${enc}`);
  if (ref?.ref !== acceptedRef || ref?.object?.type !== 'commit' || !/^[a-f0-9]{40}$/.test(ref?.object?.sha || ''))
    refused('source-invalid', acceptedRef, 'Protected mirror ref read contradicts configured accepted ref');
  const acceptedHead = ref.object.sha;

  // Effective strict protected-branch rule with the named App required check.
  const protection = await get(`branches/${enc}/protection`);
  const statuses = protection?.required_status_checks;
  const reviews = protection?.required_pull_request_reviews;
  const bypass = reviews?.bypass_pull_request_allowances;
  const noBypass = !bypass || Object.values(bypass).every(v => Array.isArray(v) && v.length === 0);
  const requiredCheck = (statuses?.checks || []).filter(c => c.context === checkName);
  if (statuses?.strict !== true || requiredCheck.length !== 1 || requiredCheck[0].app_id !== appId ||
      !Number.isInteger(reviews?.required_approving_review_count) || reviews.required_approving_review_count < 1 ||
      reviews.dismiss_stale_reviews !== true || protection?.enforce_admins?.enabled !== true ||
      protection?.allow_force_pushes?.enabled !== false || protection?.allow_deletions?.enabled !== false ||
      protection?.required_linear_history?.enabled === true || protection?.required_merge_queue?.enabled === true ||
      !noBypass)
    refused('source-invalid', acceptedRef, 'Protected mirror rule lacks strict named-App check, owner review, or no-bypass controls');

  // Merge-commit-only repository settings.
  const settings = await get('');
  if (settings?.full_name !== mirrorIdentity || settings?.allow_merge_commit !== true ||
      settings?.allow_squash_merge !== false || settings?.allow_rebase_merge !== false)
    refused('source-invalid', mirrorIdentity, 'Mirror repository is not merge-commit-only');

  // The pinned accepted commit must descend from the protected ref.
  const compare = await get(`compare/${mirrorCommit}...${acceptedHead}`);
  if (compare?.base_commit?.sha !== mirrorCommit || compare?.head_commit?.sha !== acceptedHead ||
      !['ahead', 'identical', 'diverged', 'behind'].includes(compare?.status))
    refused('source-invalid', mirrorCommit, 'Contradictory mirror ancestry response');
  if (!['ahead', 'identical'].includes(compare.status))
    refused('source-unapproved', mirrorCommit, 'Pinned mirror commit is outside the protected accepted ref');

  // The single owner-reviewed merged PR that produced the accepted commit.
  const pulls = await paged(page => `commits/${mirrorCommit}/pulls?per_page=100&page=${page}`);
  const candidates = pulls.filter(p => p?.merge_commit_sha === mirrorCommit && p?.base?.ref === branch &&
    p?.base?.repo?.full_name === mirrorIdentity && p?.merged_at);
  if (!candidates.length)
    refused('source-unapproved', mirrorCommit, 'No owner-reviewed merged mirror PR binds the accepted commit');
  if (candidates.length !== 1)
    refused('source-invalid', mirrorCommit, 'Multiple conflicting merged PRs bind the accepted commit');
  const pr = candidates[0];
  if (!Number.isInteger(pr.number) || pr.number < 1 || !COMMIT_RE.test(pr?.head?.sha || '') ||
      typeof pr?.head?.repo?.full_name !== 'string' || !pr.head.repo.full_name)
    refused('source-invalid', mirrorCommit, 'Malformed merged mirror PR identity');
  const mergedAt = normalizeTime(pr.merged_at);

  // Separate same-head owner approval.
  const reviewRecords = await paged(page => `pulls/${pr.number}/reviews?per_page=100&page=${page}`);
  const ownerReview = reviewRecords.filter(r => r?.user?.login === owner && r?.submitted_at &&
    Number.isFinite(Date.parse(r.submitted_at))).sort((a, b) => Date.parse(a.submitted_at) - Date.parse(b.submitted_at)).at(-1);
  if (ownerReview?.state !== 'APPROVED' || ownerReview.commit_id !== pr.head.sha || !Number.isInteger(ownerReview.id))
    refused('source-unapproved', String(pr.number), 'Separate same-head owner approval absent on merged mirror PR');

  // Exactly one unedited locator-only comment from the configured actor.
  const comments = await paged(page => `issues/${pr.number}/comments?per_page=100&page=${page}`);
  const mine = [];
  for (const comment of comments) {
    if (comment?.user?.login !== actor) continue;
    let value;
    try { value = JSON.parse(F.utf8(Buffer.from(String(comment.body ?? '')))); }
    catch { continue; }
    if (!value || value.schema !== LOCATOR_SCHEMA_ID) continue;
    mine.push({comment, value});
  }
  if (mine.length === 0) held('source-unavailable', String(pr.number), 'No public mirror locator comment from the configured actor');
  if (mine.length !== 1) refused('source-invalid', String(pr.number), 'More than one locator comment from the configured actor');
  const {comment: locComment, value: locator} = mine[0];
  if (!locatorValid(locator))
    refused('source-invalid', String(pr.number), 'Locator comment violates the closed public schema');
  const body = String(locComment.body ?? '');
  if (F.utf8(Buffer.from(body)) !== JSON.stringify(locator) + '\n' ||
      !equal(locator, F.parseDocument(Buffer.from(body))))
    refused('source-invalid', String(pr.number), 'Locator comment body is not the exact closed locator object');
  const created = normalizeTime(locComment.created_at);
  if (locComment.created_at == null || locComment.updated_at == null || locComment.created_at !== locComment.updated_at)
    refused('source-invalid', String(pr.number), 'Locator comment is edited or missing creation time');
  // Locator coordinates must match the live PR pair.
  if (locator.mirror_repository_identity !== mirrorIdentity || locator.mirror_pr_number !== pr.number ||
      locator.mirror_head_commit !== pr.head.sha || locator.mirror_head_repository_identity !== pr.head.repo.full_name)
    refused('source-invalid', String(pr.number), 'Locator coordinates differ from the live merged PR');

  // The reserved named-App run, read strictly BY the locator ID; require completed success on
  // the locator's checked test-merge SHA. A second/head/accepted-SHA run cannot substitute.
  const run = await get(`check-runs/${locator.check_run_id}`);
  const startedAt = normalizeTime(run?.started_at);
  if (run?.id !== locator.check_run_id || run?.app?.id !== appId || run?.name !== checkName ||
      !COMMIT_RE.test(run?.head_sha || '') || !startedAt)
    refused('source-invalid', String(locator.check_run_id), 'Reserved run identity differs from locator or is malformed');
  if (run.status !== 'completed' || run.conclusion !== 'success')
    refused('source-invalid', String(locator.check_run_id), 'Reserved named-App run is not completed successfully');
  if (run.head_sha !== locator.test_merge_commit)
    refused('source-invalid', String(locator.check_run_id), 'Reserved run head SHA differs from the checked test merge');
  const completedAt = normalizeTime(run.completed_at);

  // The accepted merge commit: exactly two ordered parents (locator base/head) and the checked
  // test-merge tree. Squash/rebase leave the wrong parent shape and refuse here.
  const mergeCommit = await get(`git/commits/${mirrorCommit}`);
  const parents = mergeCommit?.parents?.map(p => p.sha);
  const mergeTree = mergeCommit?.tree?.sha;
  if (mergeCommit?.sha !== mirrorCommit || !equal(parents, [locator.mirror_base_commit, locator.mirror_head_commit]) ||
      !COMMIT_RE.test(mergeTree || ''))
    refused('source-invalid', mirrorCommit, 'Accepted merge commit lacks two ordered base/head parents');
  if (mergeTree !== locator.test_merge_tree)
    refused('source-invalid', mirrorCommit, 'Accepted merge commit tree differs from the checked test-merge tree');

  // Public time order: run start < locator creation < completion < merge, strict at returned
  // precision (equal/missing HOLD), and completion + merge strictly before the seven-day
  // deadline computed from the normalized run start.
  assertStrictOrder([
    {label: 'run.started_at', ms: startedAt === null ? null : Date.parse(startedAt)},
    {label: 'locator.created_at', ms: created === null ? null : Date.parse(created)},
    {label: 'run.completed_at', ms: completedAt === null ? null : Date.parse(completedAt)},
    {label: 'pr.merged_at', ms: mergedAt === null ? null : Date.parse(mergedAt)},
  ], String(pr.number), String(pr.number));
  const deadline = Date.parse(startedAt) + SEVEN_DAYS_MS;
  if (!(Date.parse(completedAt) < deadline && Date.parse(mergedAt) < deadline))
    refused('source-invalid', String(pr.number), 'Completion or merge is not strictly before the seven-day deadline');

  return Object.freeze({prNumber: pr.number, acceptedMergeCommit: mirrorCommit, checkRunId: locator.check_run_id,
    locatorCommentId: locComment.id, checkedTestMerge: run.head_sha, testMergeTree: mergeTree,
    headCommit: pr.head.sha, startedAt, completedAt, mergedAt});
}

// ---- Step 3: independent receiver PR body review binding ----------------------------------
async function readReceiverReviewBinding(input, binding, release, deps) {
  const {receiverLocation} = binding;
  const {proposedRevision, mirrorIdentity, mirrorCommit, releaseIdentity, receiverIdentity} = input;
  const get = async suffix => {
    try { return await deps.receiverHost.get(suffix); }
    catch (error) {
      if (error && error.malformed) refused('proposal-invalid', suffix, 'Malformed receiver host response');
      held('observation-unavailable', suffix, (error && error.message) || 'Receiver PR read unavailable');
    }
  };
  const pulls = await get(`commits/${proposedRevision}/pulls?per_page=100&page=1`);
  if (!Array.isArray(pulls)) refused('proposal-invalid', 'receiver-pulls', 'Malformed receiver PR list');
  const matched = pulls.filter(p => p?.head?.sha === proposedRevision &&
    p?.head?.repo?.full_name === receiverLocation);
  if (matched.length === 0) held('observation-unavailable', proposedRevision, 'No receiver PR exposes the proposed revision');
  if (matched.length !== 1) refused('proposal-invalid', proposedRevision, 'Multiple receiver PRs claim the proposed revision');
  const prBody = matched[0].body;
  if (typeof prBody !== 'string') refused('proposal-invalid', proposedRevision, 'Receiver PR body is unreadable');

  // Exactly one start/end marker pair containing exactly one fenced JSON object.
  if (prBody.split(BINDING_START).length !== 2 || prBody.split(BINDING_END).length !== 2)
    refused('proposal-invalid', proposedRevision, 'Receiver PR body lacks exactly one review-binding marker pair');
  const between = prBody.slice(prBody.indexOf(BINDING_START) + BINDING_START.length, prBody.indexOf(BINDING_END));
  if (prBody.indexOf(BINDING_END) < prBody.indexOf(BINDING_START))
    refused('proposal-invalid', proposedRevision, 'Review-binding markers are out of order');
  const fences = [...between.matchAll(/```json\n([\s\S]*?)\n```/g)];
  if (fences.length !== 1)
    refused('proposal-invalid', proposedRevision, 'Review-binding block does not carry exactly one fenced JSON object');
  let block;
  try { block = JSON.parse(fences[0][1]); }
  catch { refused('proposal-invalid', proposedRevision, 'Review-binding block is not valid JSON'); }
  if (!reviewBindingValid(block))
    refused('proposal-invalid', proposedRevision, 'Review-binding block violates its closed schema');
  if (block.program_identity !== receiverIdentity || block.release_identity !== releaseIdentity ||
      block.content_digest !== release.contentDigest || block.mirror_repository_identity !== mirrorIdentity ||
      block.mirror_commit !== mirrorCommit || block.admission_path !== release.passPath)
    refused('proposal-invalid', proposedRevision, 'Review-binding pointers differ from the independently verified source');
  // The #90 pass reference must also be RENDERED as ordinary review text outside the block.
  const outside = prBody.slice(0, prBody.indexOf(BINDING_START)) + prBody.slice(prBody.indexOf(BINDING_END) + BINDING_END.length);
  if (!outside.includes(release.passPath))
    refused('proposal-invalid', proposedRevision, 'Receiver PR does not render the #90 pass reference outside the projected binding');
  return block;
}

// ---- Step 4: expected projection vs proposed tree, local validity, compatibility ----------
function verifyProjectionAndLocal(receiverSource, release, input, deps) {
  const {selected} = release;
  const {receiverIdentity} = input;
  // Deterministic expected projection from the verified release + accepted registration.
  const manifestEntry = selected.entries.find(e => e.path === 'MANIFEST.yaml');
  let manifest;
  try { manifest = F.parseDocument(manifestEntry.bytes, 'manifest', 'MANIFEST.yaml'); }
  catch (error) { refused('source-invalid', 'MANIFEST.yaml', error.message); }
  const resolvedRelease = {
    release_identity: selected.entry.metadata.release_identity,
    content_digest: selected.entry.content_digest,
    files: new Map(selected.entries.map(e => [e.path, Buffer.from(e.bytes)])),
    manifest,
  };
  let expected;
  try { expected = compose(resolvedRelease, {program_identity: receiverIdentity}); }
  catch (error) { refused('proposal-invalid', 'projection', `Expected projection could not be composed: ${error && error.message ? error.message : error}`); }

  // Read the proposed receiver `.collective/` tree from the pinned proposed revision.
  let proposedEntries;
  try { proposedEntries = receiverSource.readTree('.collective', {optional: false}); }
  catch (error) {
    if (error instanceof F.ReleaseError && error.state === 'source-unavailable')
      held('observation-unavailable', '.collective', 'Proposed `.collective/` tree is absent from the proposed revision');
    refused('proposal-invalid', '.collective', error && error.message ? error.message : String(error));
  }
  const proposedByPath = new Map(proposedEntries.map(e => [`.collective/${e.path}`, e]));
  const expectedByPath = new Map(expected.files.map(f => [f.path, f]));

  // Byte/mode compare every BASELINE-owned projected path; program-authored paths (EXTENSIONS)
  // may carry program bytes but must still be present and within the closed set.
  for (const file of expected.files) {
    const actual = proposedByPath.get(file.path);
    if (!actual) refused('proposal-invalid', file.path, 'Proposed revision is missing an expected projected path');
    if (actual.mode !== '0644') refused('proposal-invalid', file.path, 'Projected path mode is not 0644');
    if (file.authority === 'baseline' && !equal(Buffer.from(actual.bytes), Buffer.from(file.bytes)))
      refused('proposal-invalid', file.path, 'Baseline-owned projected bytes differ from the expected projection');
  }
  for (const proposedPath of proposedByPath.keys())
    if (!expectedByPath.has(proposedPath))
      refused('proposal-invalid', proposedPath, 'Proposed revision carries an unexpected projected path');

  // Invoke the receiver's REAL local validator on the exact proposed tree; retain its exit.
  const record = {release_identity: selected.entry.metadata.release_identity, content_digest: selected.entry.content_digest};
  const proposedBundle = proposedEntries.map(e => ({path: `.collective/${e.path}`, bytes: Buffer.from(e.bytes)}));
  const validator = deps.localValidator || defaultLocalValidator(deps);
  let localResult;
  try { localResult = validator(proposedBundle, record); }
  catch (error) { refused('local-invalid', 'local-validator', error && error.message ? error.message : String(error)); }
  if (!localResult || localResult.ok !== true)
    refused('local-invalid', 'local-validator',
      `Program-local validation refused the proposed tree: ${JSON.stringify(localResult && localResult.violations || localResult)}`);

  // Compatibility against released semantics and local policy.
  const level = release.compatibility;
  if (!Object.prototype.hasOwnProperty.call(COMPATIBILITY, level))
    refused('compatibility-invalid', level || 'compatibility', 'Released compatibility level is not recognised');
  const rejected = deps.localPolicy && deps.localPolicy.rejectCompatibility;
  if (rejected && (rejected instanceof Set ? rejected.has(level) : Array.isArray(rejected) && rejected.includes(level)))
    refused('compatibility-invalid', level, 'Receiver-local policy rejects this compatibility level');
  return {adoptionMode: adoptionModeFor(level)};
}
// `compatible` alone is automatic-adoption eligible under baseline §1 (col-007 never merges);
// `extending`/`breaking` require the Program's human adoption decision.
function adoptionModeFor(level) { return COMPATIBILITY[level]; }
function defaultLocalValidator(deps) {
  const classification = deps.classification || loadClassification();
  return (bundle, record) => checkProjection(bundle, record, classification);
}

// ---- Orchestration -----------------------------------------------------------------------
async function verifyProposal(input, deps = {}) {
  const now = () => new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const base = {
    schema: 'collective.delivery-verification/1',
    program_identity: input && input.receiverIdentity,
    release_identity: input && input.releaseIdentity,
    content_digest: null,
    checked_at: now(),
  };
  try {
    if (!input || typeof input !== 'object') usage('input', 'Verification input object required');
    requireCommitOrUsage(input.proposedRevision, 'proposed_revision');
    const binding = bindIdentityAndTarget(input);
    const mirrorSource = deps.mirrorSource || openGitRevision(requireGitDir(deps.mirrorGitDir, 'mirror'), requireCommitOrUsage(input.mirrorCommit, 'mirror_commit'));
    const release = verifyReleaseIntegrity(mirrorSource, input);
    base.content_digest = release.contentDigest;
    await readMirrorCustody(input, binding, deps);
    const receiverSource = deps.receiverSource || openGitRevision(requireGitDir(deps.receiverGitDir, 'receiver'), input.proposedRevision);
    await readReceiverReviewBinding(input, binding, release, deps);
    const {adoptionMode} = verifyProjectionAndLocal(receiverSource, release, input, deps);
    const verification = Object.freeze({...base, result: 'verified', condition: 'valid', adoption_mode: adoptionMode,
      source_revision: input.mirrorCommit, proposed_revision: input.proposedRevision});
    assertSchema(verification);
    return {verification, exitCode: 0};
  } catch (error) {
    if (error instanceof VerifyError) {
      if (error.result === 'usage')
        return {verification: Object.freeze({...base, content_digest: base.content_digest || null,
          result: 'refused', condition: 'source-invalid', evidence_reference: error.item}), exitCode: 2,
          usage: true, message: error.message};
      const verification = {...base, content_digest: base.content_digest || placeholderDigest(),
        result: error.result, condition: error.condition};
      if (error.item) verification.evidence_reference = String(error.item);
      const frozen = Object.freeze(verification);
      assertSchema(frozen);
      return {verification: frozen, exitCode: 3, message: error.message};
    }
    // Unexpected: fail closed as a source hold rather than a pass.
    const verification = Object.freeze({...base, content_digest: base.content_digest || placeholderDigest(),
      result: 'held', condition: 'source-unavailable', evidence_reference: 'unexpected'});
    return {verification, exitCode: 3, message: error && error.message ? error.message : String(error)};
  }
}
// A non-verified result still must satisfy the closed output schema's required content_digest
// shape; when the failure precedes digest discovery we emit a syntactically valid placeholder
// (zeroed) digest and never a pass.
function placeholderDigest() { return 'sha256:' + '0'.repeat(64); }
function requireCommitOrUsage(value, item) {
  try { return requireCommit(value); }
  catch { usage(item, 'Expected a full immutable commit ID'); }
}
function requireGitDir(value, which) {
  if (typeof value !== 'string' || !value) usage(`${which}_git_dir`, `Configured ${which} Git directory required`);
  return value;
}
function assertSchema(verification) {
  if (!verificationShapeValid(verification))
    throw new Error('Produced verification violates its own closed schema');
}

// ---- CLI ---------------------------------------------------------------------------------
function fetchHost(repository, apiUrl, token) {
  const api = new URL(apiUrl || 'https://api.github.com/');
  return {
    async get(suffix) {
      let response;
      try {
        response = await fetch(new URL(`repos/${repository}/${suffix}`, api), {
          headers: {Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28',
            ...(token ? {Authorization: `Bearer ${token}`} : {})},
          signal: AbortSignal.timeout(15000),
        });
      } catch { const e = new Error('host request unavailable'); e.unavailable = true; throw e; }
      if (!response.ok) { const e = new Error(`host HTTP ${response.status}`); e.unavailable = true; throw e; }
      try { return await response.json(); }
      catch { const e = new Error('malformed host response'); e.malformed = true; throw e; }
    },
  };
}
async function main(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    const flag = argv[i];
    if (!['--mirror-identity', '--mirror-commit', '--release', '--receiver', '--proposed-revision',
      '--installation', '--targets'].includes(flag) || args[flag] !== undefined || argv[i + 1] === undefined) {
      process.stderr.write('Invalid verify-proposal arguments\n');
      return 2;
    }
    args[flag] = argv[i + 1];
  }
  let installation, targets;
  try {
    installation = JSON.parse(fs.readFileSync(args['--installation'], 'utf8'));
    targets = F.parseDocument(fs.readFileSync(args['--targets']));
  } catch (error) {
    process.stdout.write(JSON.stringify({result: 'held', condition: 'observation-unavailable',
      message: `installation/targets unreadable: ${error.message}`}) + '\n');
    return 3;
  }
  const input = {mirrorIdentity: args['--mirror-identity'], mirrorCommit: args['--mirror-commit'],
    releaseIdentity: args['--release'], receiverIdentity: args['--receiver'],
    proposedRevision: args['--proposed-revision'], installation, targets};
  const deps = {
    mirrorGitDir: process.env.COLLECTIVE_MIRROR_GIT_DIR,
    receiverGitDir: process.env.COLLECTIVE_RECEIVER_GIT_DIR || process.cwd(),
    mirrorHost: fetchHost(args['--mirror-identity'], process.env.COLLECTIVE_MIRROR_HOST_API_URL, process.env.COLLECTIVE_MIRROR_PUBLIC_TOKEN),
    receiverHost: fetchHost(installation && installation.mirror && targets ? undefinedReceiver(targets, args['--receiver']) : '',
      process.env.COLLECTIVE_RECEIVER_HOST_API_URL, process.env.COLLECTIVE_RECEIVER_HOST_TOKEN),
    policy: {owner: process.env.COLLECTIVE_MIRROR_OWNER_LOGIN, checkName: process.env.COLLECTIVE_MIRROR_LANDING_CHECK,
      appId: Number(process.env.COLLECTIVE_MIRROR_CHECK_APP_ID), actor: process.env.COLLECTIVE_MIRROR_LOCATOR_ACTOR},
  };
  const {verification, exitCode} = await verifyProposal(input, deps);
  process.stdout.write(JSON.stringify(verification) + '\n');
  return exitCode;
}
function undefinedReceiver(targets, receiver) {
  const t = (targets.targets || []).find(x => x.program_identity === receiver);
  return t ? t.receiver_location : '';
}
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; });

module.exports = {verifyProposal, normalizeTime, admissionPath, adoptionModeFor};
