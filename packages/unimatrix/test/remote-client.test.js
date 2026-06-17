"use strict";

// remote-client.test.js — vnc-034 Wave-1 RemoteClient (#725). Covers the C1
// bundle decoder (bundle.js), the C2 cert pin (cert-pin.js), and the
// `init --remote <bundle> [--slug s]` flow (init.js initRemote bundle path).
//
// PARITY: the fingerprint + bundle expectations are READ FROM the committed Rust
// oracle corpus (crates/unimatrix-server/tests/fixtures/c1c2-parity/). The JS
// golden is NEVER hand-written (SR-02 / ADR-002 / ADR-006). Cumulative infra —
// extends the existing node:test suites.

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

// NOTE (vnc-038 / ADR-002 / ADR-001): the v:2 bundle decoder parity, guard-
// ordering, strict-schema, and v:1-hard-cut suites now live in the focused
// test/hook-client/bundle.test.js. assertSlugAllowlist / SLUG_RE were RETIRED
// from bundle.js (the client no longer derives a slug). The bundle-decode and
// slug-allowlist describe blocks below were removed here to avoid duplication.
const { BundleError, MAX_RAW_LEN } = require("../lib/hook-client/bundle.js");
const {
  computeFingerprint,
  makeCheckServerIdentity,
  verifyPeerFingerprint,
  applyCertPin,
} = require("../lib/hook-client/cert-pin.js");

const initModule = require("../lib/init.js");
const { initRemote, resolveRemoteTarget } = initModule;
const transport = require("../lib/hook-client/transport-http.js");

// ----------------------------------------------------------------------------
// Committed Rust-oracle parity corpus (NEVER hand-write these values).
// ----------------------------------------------------------------------------
const CORPUS_DIR = path.join(
  __dirname,
  "..",
  "..",
  "..",
  "crates",
  "unimatrix-server",
  "tests",
  "fixtures",
  "c1c2-parity"
);
const FINGERPRINT_GOLDEN = JSON.parse(
  fs.readFileSync(path.join(CORPUS_DIR, "fingerprint-golden.json"), "utf8")
);
const BUNDLE_GOLDEN = JSON.parse(
  fs.readFileSync(path.join(CORPUS_DIR, "bundle-golden.json"), "utf8")
);

// ----------------------------------------------------------------------------
// Test helpers (cumulative — mirror init-remote.test.js conventions).
// ----------------------------------------------------------------------------
function makeTempProject() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-vnc034-test-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  return dir;
}

function readSettingsLocal(projectRoot) {
  const fp = path.join(projectRoot, ".claude", "settings.local.json");
  return JSON.parse(fs.readFileSync(fp, "utf8"));
}

// Stub pingForInit; capture the args it was called with so the pinned-fp thread
// is observable. Restore in afterEach.
let origPing;
let lastPingArgs;
function stubPing(fn) {
  origPing = transport.pingForInit;
  lastPingArgs = null;
  transport.pingForInit = function (...args) {
    lastPingArgs = args;
    return fn(...args);
  };
}
function okPing() {
  return Promise.resolve({ ok: true, message: "Pong from host (server x)" });
}

// A valid bundle string from the committed corpus (first entry).
const GOLDEN_BUNDLE = BUNDLE_GOLDEN[0];
const VALID_WIRE = GOLDEN_BUNDLE.wire;

// ============================================================================
// C2 — cert pinning (R-02 / AC-W1-C2)
// ============================================================================
describe("cert pin — computeFingerprint parity (R-02 / SR-02)", () => {
  it("test_checkserveridentity_computes_pin_over_cert_raw — matches Rust oracle for every golden DER", () => {
    assert.ok(FINGERPRINT_GOLDEN.length > 0, "corpus must be non-empty");
    for (const entry of FINGERPRINT_GOLDEN) {
      const der = Buffer.from(entry.der_hex, "hex");
      const got = computeFingerprint(der);
      assert.strictEqual(
        got,
        entry.fp,
        "fingerprint parity for der_hex=" + entry.der_hex.slice(0, 16) + "..."
      );
      // Format contract: sha256:<64 lowercase hex>.
      assert.match(got, /^sha256:[0-9a-f]{64}$/);
    }
  });
});

