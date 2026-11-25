"use strict";

// ca-fixture.js — vnc-050 C10 test helper: a local CA + leaf for localhost, a
// wrong-host leaf, and a standalone self-signed cert (openssl CLI, no deps),
// plus a raw TLS server recording every decrypted application byte.

const fs = require("fs");
const os = require("os");
const path = require("path");
const tls = require("tls");
const { execFileSync } = require("child_process");

function ossl(args, cwd) {
  execFileSync("openssl", args, { cwd, stdio: "ignore" });
}

function leaf(dir, name, cn, san) {
  ossl(["req", "-newkey", "rsa:2048", "-nodes", "-keyout", name + ".key", "-out", name + ".csr", "-subj", "/CN=" + cn], dir);
  fs.writeFileSync(path.join(dir, name + ".ext"), "subjectAltName=" + san + "\n");
  ossl(["x509", "-req", "-in", name + ".csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial",
    "-out", name + ".pem", "-days", "2", "-extfile", name + ".ext"], dir);
}

/** Returns null if openssl is unavailable. */
function makeCaFixture() {
  try {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "uni-ca-"));
    ossl(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.pem",
      "-days", "2", "-subj", "/CN=uni-test-ca"], dir);
    leaf(dir, "good", "localhost", "DNS:localhost,IP:127.0.0.1");
    leaf(dir, "wrong", "elsewhere.invalid", "DNS:elsewhere.invalid");
    ossl(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "self.key", "-out", "self.pem",
      "-days", "2", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"], dir);
    const rd = (f) => fs.readFileSync(path.join(dir, f));
    return {
      dir,
      caPath: path.join(dir, "ca.pem"),
      good: { cert: rd("good.pem"), key: rd("good.key") },
      wrong: { cert: rd("wrong.pem"), key: rd("wrong.key") },
      self: { cert: rd("self.pem"), key: rd("self.key") },
    };
  } catch (_e) {
    return null;
  }
}

/**
 * Raw TLS server recording all decrypted app bytes. `respond(buf, sock)` may
 * answer once a full request is seen; default answers HTTP 204 per request.
 */
function startByteCapture(pair, respond) {
  return new Promise((resolve) => {
    const cap = { bytes: Buffer.alloc(0), server: null, port: 0 };
    cap.server = tls.createServer(pair, (sock) => {
      sock.on("data", (d) => {
        cap.bytes = Buffer.concat([cap.bytes, d]);
        if (respond) respond(cap.bytes, sock);
        else if (cap.bytes.includes("\r\n\r\n")) sock.end("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
      });
      sock.on("error", () => {});
    });
    cap.server.on("tlsClientError", () => {});
    cap.server.listen(0, "127.0.0.1", () => {
      cap.port = cap.server.address().port;
      resolve(cap);
    });
  });
}

module.exports = { makeCaFixture, startByteCapture };
