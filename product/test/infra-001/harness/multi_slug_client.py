"""#800 multi-slug HTTPS fixture substrate (vnc-046 Stage 3c).

Boots a REAL `unimatrix serve --foreground` daemon with the HTTP transport
enabled and >=2 registered `[[projects]]` slugs on ONE instance, then drives
each slug's own `/v1/{slug}/observe` edge over HTTPS (bearer + provisioned leaf
cert) and reads the per-slug store back via sqlite. This is the true MCP/HTTP
wire surface a future rewire would break — the surface the in-process Rust
behavioral suite (`tests/project_routing_integration.rs`) cannot reach.

EXTENDS infra-001 (SR-08 — extend, never fork): it reuses the daemon boot /
data-dir discovery idiom from `harness/conftest.py::daemon_server` and the wire
recipe proven by `scripts/isolation-probe-lib.sh` (observe RecordEvent body,
bearer + `--cacert` leaf-cert pinning, the per-slug sqlite read). No new
transport/spawn/framing path; stdlib only (urllib + ssl + sqlite3) so it adds no
pip dependency.

Design notes:
  * SHORT temp HOME — the daemon binds a UDS socket even in HTTP mode; a deep
    scratchpad HOME overflows `SUN_LEN` (108), so HOME lives under /tmp.
  * The observe write is durable only EVENTUALLY (async observation writer), so
    the own-read positive control is a bounded read-as-barrier (retry-until-
    present), mirroring the infra-003 smoke; a cross-read is a synchronous
    absence check gated behind the own-read barrier (positive-gates-negative).
  * topic_signal markers carry the feature-id shape (an all-digit hyphen-segment
    + an alpha-bearing segment) so the observe path persists topic_signal
    (bugfix-832 `looks_like_feature_id`), and are mutually NON-SUBSTRING (#5347).
"""

from __future__ import annotations

import json
import os
import signal
import sqlite3
import ssl
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

# UDS SUN_LEN ceiling guard: keep the temp HOME short so the daemon's
# `{HOME}/.unimatrix/{hash16}/unimatrix-mcp.sock` path stays < 108 chars.
_TMP_ROOT = "/tmp"
_BOOT_DEADLINE_S = 120
_HTTP_PORT = int(os.environ.get("UNI_ISO_HTTP_PORT", "8443"))