describe("cert pin — checkServerIdentity (R-02 / AC-W1-C2 / AC-CT-ROT)", () => {
  // Reuse a corpus DER as the "served leaf" so the pinned value is oracle-derived.
  const LEAF = FINGERPRINT_GOLDEN[2];
  const leafDer = Buffer.from(LEAF.der_hex, "hex");
  const pinnedFp = LEAF.fp;

  it("test_pin_match_accepts — matching cert → undefined (accept)", () => {
    const check = makeCheckServerIdentity(pinnedFp);
    const result = check("host.example", { raw: leafDer });
    assert.strictEqual(result, undefined);
  });

  it("test_pin_mismatch_rejects_with_diagnosable_error — names expected vs presented + remediation", () => {
    const wrongDer = Buffer.from(FINGERPRINT_GOLDEN[3].der_hex, "hex");
    const presented = FINGERPRINT_GOLDEN[3].fp;
    const check = makeCheckServerIdentity(pinnedFp);
    const result = check("host.example", { raw: wrongDer });
    assert.ok(result instanceof Error, "mismatch must return an Error");
    // Diagnosable contract (AC-CT-ROT): BOTH fingerprints + remediation present.
    assert.ok(result.message.includes(pinnedFp), "names expected (pinned) fp");
    assert.ok(result.message.includes(presented), "names presented (server) fp");
    assert.ok(
      result.message.includes("client-bundle") &&
        result.message.includes("init --remote"),
      "points at re-bundle remediation"
    );
    // NOT a bare opaque TLS handshake error.
    assert.ok(
      /mismatch/i.test(result.message),
      "is the explicit pin-mismatch message"
    );
  });

  it("test_no_cert_presented → diagnosable error, not a crash", () => {
    const check = makeCheckServerIdentity(pinnedFp);
    assert.ok(check("h", null) instanceof Error);
    assert.ok(check("h", {}) instanceof Error);
  });

  it("test_pin_completes_self_signed_handshake — applyCertPin sets rejectUnauthorized:false, clears CA trust (F1)", () => {
    // F1 fix: the self-signed handshake must COMPLETE (rejectUnauthorized:false)
    // so the manual secureConnect fingerprint check can run; rejectUnauthorized
    // true + ca:undefined rejected the legitimate leaf before the pin ran.
    const opts = {};
    applyCertPin(opts, true, pinnedFp);
    assert.strictEqual(opts.rejectUnauthorized, false, "self-signed handshake must complete");
    assert.strictEqual(opts.ca, undefined, "no CA trust path is supplied");
    // Non-TLS / unpinned → no-op (no override on plain http).
    const plain = {};
    applyCertPin(plain, false, pinnedFp);
    assert.strictEqual(plain.rejectUnauthorized, undefined);
    const unpinned = {};
    applyCertPin(unpinned, true, null);
    assert.strictEqual(unpinned.rejectUnauthorized, undefined);
  });

  it("test_verify_peer_fingerprint_match — null on a matching live peer cert", () => {
    const fakeSocket = { getPeerCertificate: () => ({ raw: leafDer }) };
    assert.strictEqual(verifyPeerFingerprint(fakeSocket, pinnedFp), null);
  });

  it("test_verify_peer_fingerprint_mismatch — diagnosable Error naming both fps", () => {
    const wrongDer = Buffer.from(FINGERPRINT_GOLDEN[3].der_hex, "hex");
    const presented = FINGERPRINT_GOLDEN[3].fp;
    const fakeSocket = { getPeerCertificate: () => ({ raw: wrongDer }) };
    const err = verifyPeerFingerprint(fakeSocket, pinnedFp);
    assert.ok(err instanceof Error, "mismatch must return an Error");
    assert.ok(err.message.includes(pinnedFp), "names expected (pinned) fp");
    assert.ok(err.message.includes(presented), "names presented (server) fp");
    assert.ok(
      err.message.includes("client-bundle") && err.message.includes("init --remote"),
      "points at re-bundle remediation"
    );
  });

  it("test_verify_peer_fingerprint_no_cert — Error, not a crash", () => {
    assert.ok(verifyPeerFingerprint({ getPeerCertificate: () => ({}) }, pinnedFp) instanceof Error);
    assert.ok(verifyPeerFingerprint({ getPeerCertificate: () => null }, pinnedFp) instanceof Error);
    assert.ok(verifyPeerFingerprint({}, pinnedFp) instanceof Error);
  });
});

