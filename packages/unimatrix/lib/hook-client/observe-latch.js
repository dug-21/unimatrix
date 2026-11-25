"use strict";
// vnc-050 C6c / ADR-004 (HD 10): per-credential observe-denied marker latch.
// Marker keyed on SHA-256 of the credential (secret never stored), 0600 via
// state.atomicWrite, no expiry. Fail open: missing/corrupt/unreadable marker =
// not denied; write failure ignored (atomicWrite never throws).
const fs = require("fs");
const state = require("./state");
let denied = false;
const file = (c) => c.stateDir + "/observe-denied.json";
const cred = (c) => require("crypto").createHash("sha256").update(String(c.token)).digest("hex");
function isDenied(c) {
  try {
    denied = denied || (!!c.stateDir && JSON.parse(fs.readFileSync(file(c))).cred === cred(c));
  } catch (_e) {}
  return denied;
}
function latch(c) {
  if (!denied) process.stderr.write("unimatrix: auth: observe denied (read-only token); events not recorded\n");
  denied = true;
  state.ensureStateDir(c.stateDir);
  state.atomicWrite(file(c), JSON.stringify({ cred: cred(c) }));
}
/** Wrap transport.post: no POST once latched; latch on SessionWrite 403. */
const guard = (c, t) => ({
  post: (cf, f, o) =>
    denied ? Promise.resolve({ ok: false, status: 403, failureClass: "auth", deny: 1 }) : t.post(cf, f, o).then((r) => (r && r.deny && latch(c), r)),
});
module.exports = { isDenied, latch, guard };
