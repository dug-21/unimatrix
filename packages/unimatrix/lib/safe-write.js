"use strict";
// The one hardened writer API (ADR-003). The low-level core lives in trust.js
// (ADR-005) so the installed self-contained trust.js copy shares it.
const fs = require("node:fs");
const core = require("./trust.js");

const { SafeWriteError, sha256 } = core;
const noop = () => {};

function chmodOrThrow(target, mode) {
  try {
    fs.chmodSync(target, mode);
  } catch (e) {
    throw new SafeWriteError("io", target, `${target}: ${e && e.message}`);
  }
}

// Fixpoint verify (FR-29): same target, link still a link, bytes as written,
// and the mutate now declines to change anything.
function verifyOk(file, r, written, bufMutate) {
  const now = core.resolveTarget(file);
  if (now.target !== r.target) return false;
  if (r.isLink && !fs.lstatSync(file).isSymbolicLink()) return false;
  let back;
  try {
    back = fs.readFileSync(now.target);
  } catch (_) {
    return false;
  }
  if (sha256(back) !== sha256(written)) return false;
  const again = bufMutate(back);
  return again === null || again.equals(back);
}

// Shared loop over Buffers. bufMutate(Buffer|null) -> Buffer|null (null = no change).
function updateBuffer(file, bufMutate, opts) {
  const o = Object.assign({ dryRun: false, newMode: 0o600, verify: true, attempts: 3, platform: process.platform, fault: noop }, opts);
  let lastCode = "conflict";
  for (let attempt = 1; attempt <= o.attempts; attempt++) {
    const r = core.resolveTarget(file);
    core.checkContained(file, r.target, o.containWithin, o.platform);
    const st = core.readState(r.target);
    const raw = st.bytes;
    o.fault("after-read");
    const next = bufMutate(raw);
    const res = (action) => ({ action, file, target: r.target, before: raw });
    if (next === null || (raw !== null && next.equals(raw))) {
      if (raw === null) return res("unchanged");
      if (o.enforceMode !== undefined && st.mode !== o.enforceMode && o.platform !== "win32") {
        if (!o.dryRun) chmodOrThrow(r.target, o.enforceMode);
        return res("updated");
      }
      return res("unchanged");
    }
    const action = raw === null ? "created" : "updated";
    if (o.dryRun) return res(action);
    const wantMode = o.enforceMode !== undefined ? o.enforceMode : st.mode !== null ? st.mode : o.newMode;
    const hash = o.expectHash !== undefined ? o.expectHash : st.hash;
    try {
      core.hardenedWriteSync(file, next, { expect: { target: r.target, hash }, mode: wantMode, platform: o.platform, retry: o.retry, fault: o.fault, containWithin: o.containWithin });
    } catch (e) {
      if (e && e.code === "conflict" && o.expectHash === undefined) {
        lastCode = "conflict";
        continue;
      }
      throw e;
    }
    o.fault("after-rename");
    if (o.verify && !verifyOk(file, r, next, bufMutate)) {
      lastCode = "verify-failed";
      continue;
    }
    return res(action);
  }
  const why = lastCode === "conflict" ? "changed concurrently" : "could not verify write";
  throw new SafeWriteError(lastCode, file, `${file}: ${why} after ${o.attempts} attempts; re-run 'npx unimatrix init'`);
}

/**
 * updateFileSync(file, mutate, opts) -> {action, file, target, before}
 *   mutate(raw: string|null) -> string|null (null = leave as is). Errors thrown by
 *   mutate propagate unchanged. `before` is the raw the successful decision used.
 */
function updateFileSync(file, mutate, opts) {
  const bufMutate = (b) => {
    const out = mutate(b === null ? null : b.toString("utf8"));
    return out === null || out === undefined ? null : Buffer.from(String(out), "utf8");
  };
  const res = updateBuffer(file, bufMutate, opts);
  res.before = res.before === null ? null : res.before.toString("utf8");
  return res;
}

/**
 * writeFileSync(file, data, opts) -> {action, file, target, before}
 *   Whole-file replace (string or Buffer); equal bytes short-circuit to unchanged.
 *   opts.expectHash: CAS against that hash ("absent" for a new file), no retry.
 */
function writeFileSync(file, data, opts) {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(String(data), "utf8");
  const res = updateBuffer(file, (raw) => (raw !== null && raw.equals(bytes) ? null : bytes), opts);
  res.before = res.before === null ? null : res.before.toString("utf8");
  return res;
}

module.exports = { updateFileSync, writeFileSync, SafeWriteError, tempPath: core.tempPath, renameWithRetry: core.renameWithRetry };