class MultiSlugHttpServer:
    """A live multi-slug HTTPS daemon under test. Use via the
    `multi_slug_http_server` fixture; do not construct directly in tests."""

    def __init__(self, binary: str, slugs: list[str], port: int = _HTTP_PORT):
        self.binary = binary
        self.slugs = list(slugs)
        self.port = port
        self.base = f"https://localhost:{port}"
        self._home: Path | None = None
        self._proc: subprocess.Popen | None = None
        self._stderr: list[str] = []
        self.token: str = ""
        self.cert_path: str = ""
        # slug -> per-slug store db path
        self.store_db: dict[str, str] = {}

    # -- lifecycle -----------------------------------------------------------
    def start(self) -> "MultiSlugHttpServer":
        self._home = Path(tempfile.mkdtemp(prefix="ums", dir=_TMP_ROOT))
        proj = self._home / "proj"
        proj.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        env["HOME"] = str(self._home)

        # Register every slug BEFORE the single boot (routing read once at boot).
        for slug in self.slugs:
            r = subprocess.run(
                [self.binary, "--project-dir", str(proj), "project", "register", slug],
                env=env, capture_output=True, text=True,
            )
            if r.returncode != 0:
                self._cleanup()
                raise RuntimeError(f"project register {slug} failed: {r.stderr[-400:]}")

        env["UNIMATRIX_HTTP_ENABLED"] = "true"
        env["UNIMATRIX_PUBLIC_URL"] = self.base
        log_path = self._home / "daemon.log"
        self._logf = open(log_path, "w")
        self._proc = subprocess.Popen(
            [self.binary, "--project-dir", str(proj), "serve", "--foreground"],
            stdin=subprocess.DEVNULL, stdout=self._logf, stderr=self._logf, env=env,
        )
        self._wait_http_active(log_path)
        self._discover(env)
        return self

    def _wait_http_active(self, log_path: Path) -> None:
        deadline = time.monotonic() + _BOOT_DEADLINE_S
        while time.monotonic() < deadline:
            if self._proc.poll() is not None:
                self._cleanup()
                raise RuntimeError(f"daemon exited early:\n{self._tail(log_path)}")
            if "HTTP transport active" in log_path.read_text(errors="replace"):
                return
            time.sleep(1)
        tail = self._tail(log_path)
        self._cleanup()
        raise RuntimeError(f"HTTP transport never became active:\n{tail}")

    def _discover(self, env: dict) -> None:
        base = self._home / ".unimatrix"
        # The daemon's own data dir (token + tls/cert.pem) is the hash16 subdir
        # that carries a `token` file; per-slug stores are sibling `{slug}/` dirs.
        hash_dir = None
        for d in base.iterdir():
            if (d / "token").is_file():
                hash_dir = d
                break
        if hash_dir is None:
            raise RuntimeError("could not locate daemon data dir (no token file)")
        self.token = (hash_dir / "token").read_text().strip()
        self.cert_path = str(hash_dir / "tls" / "cert.pem")
        if not self.token or not Path(self.cert_path).is_file():
            raise RuntimeError("empty bearer token or missing leaf cert")
        for slug in self.slugs:
            db = base / slug / "unimatrix.db"
            if not db.is_file():
                raise RuntimeError(f"per-slug store missing for {slug}: {db}")
            self.store_db[slug] = str(db)

    def stop(self) -> None:
        self._cleanup()

    def _cleanup(self) -> None:
        if self._proc is not None and self._proc.poll() is None:
            try:
                self._proc.send_signal(signal.SIGTERM)
                self._proc.wait(timeout=15)
            except Exception:
                try:
                    self._proc.kill()
                    self._proc.wait(timeout=5)
                except Exception:
                    pass
        try:
            self._logf.close()
        except Exception:
            pass
        if self._home is not None:
            import shutil
            shutil.rmtree(self._home, ignore_errors=True)

    @staticmethod
    def _tail(log_path: Path, n: int = 12) -> str:
        try:
            return "\n".join(log_path.read_text(errors="replace").splitlines()[-n:])
        except Exception:
            return "<no log>"

    # -- wire surface --------------------------------------------------------
    def _ssl_ctx(self) -> ssl.SSLContext:
        ctx = ssl.create_default_context(cafile=self.cert_path)
        return ctx

    def observe_record(self, slug: str, session_id: str, topic_signal: str) -> int:
        """POST a RecordEvent to `/v1/{slug}/observe`. Returns the HTTP status.
        Body mirrors `isolation-probe-lib.sh::observe_write` (RecordEvent w/
        flattened ImplantEvent; the marker rides `topic_signal`)."""
        body = json.dumps({
            "type": "RecordEvent",
            "event_type": "tool_use",
            "session_id": session_id,
            "timestamp": 0,
            "payload": {},
            "topic_signal": topic_signal,
        }).encode()
        req = urllib.request.Request(
            f"{self.base}/v1/{slug}/observe", data=body, method="POST",
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(req, context=self._ssl_ctx(), timeout=30) as resp:
                return resp.status
        except urllib.error.HTTPError as e:
            return e.code

    def get_status(self, path: str) -> int:
        """GET an arbitrary path (bearer-authed); returns the HTTP status.
        Used for the unknown-slug 404 edge check."""
        req = urllib.request.Request(
            f"{self.base}{path}", method="GET",
            headers={"Authorization": f"Bearer {self.token}"},
        )
        try:
            with urllib.request.urlopen(req, context=self._ssl_ctx(), timeout=30) as resp:
                return resp.status
        except urllib.error.HTTPError as e:
            return e.code

    # -- per-slug store read (sqlite; the durable observable) ----------------
    def count_observations(self, slug: str, topic_signal: str) -> int:
        db = self.store_db[slug]
        tmp = f"{db}.rocopy.{os.getpid()}.db"
        try:
            # Copy main + WAL/SHM for a durable post-write view (WAL-robust read).
            import shutil
            shutil.copy(db, tmp)
            for ext in ("-wal", "-shm"):
                if Path(db + ext).exists():
                    shutil.copy(db + ext, tmp + ext)
            conn = sqlite3.connect(tmp)
            try:
                row = conn.execute(
                    "SELECT count(*) FROM observations WHERE topic_signal = ?",
                    (topic_signal,),
                ).fetchone()
                return int(row[0]) if row else 0
            finally:
                conn.close()
        finally:
            for ext in ("", "-wal", "-shm"):
                try:
                    os.remove(tmp + ext)
                except OSError:
                    pass

    def wait_observation(self, slug: str, topic_signal: str, deadline_s: float = 15.0) -> int:
        """Read-as-barrier positive control: poll the slug's own store until its
        marker appears, bounded. Returns the count once present (>=1) or the
        final count (0) on timeout — a 0 at the call site is a hard fidelity
        failure, never a silent pass."""
        end = time.monotonic() + deadline_s
        while time.monotonic() < end:
            n = self.count_observations(slug, topic_signal)
            if n >= 1:
                return n
            time.sleep(0.5)
        return self.count_observations(slug, topic_signal)