// ============================================================================
// C1 — bundle decode (v:2) + slug allowlist
// ----------------------------------------------------------------------------
// MOVED (vnc-038 / ADR-002 / ADR-001): the bundle-decode parity, guard-ordering,
// strict-schema, and v:1-hard-cut suites now live in test/hook-client/bundle.test.js
// (the v:2 decoder is byte-parity against the Rust-generated corpus). The slug-
// allowlist suite was RETIRED — assertSlugAllowlist/SLUG_RE no longer exist (the
// client derives no slug; the server composes both URLs into the v:2 bundle).
// ============================================================================

// ============================================================================
// initRemote bundle path (R-05 / R-06 / AC-W1-C5 / C6)
// ============================================================================
describe("initRemote — bundle path verbatim store (R-01 / ADR-001)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_mcp_url_stored_verbatim — settings.local.json mcp_url == bundle.mcp_url byte-for-byte", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const sl = readSettingsLocal(dir);
    // ADR-001 dumb-client: no append, no slug derivation, no normalization.
    assert.strictEqual(sl.unimatrix.remote.mcp_url, GOLDEN_BUNDLE.fields.mcp_url);
    assert.strictEqual(sl.unimatrix.remote.token, GOLDEN_BUNDLE.fields.token);
    assert.strictEqual(sl.unimatrix.remote.fingerprint, GOLDEN_BUNDLE.fields.fp);
  });

  it("test_observe_url_stored_verbatim — settings.local.json observe_url == bundle.observe_url byte-for-byte", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(
      sl.unimatrix.remote.observe_url,
      GOLDEN_BUNDLE.fields.observe_url
    );
    // The retired single `url` key is gone; the v:2 subtree carries two URLs.
    assert.strictEqual(sl.unimatrix.remote.url, undefined);
  });

  it("test_slug_flag_ignored — --slug is retired; the bundle URLs already encode the slug", async () => {
    const dir = makeTempProject();
    // Passing a (now-meaningless) --slug must NOT alter the verbatim URLs and
    // must NOT throw: the client derives no slug (ADR-001).
    await initRemote({ bundle: VALID_WIRE, slug: "ignored", projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(sl.unimatrix.remote.mcp_url, GOLDEN_BUNDLE.fields.mcp_url);
    assert.strictEqual(
      sl.unimatrix.remote.observe_url,
      GOLDEN_BUNDLE.fields.observe_url
    );
  });

  it("test_pinned_fp_threaded_into_ping — Ping runs over the pinned connection", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    // pingForInit(url, token, timeouts, pinnedFp)
    assert.strictEqual(lastPingArgs[3], GOLDEN_BUNDLE.fields.fp);
  });

  it("test_init_pings_observe_url_verbatim — Ping target is bundle.observe_url exactly", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    // AC-07 / #766: the init Ping posts to the server-composed observe URL
    // verbatim (not a client re-derived /observe append).
    assert.strictEqual(lastPingArgs[0], GOLDEN_BUNDLE.fields.observe_url);
  });
});

