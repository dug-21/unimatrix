#!/usr/bin/env node
'use strict';

// Scheduled/triggered adoption proposal job wrapper (col-007 P1) — PROGRAM-OWNED glue.
//
// Unimatrix's own thin wrapper around the vendored collective receiver
// (delivery/receiver-run.js). It builds the single reviewed-target descriptor the receiver
// job needs PURELY from PUBLIC data Unimatrix already holds — its own
// `.collective-delivery/INSTALLATION.json` and the GitHub-provided repository context — and
// invokes the vendored receiver-run.js CLI, which is fail-closed: absent the accepted-mirror
// source it HOLDS (non-zero) and opens nothing. The job can only ever open a proposal PR on
// its own branch; it holds no merge, admin, or collective grant (enforced by
// permissions: contents: write + pull-requests: write and by the adapter's own
// no-admin-grant guard).
//
// This wrapper carries no collective-private material. The mirror repository identity and
// accepted ref come from the receiver's installed declaration; the mirror commit is resolved
// by a public `git ls-remote`; the release identity to propose is a repo-configured input
// (COLLECTIVE_TARGET_RELEASE). Full deployment host-adapter wiring for autonomous opening is
// completed in the collective delivery-trial step; until then this entry point is installed
// with least privilege and holds.

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const HERE = __dirname;
const INSTALLATION = path.join(HERE, 'INSTALLATION.json');
const RECEIVER_CLI = path.join(HERE, 'delivery', 'receiver-run.js');

function hold(message) {
  process.stdout.write(JSON.stringify({ outcome: 'held', condition: 'not-configured', message }) + '\n');
  process.exit(3);
}

function main() {
  let installation;
  try { installation = JSON.parse(fs.readFileSync(INSTALLATION, 'utf8')); }
  catch (e) { hold(`INSTALLATION.json unreadable: ${e.message}`); }

  const server = process.env.GITHUB_SERVER_URL || 'https://github.com';
  const repo = process.env.GITHUB_REPOSITORY;
  if (!repo) hold('GITHUB_REPOSITORY is not set');
  const receiverLocation = `${server}/${repo}`;

  const release = process.env.COLLECTIVE_TARGET_RELEASE;
  if (!release) hold('COLLECTIVE_TARGET_RELEASE (release identity to propose) is not configured');

  // Resolve the accepted mirror commit with a public read. No secret is used or required.
  const mirrorUrl = `${server}/${installation.mirror.repository_identity}`;
  const ls = spawnSync('git', ['ls-remote', mirrorUrl, installation.mirror.accepted_ref], { encoding: 'utf8' });
  if (ls.status !== 0 || !ls.stdout) hold(`could not read mirror ${installation.mirror.repository_identity} ${installation.mirror.accepted_ref}`);
  const mirrorCommit = ls.stdout.split(/\s+/)[0];
  if (!/^[0-9a-f]{40}$/.test(mirrorCommit || '')) hold('mirror accepted ref did not resolve to a commit');

  const target = {
    receiver_location: receiverLocation,
    canonical_branch: installation.canonical_branch,
    proposal_base: installation.proposal_base,
    accepted_ref: installation.mirror.accepted_ref,
    reviewed_revision: mirrorCommit,
  };
  const tmp = path.join(os.tmpdir(), `collective-target-${process.pid}.json`);
  fs.writeFileSync(tmp, JSON.stringify(target));

  const args = [
    RECEIVER_CLI,
    '--receiver', installation.program_identity,
    '--mirror-identity', installation.mirror.repository_identity,
    '--mirror-commit', mirrorCommit,
    '--release', release,
    '--target', tmp,
  ];
  const run = spawnSync(process.execPath, args, { stdio: 'inherit' });
  try { fs.unlinkSync(tmp); } catch { /* best effort */ }
  process.exit(run.status === null ? 1 : run.status);
}

main();
