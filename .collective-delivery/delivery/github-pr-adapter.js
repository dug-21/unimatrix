'use strict';

// Transport-only receiver PR adapter (P1 step 5, col-007).
//
// It is the replaceable host seam for the receiver proposal: it creates ONE proposal branch
// against the receiver's explicit proposal base, stages ONLY the deterministic `.collective/`
// projection onto the base tree, pushes that branch and opens (or locates) ONE reviewable PR,
// then reads the PR back from the host to record diagnostic coordinates. It carries the review
// body verbatim (the review-binding block + off-payload #90 reference are built by the caller,
// never here) and it NEVER merges, approves, or writes any collective/adoption record.
//
//   openProposal(proposal, deps) -> { receiverRepositoryIdentity, number, url, baseCommit,
//                                     proposedRevision } | throws PrAdapterError
//
// The receiver credential may push a branch and open a PR in its OWN repository but must have
// no administration/maintain grant (which alone could bypass branch protection and merge). The
// host transport is injected (`deps.host`) so this adapter is fully offline and replaceable in
// tests; the git plumbing runs against a real receiver working repo (`deps.gitDir`).

const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const F = require('../release/source/format');
const { openGitRevision } = require('../release/source/git-revision');

const PROJECTED_PREFIX = '.collective/';
const BRANCH_RE = /^[A-Za-z0-9][A-Za-z0-9._/-]*$/;

class PrAdapterError extends Error {
  // `pushed` records whether the proposal branch was already pushed when the failure occurred,
  // so the caller can distinguish `prepared/failed` (branch pushed, PR open failed) from a
  // pre-push preparation failure. `stage` is one of prepare | push | open | readback.
  constructor(state, item, message, { stage = 'prepare', pushed = false } = {}) {
    super(message);
    Object.assign(this, { name: 'PrAdapterError', state, item, stage, pushed, code: 3 });
  }
}
function fail(state, item, message, meta) { throw new PrAdapterError(state, item, message, meta); }

function gitEnv() {
  return {
    ...Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith('GIT_'))),
    GIT_NO_REPLACE_OBJECTS: '1', GIT_OPTIONAL_LOCKS: '0', LC_ALL: 'C',
  };
}
function makeGit(gitDir) {
  const env = gitEnv();
  return (args, opts = {}) => execFileSync('git', ['-C', gitDir, ...args],
    { env, maxBuffer: 128 * 1024 * 1024, encoding: opts.encoding, input: opts.input, stdio: opts.stdio });
}

// Independently reconfirm the built head is the base tree with ONLY `.collective/` additions or
// changes and nothing else — the transport must not silently smuggle extra paths.
function verifyProjectionDiff(git, gitDir, baseCommit, headCommit, projection) {
  try { git(['merge-base', '--is-ancestor', baseCommit, headCommit], { stdio: 'ignore' }); }
  catch { fail('proposal-invalid', headCommit, 'Proposal head does not descend from the proposal base'); }
  let raw;
  try { raw = git(['diff', '--name-status', '-z', baseCommit, headCommit]); }
  catch { fail('proposal-invalid', headCommit, 'Proposal diff unreadable'); }
  const parts = F.utf8(raw).split('\0').filter(Boolean);
  const changed = new Set();
  for (let i = 0; i < parts.length; i += 2) {
    const status = parts[i]; const p = parts[i + 1];
    if (!p || !['A', 'M'].includes(status) || !p.startsWith(PROJECTED_PREFIX))
      fail('proposal-invalid', p || headCommit, 'Proposal diff contains a non-projection change');
    changed.add(p);
  }
  const expected = new Set(projection.map(e => e.path));
  if (!equal([...changed].sort(), [...expected].sort()))
    fail('proposal-invalid', headCommit, 'Proposal diff differs from the composed projection');
  const source = openGitRevision(gitDir, headCommit);
  for (const entry of projection) {
    const file = source.readFile(entry.path);
    if (file.mode !== '0644' || !file.bytes.equals(Buffer.from(entry.bytes)))
      fail('proposal-invalid', entry.path, 'Proposal head bytes differ from the composed projection');
  }
}

// Build the proposal head commit: base tree + exactly the staged `.collective/` projection.
function buildProposalHead(git, gitDir, baseCommit, projection) {
  const indexFile = execFileSync('mktemp', { encoding: 'utf8' }).trim();
  try {
    let baseTree;
    try { baseTree = git(['rev-parse', `${baseCommit}^{tree}`], { encoding: 'ascii' }).trim(); }
    catch { fail('proposal-invalid', baseCommit, 'Proposal base commit absent from the receiver object store'); }
    const ienv = { GIT_INDEX_FILE: indexFile };
    const igit = (args, opts = {}) => execFileSync('git', ['-C', gitDir, ...args],
      { env: { ...gitEnv(), ...ienv }, encoding: opts.encoding, input: opts.input });
    igit(['read-tree', baseTree]);
    for (const entry of projection) {
      F.safePath(entry.path);
      if (!entry.path.startsWith(PROJECTED_PREFIX))
        fail('proposal-invalid', entry.path, 'Projection contains a path outside `.collective/`');
      if (entry.mode && entry.mode !== '0644')
        fail('proposal-invalid', entry.path, 'Projected file mode must be 0644');
      const blob = igit(['hash-object', '-w', '--stdin'], { input: Buffer.from(entry.bytes), encoding: 'ascii' }).trim();
      igit(['update-index', '--add', '--cacheinfo', `100644,${blob},${entry.path}`]);
    }
    const tree = igit(['write-tree'], { encoding: 'ascii' }).trim();
    const head = git(['-c', 'user.name=col-007-receiver', '-c', 'user.email=receiver@example.invalid',
      'commit-tree', tree, '-p', baseCommit, '-m', 'Adopt collective release proposal'],
      { encoding: 'ascii' }).trim();
    return head;
  } finally { try { execFileSync('rm', ['-f', indexFile]); } catch { /* best effort */ } }
}