describe("initRemote — 1:1 unrepresentable (R-06 / AC-W1-C5)", () => {
  it("test_client_has_no_second_project_field — config bakes exactly one project's URLs", () => {
    // resolveRemoteTarget yields a flat {mcpUrl, observeUrl, token, pinnedFp}:
    // there is no array/list/second-endpoint field by which a second project can
    // be named. Both URLs are the server-composed verbatim fields.
    const t = resolveRemoteTarget({ bundle: VALID_WIRE });
    assert.deepStrictEqual(
      Object.keys(t).sort(),
      ["mcpUrl", "observeUrl", "pinnedFp", "token"]
    );
    assert.strictEqual(t.mcpUrl, GOLDEN_BUNDLE.fields.mcp_url);
    assert.strictEqual(t.observeUrl, GOLDEN_BUNDLE.fields.observe_url);
    // ADR-001: the URLs are verbatim bundle fields — the client composed nothing.
    assert.strictEqual(t.mcpUrl, GOLDEN_BUNDLE.fields.mcp_url);
  });
});

// Closed-set / empty-compose invariant (R-01 — load-bearing for SR-01): after
// the bundle path, init.js contains NO client-side URL composition — no slug
// append, no "/v1" append, no assertSlugAllowlist. (NFR-01.)
describe("init.js — empty-compose invariant (R-01 / NFR-01)", () => {
  const SRC = fs.readFileSync(
    path.join(__dirname, "..", "lib", "init.js"),
    "utf8"
  );

  it("test_no_slug_append_in_init — no '/v1/' + slug composition", () => {
    assert.ok(!/\+\s*options\.slug/.test(SRC), "no slug concatenation");
    assert.ok(!/"\/v1\/"\s*\+/.test(SRC), "no '/v1/' + slug append");
  });

  it("test_no_v1_default_append_in_init — no base + '/v1' default-alias append", () => {
    assert.ok(!/\+\s*"\/v1"/.test(SRC), "no '/v1' default append");
  });

  it("test_assert_slug_allowlist_import_removed — bundle.js slug helper is not imported", () => {
    assert.ok(
      !/assertSlugAllowlist/.test(SRC),
      "assertSlugAllowlist is retired and must not be imported"
    );
  });
});

describe("initRemote — malformed bundle reaches init (R-05 / AC-W1-C9-C10)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_init_remote_rejects_malformed_bundle — no config written, process survives", async () => {
    const dir = makeTempProject();
    await assert.rejects(
      () => initRemote({ bundle: "unimatrix-bundle:!!!notbase64!!!", projectDir: dir }),
      BundleError
    );
    assert.ok(!fs.existsSync(path.join(dir, ".claude", "settings.local.json")));
  });

  it("test_init_remote_length_cap_before_decode — over-cap arg rejects on length", async () => {
    const dir = makeTempProject();
    const overCap = "@".repeat(MAX_RAW_LEN + 50);
    await assert.rejects(
      () => initRemote({ bundle: overCap, projectDir: dir }),
      (err) => err instanceof BundleError && /too long/.test(err.message)
    );
  });
});

// ============================================================================
// Onboarding artifacts (AC-W1-C6) + secret hygiene (R-12 / NFR-06)
// ============================================================================
describe("initRemote — onboarding artifacts (AC-W1-C6)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_skills_copied — .claude/skills present after init", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const skillsDir = path.join(dir, ".claude", "skills");
    // Skills are copied when the package ships a skills/ dir; assert the copy
    // path ran (dir exists) OR the package has no bundled skills (documented skip).
    const sourceDir = path.join(__dirname, "..", "skills");
    if (fs.existsSync(sourceDir)) {
      assert.ok(fs.existsSync(skillsDir), "skills copied into project");
    }
  });

  it("test_claudemd_block_not_appended — init does not write CLAUDE.md", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    assert.ok(
      !fs.existsSync(path.join(dir, "CLAUDE.md")),
      "init must not append a CLAUDE.md knowledge block (uni-init owns it)"
    );
  });

  it("test_unimatrix_init_pointer_printed — stdout names /unimatrix-init", async () => {
    const dir = makeTempProject();
    const lines = [];
    const origLog = console.log;
    console.log = (...a) => lines.push(a.join(" "));
    try {
      await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    } finally {
      console.log = origLog;
    }
    assert.ok(
      lines.some((l) => l.includes("/unimatrix-init")),
      "the /unimatrix-init pointer is printed"
    );
  });
});

