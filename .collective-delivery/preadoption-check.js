#!/usr/bin/env node
'use strict';

// Required pre-adoption check wrapper (col-007 P2) — PROGRAM-OWNED glue.
//
// This is Unimatrix's own thin wrapper around the vendored collective verifier
// (delivery/verify-proposal.js). It is NOT collective mechanism code and carries no
// collective-private material. It:
//
//   1. Extracts the review-binding block from the adoption PR body using the SAME parser
//      the collective ships (delivery/receiver-run.js#parseReviewBinding), yielding the
//      release identity, mirror repository identity and mirror commit under review.
//   2. Reconstructs the two-receiver delivery-targets doc that verify-proposal requires,
//      PURELY from PUBLIC data Unimatrix already holds: its adopted
//      `.collective/contract/REGISTRATIONS.yaml` (both opaque identities + locations) and
//      its own `.collective-delivery/INSTALLATION.json` (mirror + branches). The private
//      collective trial-target map is never present in this repository.
//   3. Invokes the vendored verify-proposal.js CLI, which builds its own host readers from
//      the COLLECTIVE_MIRROR_* / COLLECTIVE_RECEIVER_* environment (repo vars/secrets) and
//      exits non-zero on any held/refused outcome — blocking the merge (fail-closed).
//
// A `.collective/**` PR with no valid binding, or with unreadable inputs, refuses (non-zero).
// This wrapper cannot merge, approve, or create any collective grant.

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const YAML = require('yaml');

const { parseReviewBinding } = require('./delivery/receiver-run.js');

const HERE = __dirname;
const INSTALLATION = path.join(HERE, 'INSTALLATION.json');
const REGISTRATIONS = path.resolve(HERE, '..', '.collective', 'contract', 'REGISTRATIONS.yaml');
const VERIFY_CLI = path.join(HERE, 'delivery', 'verify-proposal.js');

function fail(message) { process.stderr.write(`pre-adoption check refused: ${message}\n`); process.exit(1); }

function main() {
  const prBody = process.env.COLLECTIVE_PR_BODY;
  const proposedRevision = process.env.COLLECTIVE_PR_HEAD_SHA;
  if (typeof prBody !== 'string' || prBody.length === 0) fail('COLLECTIVE_PR_BODY is empty');
  if (typeof proposedRevision !== 'string' || proposedRevision.length === 0) fail('COLLECTIVE_PR_HEAD_SHA is empty');

  const binding = parseReviewBinding(prBody);
  if (!binding) fail('PR body carries no single valid collective-delivery review-binding block');

  let installation;
  try { installation = JSON.parse(fs.readFileSync(INSTALLATION, 'utf8')); }
  catch (e) { fail(`INSTALLATION.json unreadable: ${e.message}`); }

  if (binding.program_identity !== installation.program_identity)
    fail('review-binding program identity differs from the installed declaration');
  if (binding.mirror_repository_identity !== installation.mirror.repository_identity)
    fail('review-binding mirror identity differs from the installed declaration');

  let registrations;
  try { registrations = YAML.parse(fs.readFileSync(REGISTRATIONS, 'utf8')); }
  catch (e) { fail(`adopted REGISTRATIONS.yaml unreadable: ${e.message}`); }
  const programs = (registrations && registrations.programs) || [];
  if (programs.length !== 2) fail('adopted REGISTRATIONS.yaml does not list exactly two Programs');

  // Reconstruct the two-receiver target map from PUBLIC registration + own installation.
  // The receiver's own entry mirrors its installation branch bindings; the other entry
  // satisfies the closed schema from its public registration. reviewed_revision is a local
  // placeholder — P2 checks the receiver's own entry against its installation, not the
  // collective's private reviewing revision (that is R3/E1's collective-side check).
  const targets = {
    schema: 'collective.delivery-targets/1',
    reviewed_revision: '0000000000000000000000000000000000000000',
    mirror: {
      repository_identity: installation.mirror.repository_identity,
      accepted_ref: installation.mirror.accepted_ref,
    },
    targets: programs.map((p) => ({
      program_identity: p.program_identity,
      receiver_location: p.location,
      canonical_branch: p.program_identity === installation.program_identity ? installation.canonical_branch : 'main',
      proposal_base: p.program_identity === installation.program_identity ? installation.proposal_base : 'main',
      bootstrap: p.program_identity === installation.program_identity ? 'merged' : 'pending',
    })),
  };

  const tmp = path.join(os.tmpdir(), `collective-targets-${process.pid}.json`);
  fs.writeFileSync(tmp, JSON.stringify(targets));

  const args = [
    VERIFY_CLI,
    '--mirror-identity', binding.mirror_repository_identity,
    '--mirror-commit', binding.mirror_commit,
    '--release', binding.release_identity,
    '--receiver', installation.program_identity,
    '--proposed-revision', proposedRevision,
    '--installation', INSTALLATION,
    '--targets', tmp,
  ];
  const run = spawnSync(process.execPath, args, { stdio: 'inherit' });
  try { fs.unlinkSync(tmp); } catch { /* best effort */ }
  process.exit(run.status === null ? 1 : run.status);
}

main();