// The receiver credential must not be able to administer/merge the protected canonical branch.
async function assertLeastPrivilege(host, receiverRepository) {
  let perms;
  try { perms = await host.permissions(); }
  catch (error) { fail('proposal-invalid', receiverRepository, `Receiver repository permissions unreadable: ${error && error.message || error}`); }
  if (!perms || typeof perms !== 'object')
    fail('proposal-invalid', receiverRepository, 'Receiver repository permissions unreadable');
  if (perms.admin === true || perms.maintain === true)
    fail('proposal-invalid', receiverRepository, 'Receiver credential carries administration/merge grant on the canonical branch');
}

async function openProposal(proposal, deps = {}) {
  const p = proposal || {};
  const { gitDir, host } = deps;
  if (typeof gitDir !== 'string' || !gitDir) fail('usage-invalid', 'gitDir', 'Receiver Git working directory required');
  if (!host || typeof host.openOrLocatePull !== 'function' || typeof host.readPull !== 'function' ||
      typeof host.pushBranch !== 'function' || typeof host.permissions !== 'function')
    fail('usage-invalid', 'host', 'Injected receiver PR transport (permissions/pushBranch/openOrLocatePull/readPull) required');
  if (typeof p.receiverRepository !== 'string' || !p.receiverRepository)
    fail('usage-invalid', 'receiverRepository', 'Receiver repository identity required');
  if (typeof p.proposalBase !== 'string' || !BRANCH_RE.test(p.proposalBase) || p.proposalBase.includes('..'))
    fail('usage-invalid', 'proposalBase', 'Explicit proposal base branch required');
  if (typeof p.branch !== 'string' || !BRANCH_RE.test(p.branch) || p.branch.includes('..'))
    fail('usage-invalid', 'branch', 'Deterministic proposal branch name required');
  if (!Array.isArray(p.projection) || p.projection.length === 0)
    fail('usage-invalid', 'projection', 'Composed `.collective/` projection required');
  if (typeof p.body !== 'string' || !p.body) fail('usage-invalid', 'body', 'Review body required');

  const git = makeGit(gitDir);
  await assertLeastPrivilege(host, p.receiverRepository);

  // Resolve the explicit proposal base to an immutable commit from the receiver's own repo.
  let baseCommit;
  try { baseCommit = git(['rev-parse', `${p.proposalBase}^{commit}`], { encoding: 'ascii' }).trim(); }
  catch { fail('proposal-invalid', p.proposalBase, 'Proposal base branch is absent from the receiver repository'); }
  if (!/^[0-9a-f]{40}$/.test(baseCommit)) fail('proposal-invalid', p.proposalBase, 'Proposal base did not resolve to a commit');

  const headCommit = buildProposalHead(git, gitDir, baseCommit, p.projection);
  verifyProjectionDiff(git, gitDir, baseCommit, headCommit, p.projection);

  // Update the deterministic local branch ref, then push it through the transport.
  try { git(['update-ref', `refs/heads/${p.branch}`, headCommit]); }
  catch { fail('proposal-invalid', p.branch, 'Could not update the local proposal branch ref'); }
  try { await host.pushBranch(p.branch, headCommit); }
  catch (error) { fail('proposal-invalid', p.branch, `Proposal branch push failed: ${error && error.message || error}`, { stage: 'push', pushed: false }); }

  // Open or locate one reviewable PR against the explicit base; the branch is now pushed, so a
  // subsequent failure is `prepared/failed`, not `opened`.
  let pr;
  try {
    pr = await host.openOrLocatePull({ base: p.proposalBase, branch: p.branch, headSha: headCommit,
      title: p.title || 'Adopt collective release proposal', body: p.body });
  } catch (error) { fail('proposal-invalid', p.branch, `Opening the proposal PR failed: ${error && error.message || error}`, { stage: 'open', pushed: true }); }
  if (!pr || !Number.isSafeInteger(pr.number) || pr.number < 1)
    fail('proposal-invalid', p.branch, 'Proposal PR open returned no PR number', { stage: 'open', pushed: true });

  let observed;
  try { observed = await host.readPull(pr.number); }
  catch (error) { fail('proposal-invalid', String(pr.number), `Proposal PR readback failed: ${error && error.message || error}`, { stage: 'readback', pushed: true }); }
  if (!observed || observed.number !== pr.number || observed.state !== 'open' ||
      observed.head?.sha !== headCommit || observed.base?.ref !== p.proposalBase)
    fail('proposal-invalid', String(pr.number), 'Proposal PR readback differs from the prepared base/head', { stage: 'readback', pushed: true });

  return Object.freeze({
    receiverRepositoryIdentity: p.receiverRepository,
    number: observed.number,
    url: typeof observed.url === 'string' ? observed.url : null,
    baseCommit,
    proposedRevision: headCommit,
  });
}

module.exports = { openProposal, PrAdapterError, PROJECTED_PREFIX };