describe("initRemote — token hygiene (R-12 / NFR-06)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_token_not_logged_by_client — token never on stdout/stderr", async () => {
    const dir = makeTempProject();
    const tok = GOLDEN_BUNDLE.fields.token;
    const out = [];
    const origLog = console.log;
    const origErrWrite = process.stderr.write.bind(process.stderr);
    console.log = (...a) => out.push(a.join(" "));
    process.stderr.write = (s) => {
      out.push(String(s));
      return true;
    };
    try {
      await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    } finally {
      console.log = origLog;
      process.stderr.write = origErrWrite;
    }
    const all = out.join("\n");
    assert.ok(!all.includes(tok), "token must not appear in any console output");
    // But it MUST land in the persisted config.
    assert.strictEqual(readSettingsLocal(dir).unimatrix.remote.token, tok);
  });

  it("test_settings_local_mode_0600", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: VALID_WIRE, projectDir: dir });
    const slPath = path.join(dir, ".claude", "settings.local.json");
    const mode = fs.statSync(slPath).mode & 0o777;
    if (process.platform !== "win32") {
      assert.strictEqual(mode, 0o600, "token-bearing file is 0600");
    }
  });
});

// ============================================================================
// Install footprint < 250 KB (AC-W1-C3 / NFR-01 / R-12) — HARD GATE
// ============================================================================
describe("remote install footprint (AC-W1-C3 / NFR-01)", () => {
  const SIZE_LIMIT = 250 * 1024; // 250 KB

  // The shipped remote install is the pure-JS client (lib/) + skills/, with NO
  // native binary and NO model. Walk both trees and sum file bytes.
  function dirBytes(root) {
    let total = 0;
    const stack = [root];
    while (stack.length) {
      const cur = stack.pop();
      let entries;
      try {
        entries = fs.readdirSync(cur, { withFileTypes: true });
      } catch (_err) {
        continue;
      }
      for (const e of entries) {
        const p = path.join(cur, e.name);
        if (e.isDirectory()) {
          stack.push(p);
        } else if (e.isFile()) {
          total += fs.statSync(p).size;
        }
      }
    }
    return total;
  }

  it("test_remote_install_under_250kb — lib/ + skills/ shipped footprint", () => {
    const pkgRoot = path.join(__dirname, "..");
    const libBytes = dirBytes(path.join(pkgRoot, "lib"));
    const skillsBytes = dirBytes(path.join(pkgRoot, "skills"));
    const total = libBytes + skillsBytes;
    assert.ok(
      total < SIZE_LIMIT,
      "remote install footprint " +
        total +
        " bytes must be < " +
        SIZE_LIMIT +
        " (lib=" +
        libBytes +
        ", skills=" +
        skillsBytes +
        ")"
    );
  });
});

// ============================================================================
// Rotation overwrite (edge — re-init with a new bundle overwrites the pin)
// ============================================================================
describe("initRemote — rotation overwrite (edge)", () => {
  beforeEach(() => stubPing(okPing));
  afterEach(() => {
    transport.pingForInit = origPing;
  });

  it("test_reinit_overwrites_pinned_fp_cleanly", async () => {
    const dir = makeTempProject();
    await initRemote({ bundle: BUNDLE_GOLDEN[0].wire, projectDir: dir });
    assert.strictEqual(
      readSettingsLocal(dir).unimatrix.remote.fingerprint,
      BUNDLE_GOLDEN[0].fields.fp
    );
    await initRemote({ bundle: BUNDLE_GOLDEN[1].wire, projectDir: dir });
    const sl = readSettingsLocal(dir);
    assert.strictEqual(sl.unimatrix.remote.fingerprint, BUNDLE_GOLDEN[1].fields.fp);
    assert.strictEqual(sl.unimatrix.remote.mcp_url, BUNDLE_GOLDEN[1].fields.mcp_url);
    assert.strictEqual(
      sl.unimatrix.remote.observe_url,
      BUNDLE_GOLDEN[1].fields.observe_url
    );
  });
});
