# Unimatrix (Alpha release)

Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic workflows such as software delivery. It makes knowledge curation a first-class activity in the workflow itself — not a side effect. Agents search, store, and correct knowledge entries as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Unimatrix makes that knowledge trustworthy, consistent, and — as it learns from actual usage — continuously more relevant.

Two surfaces, both driven by the same engine: agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively — phase-conditioned injections and briefings that surface what matters before agents need to ask. The combination of explicit curation and self-improving delivery is what makes it distinct.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, the cycle(feature) goal, the agent task, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions(ADR's), patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, and when needed, corrected, and only current information is made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.

---

## Getting Started

### Install via npm

> **Platform: Linux x64 and arm64 only.** macOS and Windows are not supported via npm.
> **Hook providers: Claude Code (full), Gemini CLI (v0.31+), and Codex CLI (config ready; live MCP hook testing blocked by Codex upstream bug #16732)**

**Prerequisites — both required before installing:**
- Node.js >= 18
- ONNX Runtime 1.20.x shared library installed on the system

**Install ONNX Runtime (Linux):**
```bash
# Download the release for your architecture (x64 or aarch64) from:
# https://github.com/microsoft/onnxruntime/releases
# Extract and install the shared library:
tar xzf onnxruntime-linux-*.tgz
sudo cp onnxruntime-linux-*/lib/libonnxruntime.so* /usr/lib/
sudo ldconfig
```

```bash
npm install @dug-21/unimatrix
```

The embedding model downloads automatically on first run (or via `npx unimatrix model-download`).

### Container Deployment

> **Platform: Linux x64 and arm64.** Multi-arch images published to GHCR on each release.

```bash
# One-command deployment with persistent data volume
docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix

# Or via docker compose
docker compose up
```

The container runs `unimatrix serve --foreground` as PID 1 (non-root, UID 65532). The ONNX models (embedding + NLI) are **not** baked into the image — they download from HuggingFace on first boot into the `unimatrix-shared` volume (~166 MB), so the first start needs internet access and a **writable** `/shared` (the default; see compose). Subsequent boots reuse the populated volume offline. For an **air-gapped** deployment, pre-populate the volume first — e.g. `docker run --rm -v unimatrix-shared:/shared ghcr.io/dug-21/unimatrix --project-dir /data model-download` (or pre-seed it from a host that already has the models) — and *then* you may mount `/shared` read-only (`:ro`) to harden against tampering; mounting `:ro` on a virgin volume makes first boot fail. A host bind-mount for `/shared` must be writable by UID 65532 or first-boot download fails loud. Data persists in the `unimatrix-data` named volume at `/data`. Config lives in the data volume; customize via `unimatrix config` or set `UNIMATRIX_CONFIG` for external override.

**HTTPS serving (personal cloud).** To serve the container as a reachable, operator-run cloud over pinned TLS, set two environment variables in `compose.yaml`: `UNIMATRIX_HTTP_ENABLED=true` activates the HTTPS listener (the global binary default `http.enabled` stays `false`), and `UNIMATRIX_PUBLIC_URL` declares the URL clients connect to (e.g. `https://uni.example.com:8443`) — the single knob from which the server-composed bundle endpoint URLs, the `allowed_hosts` default, and the certificate SAN all derive. The published port is **TLS-only port 8443** — no plaintext port is exposed. On first boot the binary auto-generates both a 32-byte bearer token and a self-signed cert+key (key mode `0600`), persisting them to the data volume; subsequent boots load (not regenerate) them, and an operator may mount their own cert/key read-only to override. The bearer token is never printed to stdout or logs — the remote client obtains it solely through the connection bundle. The only unauthenticated endpoint on the published port is `GET /health`. Host bind-mounted `/data` must be writable by UID 65532 (`chown 65532` the host directory; named volumes need no setup) — the binary fails loud and actionable if it is not. A fresh container serves nothing until a project is registered: run `unimatrix project register <slug>` and restart, then run `unimatrix client-bundle <slug>` to emit the per-project connection bundle for clients (see "Serving projects" below).

**Serving projects.** Project identity is mandatory at the cloud/container entrypoint: a fresh deployment serves NO request until at least one project is registered — any attach against an unregistered server fails loud with "register a project to begin." There is no no-slug / default-project route. The first project and the Nth project are registered the IDENTICAL way — a single `unimatrix project register <slug>` command that creates the per-slug store, writes the `[[projects]]` routing intent (no hand-edit of `config.toml`), AND seeds an annotated per-slug `config.toml` the operator can edit (see "Per-Slug Configuration Overlay" below); a daemon restart applies it. Each registered slug is then reachable at `https://host:8443/v1/{slug}/...` with its own database, vector index, hash chain, and analytics under `/data/.unimatrix/{slug}/` — no cross-project read or write. One container serves N fully-isolated projects from a single bearer token, with a per-project bundle emitted by `unimatrix client-bundle <slug>`. Slugs are operator-declared, never client-minted: a client attaches to an existing slug and never auto-creates a project. A slug must match `^[a-z0-9][a-z0-9-]{0,62}$` (lowercase alphanumeric and hyphen, 1–63 chars, starting alphanumeric) and may not be a reserved route segment. (The local single-project STDIO/UDS install is unaffected — it keeps its path-hash identity and requires no slug; see "Wire into your project".)

#### Backup and restore a per-slug project

To back up or move a single project's knowledge between personal-cloud instances, `export`/`import` take a `--slug <slug>` flag that targets the running project's actual per-slug store (`{base}/<slug>/unimatrix.db`, base derived from `--project-dir`) rather than the CLI's path-hash store. `--slug` means "a store dir under the base," **not** "a registered project" — a store directory is resolved directly, so the flag also works on a de-registered slug whose data still exists. The expected posture is to `exec` into the container (where `HOME=/data`, so the base is `/data/.unimatrix`) and invoke the binary there.

**Backup (export):**

```bash
# exec into the container, then:
unimatrix --project-dir <dir> export --slug <slug> -o dump.jsonl
# stderr: exported N entries, M audit rows → dump.jsonl
```

`exported 0 entries` means the resolve found an empty or wrong store — check the slug and `--project-dir`.

**Restore (import) — canonical sequence (the order is load-bearing):**

```bash
1. unimatrix project register <slug>              # creates {base}/<slug>/{unimatrix.db, vector}, writes [[projects]]
2. unimatrix stop                                 # daemon releases the per-slug stores; the live-PID gate clears
3. unimatrix --project-dir <dir> import --slug <slug> -i dump.jsonl
4. unimatrix start                                # daemon boots and loads the rebuilt index
```

After `start`, the restored slug serves the full corpus — **including vector search** — because the rebuilt `{base}/<slug>/vector` index is the one the daemon loads at boot.

- **Why `stop` is mandatory.** `import --slug` hard-errors while a live daemon holds the store: a running daemon would overwrite the freshly rebuilt vector index with its stale in-memory copy at the next shutdown. This is a refusal, not a warning, and there is no `--force` override — take the daemon down across the import.
- **Restore into a freshly-registered slug.** The target must have an empty audit log. Re-importing into a slug that already has audit history fails loud (audit history is append-only and cannot be cleared) — register a fresh slug and import there.

### Build from Source

Prerequisites:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library


**Linux (manual):**
```bash
# Download from https://github.com/microsoft/onnxruntime/releases
# Extract and set ORT_DYLIB_PATH or install to /usr/lib
```

**Devcontainer:** ONNX Runtime pre-installed. No setup needed.

Build:
```bash
cargo build --release --workspace
```

Binary at `target/release/unimatrix`.

### Wire into your project

Run this once from your project root:

```bash
npx unimatrix init
```

This configures everything automatically — MCP server, hooks, skills, and database. It is safe to re-run; existing configuration is preserved.

Then start a Claude Code session and run:

```
/uni-init     # Informs claude Unimatrix exists
```

That's it. Unimatrix is ready to use.

#### Remote (HTTP) mode

To wire a project against a networked Unimatrix server instead of a local binary, use the canonical bundle attach (`init --bundle <blob>`, see "Bundle-driven attach" below). The `--remote <url> --token <tok>` form below is **legacy** — documented for completeness, effectively unused, and bundle-only cloud MCP is not supported on it:

```bash
# legacy — prefer init --bundle <blob>
npx @dug-21/unimatrix init --remote https://uni.example.com --token <token>
```

This configures `.claude/settings.json` hooks to invoke the pure-JS HTTP hook client (`node /abs/path/lib/hook-client/index.js <EVENT>`) for the full remote event set, including `PreCompact` and `PostToolUseFailure`. The bearer credential (token, `observe_url`, and — on the bundle path — the certificate fingerprint) is written out-of-tree to `~/.unimatrix/<projectHash>/remote.json` (mode 0600), keyed by `projectHash` — never inside the repo working tree and no longer to `.claude/settings.local.json`, so a stray `git add -A` cannot commit a live credential. The environment variables `UNIMATRIX_REMOTE_URL` and `UNIMATRIX_REMOTE_TOKEN` override the file when set. Init validates connectivity with a `Ping` request before writing config. No platform binary or ONNX model is required, so remote mode works on Linux, macOS, and Windows with Node >= 18. The legacy `--remote`/`--token` env-HTTPS path does not register a `unimatrix` MCP server — cloud MCP is bundle-only, and `init` emits a loud, deterministic message stating that cloud MCP requires a `v:2` bundle (see "Bundle-driven attach" below); the path forward for a legacy client is to migrate to a `v:2` bundle. The merge is idempotent — re-running preserves non-unimatrix hooks and recognizes its own entries.

##### Bundle-driven attach (pinned TLS)

To attach against an HTTPS server that serves a self-signed certificate (the container HTTPS posture above), use the per-project connection bundle the operator emits with `unimatrix client-bundle <slug>` instead of passing the URL and token separately:

```bash
npx @dug-21/unimatrix init --bundle <blob>
```

The bundle is per-project (`v:2`): it carries the server-composed MCP and observe endpoint URLs (fully-formed, slug already baked in by the server), the bearer token, and the certificate fingerprint in one opaque string. The client composes no paths and appends no slug — it reads the finished URLs from the validated bundle and posts to them verbatim, so a client can never mis-target another project's store.

A bundle attach wires the full `context_*` MCP tool set over HTTPS, not just the observe/hook telemetry surface. `init` writes a `stdio` `unimatrix` MCP server entry into `.mcp.json` whose command invokes a pure-JS stdio→HTTPS bridge (`unimatrix mcp-bridge <projectHash>`, routed to JS — no platform binary required); the bridge opens a fingerprint-pinned TLS connection to the bundle's MCP URL, forwards the bearer token (read from the out-of-tree `~/.unimatrix/<projectHash>/remote.json` store at spawn time, never on the command line or in `.mcp.json`), and proxies stdio JSON-RPC to the cloud's Streamable-HTTP MCP endpoint. Claude Code does no TLS itself — it sees a normal stdio server. A cloud-attached client can therefore `context_search`/`context_get`/`context_store` over HTTPS at parity with the local install. The `.mcp.json` write is idempotent, merge-preserving, and `--dry-run` aware. The client pins the server's exact certificate by its `sha256:` fingerprint (no CA-trust path), so a self-signed cert is trusted by pinning rather than by a certificate authority. A wrong or rotated certificate is rejected with a clear, diagnosable fingerprint-mismatch error directing you to re-bundle. This is a pure-JS, copy-installed remote attach (under 250 KB — no platform binary, no ONNX model) and works on Linux, macOS (Apple Silicon), and Windows with Node >= 18; `init` copies skills and prints the `/uni-init` pointer (it does not append the CLAUDE.md knowledge block — `/uni-init` owns that). Each client instance is bound to exactly one project: a different project means a separate bundle and a separate client instance. Multiple clients may attach the same project (N clients sharing one project, attributed per session); a single client never spans multiple projects. Multiple distinct LLM CLIs (Claude Code, Codex CLI, Gemini CLI) attach the same server identically — each is a separate client connection.

When the operator rotates the server certificate, re-run `client-bundle <slug>` and re-run `init --bundle <blob>` on each client with the new bundle. See [docs/cert-rotation.md](docs/cert-rotation.md) for the operator rotation procedure.

### Brownfield Applications

```
/uni-seed     # Interactive skill to review existing project and seed Unimatrix with decisions(ADR's), patterns, procedures
```

### First Use Examples

**Search for existing knowledge:**
```
context_search(query: "error handling patterns", category: "pattern")
```

**Store a new decision:**
```
context_store(
  content: "Use SQLite for local storage — zero cloud dependency, single-file database.",
  topic: "nxs-008",
  category: "decision",
  title: "Storage backend choice"
)
```

**Get a knowledge briefing before starting implementation:**
```
context_briefing(
  topic: "col-031",
  task: "Implement per-agent rate limiting middleware for the MCP request handler — need patterns for token bucket implementation, existing middleware conventions, and any prior decisions on request-level enforcement"
)
```

**Example of structured 'protocols' for delivery**
```
Read how to use context_cycle commands to gain maximum value in .claude/protocols/uni/README.md
```

---

## Why Unimatrix

Multi-agent development creates knowledge that lives in context windows and dies with sessions. Decisions get re-made, patterns get re-discovered, lessons get re-learned.

- **Project knowledge is *dynamic*** - An architecture decision can change from one feature to the next.  Unimatrix makes that 1 operation and always provides the most up to date info to agents. 

- **Auditable Knowledge Lifecycle** — Every entry has a SHA-256 content hash. Corrections create hash-chained supersession links. An append-only audit log records every operation with agent identity and session context. You can trace how any piece of knowledge evolved.

- **Invisible Delivery** — Agents do actively search and use knowledge from Unimatrix.  Hook-driven integration ALSO injects relevant expertise into prompts, subagent spawns and precompaction events automatically, providing a tag team approach focused on ensuring relevant information while ALWAYS being context consumption conscious. Its a tag team approach to provide the best information.

- **Self-Learning** — Confidence scoring evolves from real usage signals: accesses, helpfulness votes, correction quality, creator trust, and co-access patterns. Entries that help get boosted; entries that mislead get downranked. Adaptive embeddings (MicroLoRA) tune search to project-specific usage patterns.

---

## Core Capabilities

Unimatrix provides these capabilities out of the box.

### Self-Learning Knowledge Engine

Captures decisions, patterns, conventions, procedures, and lessons from real feature work. Seven knowledge categories ensure entries surface in the right context. Confidence scoring combines usage signals, correction quality, creator trust, freshness, helpfulness, and co-access patterns into a composite score that evolves automatically. No manual curation required — the system learns what is useful from how knowledge is accessed and rated.

### Graph-Enhanced Retrieval

HNSW vector similarity locates an initial candidate pool from the 384-dimension embedding space. Personalized PageRank (PPR) co-access traversal then expands the pool by walking the knowledge graph — surfacing cross-category entries that pure vector search misses, because they were frequently retrieved together with the initial candidates in past sessions. A confirmed +0.0122 MRR improvement comes from this expansion step alone. Phase-conditioned category affinity stratifies results by the current workflow phase: entries from categories that historically appear during the active phase receive a ranking boost, calibrated from the per-(phase, category) frequency table rebuilt each background tick. Co-access ranking promotes entries retrieved together in prior sessions. The three layers compose in sequence: semantic similarity → PPR graph expansion → phase-conditioned and co-access re-ranking. Filters by topic, category, tags, and status apply throughout. Near-duplicate detection (cosine similarity >= 0.92) prevents redundant entries at write time.

### Adaptive Embeddings (MicroLoRA)

All-MiniLM-L6-v2 ONNX model runs locally — no API calls, no cloud dependency. A MicroLoRA layer adapts frozen embeddings to project-specific usage patterns. Search relevance improves over time as the system learns which entries are accessed together. 384-dimension vectors with HNSW index for fast approximate nearest-neighbor search.

### Hook-Driven Invisible Delivery (Cortical Implant)

Automatic context injection on every prompt via the `UserPromptSubmit` hook. Six hook events drive the integration: `UserPromptSubmit`, `SubagentStart`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`. Subagent injection: when the SM spawns a subagent, the `SubagentStart` hook fires synchronously and injects relevant knowledge into the subagent context before its first token — this combined with a `context_briefing` call on the outset, provides agents with an index of the most relevant artifacts to their goal and task. `UserPromptSubmit` injection requires at least 5 words in the prompt; shorter inputs (e.g., "yes", "ok continue") are recorded but produce no injection. **No guidance is better than misdirection**. Compaction resilience: `PreCompact` preserves critical context before Claude Code's context window compaction; the compaction payload is a flat indexed table of active entries (up to k=20) plus a session histogram summary. Closed-loop feedback: the `Stop` hook records session outcomes for confidence evolution. Sub-50ms round-trip budget per hook event. Disk-backed event queue for graceful degradation. Single binary — the `hook` subcommand connects to the running MCP server via Unix domain socket IPC. Hooks provide the telemetry necessary for Unimatrix to learn.

Multi-provider hook support: Gemini CLI events (`BeforeTool`, `AfterTool`, `SessionEnd`) are normalized to canonical Unimatrix names at the ingest boundary — no downstream code sees provider-specific strings. Codex CLI uses the same event names as Claude Code; the `--provider codex-cli` flag on the `unimatrix hook` subcommand disambiguates attribution. Reference configurations are provided at `.gemini/settings.json` and `.codex/hooks.json`. Codex live MCP hook support is pending resolution of Codex upstream bug #16732.

Remote (HTTP) hook client: for deployments connecting to a networked server, a pure-JS hook client ships in the `@dug-21/unimatrix` npm package (`lib/hook-client/`) — no platform binary or ONNX model required, so it runs on Linux, macOS, and Windows with Node >= 18. It reads hook stdin, builds the same `HookRequest` the Rust hook builds, and POSTs to the server-composed observe URL from the connection bundle (the client composes no path) with `Authorization: Bearer <token>`; sync events (`UserPromptSubmit`, `PreCompact`, `SubagentStart`) request `Accept: text/plain` so the server formats injection text. On fire-and-forget events it streams transcript deltas (`[last_offset, file_len)`) in a separate POST so the server's per-session buffer holds the authoritative conversation — bringing remote `PreCompact` restoration to local fidelity. It is fail-open (exit 0 always, never blocks the host CLI), has zero runtime dependencies, and uses a disk-backed event queue for graceful degradation. On Unix the same client also runs in local mode: with no remote config it connects to the daemon's hook IPC socket (`unimatrix.sock`) over a Unix domain socket (UDS), framing `HookRequest` messages byte-identically to the Rust hook. Transport is selected automatically — remote config (the out-of-tree `~/.unimatrix/<projectHash>/remote.json` credential store written by remote `init`, or the `UNIMATRIX_REMOTE_*` env-var pair) selects HTTP; its absence selects local UDS. On the bundle path the client reads `observe_url` and the certificate fingerprint from that store and POSTs over a fingerprint-pinned HTTPS connection. UDS local mode is Unix-only, so Windows always uses the HTTP path. Configure remote mode with `npx @dug-21/unimatrix init --remote <url> --token <tok>` (see "Wire into your project"). The hook event set written by `init` covers 8 events by default: `SessionStart`, `Stop`, `UserPromptSubmit`, `PreToolUse` (narrowed to `context_cycle` cycle-event interception — standalone tool observation is no longer registered), `PostToolUse`, `PostToolUseFailure`, `SubagentStart`, `PreCompact`. `SubagentStop` is opt-in: set `unimatrix.hooks.subagent_stop: true` in `.claude/settings.local.json` (default off) to register it.

### Cycle Review Analysis

Analyzes session telemetry for a completed feature cycle and produces the `# Unimatrix Cycle Review —` report. 22 detection rules across 4 categories: agent behavior, friction points, session health, and scope indicators. Rules are domain-aware: each rule guards on `source_domain` as its first filter, so Claude Code rules never fire on events from other domains. A domain pack registry loaded at startup from TOML defines which event types, categories, and detection rules apply to each domain; the "claude-code" pack is always active with no config required. Historical baselines with outlier detection surface anomalies. Evidence synthesis produces actionable findings with supporting data. Lessons and patterns extracted from retrospectives are stored back in the knowledge base with de-duplication via correction chains.

The report header surfaces the feature goal, any run-identity tags recorded at cycle start (a `## Tags` section, present only when the run carried tags), inferred cycle type (Design, Delivery, Bugfix, Refactor, or Unknown), attribution path used (cycle\_events-first, sessions.feature\_cycle legacy, or content-scan fallback), and an in-progress indicator when no `cycle_stop` event exists. A Phase Timeline table breaks the cycle into per-phase windows showing duration, pass count, agents spawned, records, knowledge throughput, and gate outcome. A "What Went Well" section surfaces non-outlier favorable baseline signals that were previously hidden. Per-finding evidence is rendered as relative-time burst notation (`Timeline: +0m(N) +12m(N▲) …`) rather than raw epoch values. The Knowledge Reuse section splits served entries into cross-feature (from prior cycles) and intra-cycle buckets with a top-entry breakdown. Recommendations appear immediately after the header, before all other sections.

The review also persists durable per-cycle aggregate columns so cycles can be compared over time rather than recomputed each call: phase durations, transitions, and rework loops (including phases declared but never closed, surfaced as hotspots); the rework/failure session ratio; a knowledge-reuse count over all entries served to the cycle (the union of query and injection logs, not only same-cycle-tagged entries); transcript throughput as a byte total and delta count; content-opaque behavioral-signal counts (e.g. `error`, `refusal`); a compaction count; and two distinct reload metrics — a cross-session `context_reload` (continuity/handoff cost) and a post-compaction within-cycle `compaction_reread` (the compaction tax). The two reload signals are never collapsed into one number. Two presentation-honesty rules govern rendering: a metric whose source data class is empty for the cycle renders **"unavailable"** rather than a `0` indistinguishable from a measured zero, and the behavioral-signal counts render with a coarse/directional qualifier — they are unvalidated, content-opaque regex matches, not exact auditable totals — visually distinct from exactly-counted aggregates such as phase and compaction counts. The throughput unit is bytes, never tokens or cost: these metrics inform the process; they never control execution. An optional `auto_close` parameter writes the `cycle_stop` event synchronously before the review pipeline when the cycle has no stop yet, so a final retrospective can close and review the cycle in one call.

The review is fully non-destructive: it never purges the in-memory transcript buffers on any path or parameter combination — there is no purge verb. Reclamation is delegated entirely to the unchanged backstops (24h TTL staleness sweep, 64-session hold-cap eviction, per-turn session-close purge), so a second, identical review returns the same candidates until a backstop reclaims them. Verbatim transcript candidates are returned only on explicit request, via a read-only scoped `transcript: { phase?, anchor?, match?, window? }` retrieval block — all-optional, AND-composed filters that narrow the existing candidate pipeline (whole marker-matched user/assistant blocks against four marker families: decision phrases, rework signals, lesson markers, phase/gate markers) via a read-only snapshot before attach. Omit `transcript` and the response is the observation-derived summary only (the lean default — no raw bytes); `transcript: {}` returns the full candidate set under the per-cycle cap (equivalent to `match: ".*"`). Every returned session carries per-session loss info (`matched`, `search_complete`, `elided_bytes`, `provenance`) so a `match` no-match over a lossy or `Reconstructed` session reads as INDETERMINATE, never a silent false negative; the agent queries in its own units (finding/anchor id, phase id, regex, an event or time window) and the server normalizes cross-plane clock skew internally. The calling agent performs all semantic extraction into `context_store`. When a session's buffer is empty or hole-ridden, a labeled degraded fallback reconstructs input from that session's stored observations. Candidates are never persisted — they do not ride the memoized cycle-review record, and reflect call-time buffer content (a memoization hit honors the `transcript` block identically to the full pipeline). The 22 detection rules' inputs are unchanged.

### Behavioral Signal Delivery

Cycle outcomes recorded via `context_cycle` feed as graph edges, reinforcing co-access signals between entries retrieved during successful delivery phases. Each time a phase completes with a positive outcome, the knowledge retrieved during that phase gains stronger co-access links — future agents entering the same phase surface those entries higher. `context_briefing` operates as a targeted handoff at phase transitions: it uses the current phase and the cycle's history to prioritize knowledge relevant to the agent's declared phase, delivering a structured top-k result set without requiring the agent to search. This goal-conditioned briefing, combined with UDS injection, makes knowledge delivery phase-aware and progressive rather than flat. Reference: crt-046, Group 6.

### Contradiction Detection

After each `context_store`, a background scan checks the new entry against its top HNSW neighbors using cosine similarity. Pairs with similarity >= 0.65 are recorded as `Supports` edges in the knowledge graph. Contradiction density — the ratio of unresolved contradictions to active entries — is one dimension of the Lambda structural health metric, computed periodically and surfaced in `context_status` health reports. When contradictions are identified, `context_correct` is the resolution path: it deprecates the conflicting entry and links the replacement through a hash-chained supersession record. No external model is required for contradiction management.

### Domain-Agnostic Observation Pipeline

Every detection rule carries a `source_domain` guard — a rule fires only for events from its declared domain, never cross-contaminating signals from unrelated systems. Domain packs are registered via `[[observation.domain_packs]]` entries in `config.toml`, specifying the source domain, event types, and applicable knowledge categories. The built-in "claude-code" domain pack is always active and requires no configuration — it covers all Claude Code lifecycle hook events out of the box. Any domain's event stream connects to the learning layer by registering a domain pack; no code changes are required. `source_domain` is validated at both ingest and registration: values must match `^[a-z0-9_-]{1,64}$`. Reference: W1-5, col-023.

Provider identity (`"claude-code"`, `"gemini-cli"`, `"codex-cli"`) is derived from the `--provider` flag or inferred from the hook event name at the ingest boundary and carried through as `source_domain` on observation records. Gemini CLI events with unique names (`BeforeTool`, `AfterTool`, `SessionEnd`) are unambiguously identified without a flag; Codex CLI requires `--provider codex-cli` because it shares event names with Claude Code.

### Correction Chains with Audit Trails

`context_correct` creates a new entry and deprecates the original, linking them with SHA-256 content hashes (`previous_hash` chain). The append-only audit log records every operation — store, correct, deprecate, quarantine, enroll — with agent identity, session context, and operation outcome. Correction chains are tamper-**recorded**: every correction links to its predecessor by SHA-256 `content_hash` (the `previous_hash` chain), and `unimatrix verify` (and import-time validation) detect accidental corruption or single-point tampering — a content edit not perfectly mirrored across an entry's `content_hash` and its successor's `previous_hash` fails verification, naming the offending entry. This is correction-history integrity, not tamper-evidence against an adversary with raw database write access (who holds all secrets — out of tier); cryptographic tamper-evidence against that adversary is a future hardening step.

### Coherence Gate (Lambda Health Metric)

Lambda is a composite structural integrity metric [0.0, 1.0] computed from three dimensions: graph quality (weight 0.46 — is the vector index structurally sound?), contradiction density (weight 0.31 — how many unresolved contradictions exist?), and embedding consistency (weight 0.23 — do entries have valid, current embeddings?). When lambda drops below 0.8, maintenance is recommended. A background tick handles maintenance automatically — confidence refresh, graph compaction, co-access cleanup.

`context_status` also reports five graph cohesion metrics computed per-call from the `GRAPH_EDGES` table: connectivity rate (fraction of active entries with at least one non-bootstrap edge), isolated entry count, cross-category edge count, Supports edge count, mean entry degree (in+out). These metrics are informational — they do not feed into lambda — but let operators verify whether automated platform is driving cross-category graph that PPR can exploit. Summary format includes a single "Graph cohesion:" line; Markdown format includes a `### Graph Cohesion` sub-section within the Coherence block.

### Content Scanning

Every `context_store` and `context_correct` call scans content for injection patterns (~25+ patterns including prompt injection attempts, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected with a descriptive error before storage.

### Agent Trust Hierarchy

Four-tier trust model: System > Privileged > Internal > Restricted. Four capabilities gate tool access: `read`, `write`, `search`, `admin`. Unknown agents auto-enroll as Restricted (read + search only) on first contact. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability. `context_enroll` (Admin-only) manages agent trust levels and capabilities at runtime.  This is mostly unused in currently supported STDIO mode.  More to come

### Knowledge Effectiveness Analysis

Per-entry utility scoring from injection logs and session outcomes. Confidence calibration validation — does predicted quality match actual usefulness? Dead knowledge detection — entries that are never accessed after initial storage.

---

## Tips for Maximum Value

1. **Treat Knowledge Curation as 1st class requirement.**  Agents should be encouraged to search AND store important knowledge future agents should know about their decisions, activities, etc.  

2. **Start a new session per feature cycle.** Context window pollution across features reduces knowledge quality. Each feature cycle (e.g., `col-015`) should use a fresh Claude Code session.

3. **Use `context_cycle` to declare start/top and phase transitions for your workflow.** Eg: `Spec`, `Dev`, `Testing`. The system learns the content categories used in each cycle.  **See `.claude/protocols/uni/README.md` for more details

4. **Run Retrospectives** Use `context_cycle_review` to learn about what happened on this feature.  Unimatrix looks for 21 potential hotspots that serve to improve your workflows and stores this summary.  Its also a good opportunity to quality check knowledge stored during the feature_cycle.  Storing the summary also enables the proper release of the telemetry data to avoid unwieldly db growth, while retaining the summary.

5. **Category discipline matters.** The right category determines retrieval quality. Decisions (`decision`) are not conventions (`convention`); procedures (`procedure`) are not patterns (`pattern`). Miscategorized entries surface in wrong contexts during semantic search.

6. **Cold start: use `/uni-seed`.** A fresh knowledge base returns empty search results. `/uni-seed` populates foundational entries before relying on search.

---

## Configuration

Unimatrix loads configuration from up to two optional TOML files at server startup. When neither file is present, all compiled defaults apply.

- `~/.unimatrix/{project-hash}/config.toml` — **primary** (per-project). Written automatically on first run, and also seeded as an annotated default on a container `serve` (with `http.enabled`) when absent. Seeding never overwrites an existing file. This is the canonical config location.
- `~/.unimatrix/config.toml` — **defaults** (global). Optional cross-project defaults; values here apply to all projects unless overridden per-project. List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely — there is no append behavior.

Config is loaded once at startup. Changes require a server restart. A malformed file or a security validation failure aborts startup with a descriptive error.

### Per-Slug Configuration Overlay (multi-project / cloud)

On a multi-project (cloud / HTTP) deployment, each registered slug gets its own `config.toml` in that slug's data directory — `{base_dir}/{slug}/config.toml`, the sibling of the slug's `unimatrix.db` and `vector/` index (`/data/.unimatrix/{slug}/config.toml` in the container layout). `unimatrix project register <slug>` seeds this file automatically: an annotated default with per-slug-overlayable keys rendered editable and global-locked keys included but commented out and marked "managed globally", documenting the per-slug/global boundary in place so the operator never has to reverse-engineer the format or hand-place the file. Seeding never overwrites an existing file — an operator-authored config survives a re-register. The file applies on the next daemon restart (the same restart that re-attaches routing); there is no hot-reload.

The per-slug file overlays the daemon's resolved global config **per key** — a key it sets overrides only that key, every key it leaves unset falls through to the global value. With no per-slug file present a slug is served the global config unchanged.

What a per-slug file may override (per-slug):

- `[knowledge]` categories, `boosted_categories`, and lifecycle
- `[observation.domain_packs]` domain packs
- `[confidence]` weights
- Overlayable `[inference]` tuning — fusion/PPR weights, `nli_top_k`, `nli_enabled` (the `*_sha256` hash pins stay global)
- `[server] instructions`

What stays global, even if a per-slug file sets it (the overlay ignores these; setting one logs one `tracing` WARN per ignored key, once per boot, naming the key and the slug — the value is dropped, not rejected, so the daemon keeps serving):

- Both ONNX models and the shared inference pool — the NLI model, the entire `[embedding]` section (model, dimensions, sha256), and `rayon_pool_size`. Exactly one of each model is loaded regardless of how many slugs are registered.
- The daemon permission posture (`[agents] default_trust` / `permissive`)
- All transport config — `[http]` and `[tls]` (host, port, auth, `http.enabled`)

An invalid per-slug file (unknown category, oversized instructions, malformed TOML, or world/group-writable permissions), or a valid file whose merge with the global config violates a cross-field invariant, fails the daemon loud at startup and names the offending slug.

### Profile Presets

The `[profile]` section selects a knowledge-lifecycle preset. Presets encode calibrated confidence weight vectors and freshness half-life values so operators identify their knowledge type rather than tuning ML weights directly.

```toml
[profile]
preset = "collaborative"   # default — matches current compiled behavior
```

| Preset | Best for | Freshness half-life |
|--------|----------|---------------------|
| `collaborative` | Team-built knowledge, dev/research (default) | 8760 h (1 year) |
| `authoritative` | Policy, standards, legal precedents — source trust dominant | 8760 h (1 year) |
| `operational` | Runbooks, incidents, procedures — freshness dominant | 720 h (1 month) |
| `empirical` | Sensor feeds, metrics, time-series — recency critical | 24 h |
| `custom` | Expert use — requires all six weights in `[confidence]` section | set explicitly |

### Key Config Sections

```toml
[knowledge]
# Replace the built-in 7-category list with domain-appropriate categories.
# Values: lowercase, [a-z0-9_-], max 64 chars, up to 64 categories total.
categories = ["lesson-learned", "decision", "convention",
              "pattern", "procedure"]
# Categories surfaced more prominently in search re-ranking (provenance score boost).
boosted_categories = ["lesson-learned"]
# Categories eligible for automated lifecycle management (retention, auto-deprecation).
# All other categories require explicit operator action to deprecate.
# Prerequisite for signal-driven retention (#409). Default: ["lesson-learned"].
adaptive_categories = ["lesson-learned"]
# Freshness decay rate. Overrides the preset's built-in value when set.
freshness_half_life_hours = 8760.0

[server]
# MCP server instructions passed to every connecting agent during the initialize handshake.
# Injection patterns are validated at load time; startup aborts if triggered.
instructions = "..."

[agents]
# Auto-enroll behavior for unknown agents.
# "permissive" grants [Read, Write, Search]; "strict" grants [Read, Search].
default_trust = "permissive"
session_capabilities = ["Read", "Write", "Search"]

[inference]
# Number of threads dedicated to ML inference (ONNX embedding, GNN).
# Default: (num_cpus / 2).max(4).min(8) — at least 4 threads, at most 8.
# Valid range: [1, 64]. Out-of-range value aborts startup with a structured error.
rayon_pool_size = 4

# Session context affinity weights (WA-2).
# w_phase_histogram: boost weight for implicit category histogram signal. Applied inside
# compute_fused_score. Max boost = w_phase_histogram * 1.0 = 0.02 per entry (default: 0.02).
w_phase_histogram = 0.02
# w_phase_explicit: boost weight for explicit phase signal (WA-1 current_phase). Activates
# the PhaseFreqTable — a per-(phase, category) frequency table rebuilt each background tick
# from query_log. Entries accessed frequently in the current phase receive a higher
# phase_explicit_norm contribution. Cold-start guard: when no phase history exists,
# phase_explicit_norm = 0.0 and scores are bit-for-bit identical to pre-col-031.
# Default 0.05 (additive, outside the six-weight sum constraint).
w_phase_explicit = 0.05
# query_log_lookback_days: time window (in days) for the PhaseFreqTable rebuild SQL query.
# Only query_log rows within this window contribute to phase-frequency rankings.
# Default 30. Increasing this window widens the historical signal; decreasing it makes
# rankings more sensitive to recent access patterns.
query_log_lookback_days = 30

[retention]
# Number of completed (reviewed) feature cycles to retain activity data for.
# Observations, query_log, sessions, and injection_log for cycles beyond this
# window are deleted after their cycle_review_index row exists.
# Governs the ceiling for PhaseFreqTable lookback and future GNN training window.
# Range: [1, 10000]. Default: 50.
activity_detail_retention_cycles = 50

# Maximum number of purgeable cycles to process in a single maintenance tick.
# Limits tick budget consumed by GC. Older cycles are processed first.
# Deferred cycles are picked up on the next tick.
# Range: [1, 1000]. Default: 10.
max_cycles_per_tick = 10

# Retention window in days for audit_log rows.
# Audit data is an accountability record, not a learning signal.
# Range: [1, 3650]. Default: 180.
audit_log_retention_days = 180

# Purge policy for raw session transcript (ephemeral, possibly secret-bearing
# conversation bytes — distinct from distilled knowledge, observations, and the
# audit log, which have their own retention knobs above).
# Values:
#   "PurgeOnCycleClose"  — purge the in-memory transcript buffer at session close
#                          and the staleness sweep (cycle review no longer purges;
#                          context_cycle_review is fully non-destructive). Default,
#                          and the only value the OSS build accepts.
#   { RetainDays = N }   — enterprise retain-N-days seam. REJECTED at startup by
#                          the OSS build with an enterprise-only validation error.
transcript_retention = "PurgeOnCycleClose"

# Maximum accumulated size in bytes of a session's in-memory transcript buffer.
# On overflow the most recent content is kept (ring-tail) and the dropped-byte
# count is recorded as metadata — never as marker bytes in content. Per-session
# bound; aggregate memory is this cap times the number of concurrent sessions.
# Floor: 65536 (64 KiB) — values below abort startup. Default: 4194304 (4 MiB).
transcript_buffer_max_bytes = 4194304

# Per-session cap (bytes) on the transcript candidates a single session may
# contribute to the cycle-review distillation pass. Caps distilled output volume,
# not the raw buffer (which transcript_buffer_max_bytes governs).
# Range: [1, transcript_candidate_cycle_cap_bytes]. Default: 24576 (24 KiB).
transcript_candidate_session_cap_bytes = 24576

# Per-cycle aggregate cap (bytes) on selected candidates across all sessions in
# one cycle-review distillation pass. Truncation is deterministic chronological
# keep-earliest; a cap-forced drop is reported in the candidates loss info.
# Range: [transcript_candidate_session_cap_bytes, usize::MAX]. Default: 262144 (256 KiB).
transcript_candidate_cycle_cap_bytes = 262144

# Hole-fraction threshold for the reconstruction fallback. When the fraction of a
# session's snapshot window lost to holes/elision meets or exceeds this ratio, the
# session is routed through the labeled degraded reconstruction path (stored
# observations) at the documented fidelity floor.
# Range: [0.0, 1.0]. Default: 0.5.
transcript_fallback_hole_fraction = 0.5

# Held-buffer count ceiling. The held-buffer store keeps drained multi-turn
# transcript buffers alive across the per-turn drain so the cycle-review
# distillation path is non-empty. On the (cap+1)th hold the oldest held buffer is
# evicted (every eviction audited). Memory is bounded by transcript_buffer_max_bytes
# times this value. One of two independent reclamation mechanisms (the other is the
# TTL sweep). Range: [1, usize::MAX]. Default: 64.
transcript_hold_max_sessions = 64

# Held-buffer stale-sweep TTL in seconds. A held buffer untouched for longer than
# this is reclaimed by the maintenance-tick stale sweep, independent of whether
# cycle review ever fires. The second independent reclamation mechanism; either
# alone bounds memory. Range: [1, u64::MAX]. Default: 86400 (24 h).
transcript_hold_ttl_secs = 86400
```

```toml
# [transcript_signals] — content-free behavioral-signature classes folded over the
# in-memory transcript stream (sibling to [retention]; #[serde(default)]).
# Each enabled class is a labelled regex matched against raw transcript-delta bytes
# in a single shared scan; only a per-class match COUNT is kept — no transcript
# content is ever stored or surfaced. The counts are DIRECTIONAL, not precise: a
# non-zero count means "this cycle saw matches," not an audited incident total
# (the producer is content-opaque, so the false-positive rate cannot be audited
# after the fact). When omitted, the default catalog ships exactly two domain-neutral
# classes: `error` (index 0 — provider/model hard + overload errors) and
# `refusal` (index 1 — first-person model refusal phrasings). No SDLC literals.
#
# Validation is loud at startup: more than 16 enabled classes, an invalid regex,
# or a duplicate class_name aborts the server with a descriptive error.
[[transcript_signals]]
class_name = "error"      # Stable label; maps to a fixed index by config order.
pattern    = "..."         # Regex compiled once into one shared RegexSet.
enabled    = true          # Disabled entries are parsed but excluded from the scan.
```

```toml
# [observation] — domain pack registry (optional; omit for Claude Code-only deployments)
# The "claude-code" pack is always loaded as the built-in default.
# Domain pack changes require a server restart — runtime re-registration is not supported.
[[observation.domain_packs]]
source_domain = "sre"                    # Must match ^[a-z0-9_-]{1,64}$
event_types   = ["incident_opened", "incident_resolved", "alert_fired"]
categories    = ["runbook", "post-mortem"]
# Built-in domain packs (claude-code) register detection rules as Rust code.
# External packs declare threshold or temporal-window rules as TOML descriptors.
```

```toml
# [http] — HTTPS transport (disabled by default)
[http]
enabled = false               # Set to true to activate the HTTPS listener
content_port = 8443           # TCP port for MCP, health, and observe endpoints
bind_address = "0.0.0.0"     # Bind address (use 0 for OS-assigned port in testing)
max_connections = 32          # Maximum concurrent HTTP sessions

# [tls] — TLS termination (auto-enabled when cert_path and key_path are present)
[tls]
enabled = true                # Set to false for reverse-proxy deployments (plain HTTP)
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
```

A 32-byte bearer token is generated automatically at `{data_volume}/token` on first server start with HTTP enabled. Under the cloud HTTPS posture the token is never printed to stdout or written to logs — the remote client obtains it solely through the per-project connection bundle (`unimatrix client-bundle <slug>`). Subsequent starts load it silently. See [docs/client-setup.md](docs/client-setup.md) for client configuration.

**Container HTTPS env vars.** For container deployments, two environment variables drive the HTTPS posture without editing config files (the distroless runtime has no shell, and the global binary default `http.enabled` stays `false`):

- `UNIMATRIX_HTTP_ENABLED=true` — activates the HTTPS listener, container-scoped (an env override of `[http] enabled`).
- `UNIMATRIX_PUBLIC_URL` — the URL clients connect to (e.g. `https://uni.example.com:8443`). A single derivation feeds three consumers: the server-composed endpoint URLs in the connection bundle, the `allowed_hosts` default, and the generated certificate's SAN. When unset, the server uses a loud `https://<EDIT-ME>:8443` placeholder and a permissive-with-warning posture; socket auto-detection is not used.

On first boot with HTTP enabled the binary auto-generates the self-signed cert+key alongside the token (key mode `0600`), with the SAN derived from `UNIMATRIX_PUBLIC_URL` plus the local set (`localhost`, `127.0.0.1`, `0.0.0.0`); the operator may mount their own cert/key read-only to override.

Config files are validated for security at load time: world-writable files abort startup; group-writable files log a warning. `[server] instructions` is scanned for injection patterns before use.

---

## MCP Tool Reference

Unimatrix exposes 15 MCP tools. All tools accept `format: "summary" | "markdown" | "json"` as a common parameter — except `context_cycle_review`, which accepts only `"markdown" | "json"` (the former `"summary"` alias is no longer accepted and returns an invalid-params error), and `context_graph`, which treats `format` as serialization-only (`"json"`; `"markdown"` is rejected until a graph-markdown renderer ships) and adds a separate `detail: "summary" | "full"` verbosity axis (default `"summary"`); legacy `format=summary` on `context_graph` remains accepted as a deprecated alias for `detail=summary`.

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `context_search` | Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. | When you need to find patterns, conventions, or decisions related to a concept. Use when you do NOT know exactly what you are looking for. Key params: `query` (required), `category`, `topic`, `tags`, `k` (default 5), `helpful`. |
| `context_lookup` | Look up context entries by exact filters. Returns entries matching topic, category, tags, status, or ID. | When you KNOW what you are looking for — a specific feature's entries, a category listing, or a known ID. Key params: `topic`, `category`, `tags`, `id`, `status`, `limit` (default 10). |
| `context_get` | Get a specific context entry by its ID. When the requested ID points at a deprecated entry, it resolves to that entry's active terminal by default (follow-to-current along the supersession chain, 50-hop cap), returning the terminal's full content in the same shape as a direct get; a one-line resolution notice `↻ Requested #X (deprecated) → returning current version #Y` is emitted only when a hop occurs (clean passthrough with no notice when the requested ID is already the active terminal). If the chain dead-ends on a non-active entry (orphaned/quarantined/>50 hops), the originally-requested entry is returned with a loud non-active flag — never empty, never silent. By default also surfaces a ranked, capped (≤3) set of the entry's depth-1 typed graph edges (both directions) as a next-hop navigation affordance — authored edges first, then inferred edges ranked by target-entry confidence — alongside honest, uncapped totals split inbound/outbound (symmetric edges counted once). Each surfaced edge carries `{edge_type, direction, target_id, target_title, authored}`; `direction` is `→`/`←` for asymmetric types and `↔` for canonicalized symmetric types (`Contradicts`, `CoAccess`, `Informs`). A zero-edge entry renders an explicit empty state; when more than 3 edges exist, a "…N more — use context_graph" pointer is shown. `Supersedes` never appears. The summary format appends an edges digest to the entry line, markdown adds a `### Related` section, and json adds `edges` plus split totals. | When you have an entry ID from a previous search or lookup result and need the full content. Key params: `id` (required), `helpful`, `follow_supersessions` (optional bool — default `true`; resolves a deprecated ID to its active terminal, pass `false` to return the entry exactly as stored for any status — provenance/audit/lookback — with a `deprecated; superseded by #X (pass follow_supersessions=true to follow)` footer when the requested entry is deprecated), `include_edges` (optional bool — default-on; pass `false` to suppress the edge surfacing for latency/payload-sensitive bulk reads, which also omits the `edges` key entirely). |
| `context_store` | Store a new context entry with duplicate detection and content scanning. Each successful non-duplicate store increments the per-session category histogram used by `context_search` for implicit session affinity ranking. | When you discover a pattern, convention, decision, or lesson worth preserving. Key params: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`, `edges` (optional list of `{edge_type, target_id}` to declare typed relationships at creation time). |
| `context_correct` | Correct an existing entry. Deprecates the original and creates a new entry with a hash-chain link. Automatically redirects all incoming graph edges (excluding `Supersedes`) from the deprecated original to the new entry — no separate `context_edge(mode="redirect")` call required. The original's eligible outgoing graph edges (agent-declared types only; `Supersedes` and tick-generated `CoAccess`/`Informs` excluded) are also carried forward to the new entry by default — no need to re-declare them in `edges`. The response includes an `edges_carried` integer count when one or more outgoing edges are carried (omitted when zero). To intentionally drop an outgoing edge that no longer holds, shed it with `context_edge(mode="remove")` or `context_edge(mode="redirect")` against the new entry id (the deprecated original cannot be edited). Response text includes `"Redirected N incoming edges (M failed, see logs)"` when edges are affected. | When an entry contains wrong or outdated information that should be superseded (not just hidden). Key params: `original_id` (required), `content` (required), `reason`, `edges` (optional list of `{edge_type, target_id}` — edges attach to the new corrected entry, not the deprecated original; passed edges compose additively with carried edges on `(source, target, relation_type)`). |
| `context_deprecate` | Mark an entry as outdated. Entry remains accessible but excluded from default search/lookup. Also eagerly deletes the agent-authored graph edges (`source='agent'`, both inbound and outbound) touching the entry, so no live entry retains a dangling reference to the retired one — pulling the periodic orphaned-edge compaction forward for this single entry. Machine-generated edges (`nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`) are left to the background compaction. The response reports an `edges_removed` count (a literal `0` when the delete ran and removed nothing; omitted only when the cleanup did not run or failed), and the removal — including the removed-edge tuples — is recorded in the audit log as a `context_deprecate.edge_cleanup` event. The cleanup is synchronous and non-fatal: a failure never affects the deprecation result, and the background compaction remains the backstop. | When knowledge is no longer relevant but should not be deleted (historical record). Key params: `id` (required), `reason`. |
| `context_quarantine` | Quarantine or restore an entry. Quarantined entries are excluded from search and lookup. **Admin only.** | When an entry is suspicious, invalid, or harmful and should be isolated. Use `action: "restore"` to undo. Key params: `id` (required), `action` ("quarantine" or "restore"), `reason`. |
| `context_status` | Get knowledge base health metrics. Shows entry counts, distributions, correction chains, coherence score, security metrics, graph cohesion metrics (connectivity rate, isolated entry count, cross-category edge count, Supports edge count, mean entry degree, and inferred edge count), per-category lifecycle labels (adaptive vs pinned), `pending_cycle_reviews` (cycle IDs that have started within the retention window but have no stored cycle review yet — always computed), and a `curation_health` aggregate block (per-cycle correction rate mean/stddev, source breakdown as agent% and human%, orphan deprecation ratio mean/stddev, and trend direction when at least 6 cycles of snapshot data are available). **Admin only.** | When you need to assess knowledge base health or inspect whether graph edge inference is producing a connected, cross-category graph, identify cycles awaiting retrospective review before signals can be purged, or review curation behavior trends across recent cycles. The `maintain` parameter is accepted but silently ignored — a background tick handles maintenance automatically. Key params: `topic`, `category`, `check_embeddings`. |
| `context_briefing` | Get a knowledge index for a topic or task. Returns up to 20 active entries as a flat indexed table (columns: row, id, topic, category, confidence, snippet). Query derived from: (1) explicit `task` param. Used at the start of a phase or task to get oriented. Call when starting a new 'task' (often with a new subagent). Key params: `topic` (also known as feature_cycle), `task`, `k` (default 20), `max_tokens` (default 3000, range 500-10000). |
| `context_enroll` | Future use |
| `context_cycle` | Signal feature cycle lifecycle events: start, phase transitions, and stop.  | At cycle start/stop and at each phase boundary. Key params: `type` (required: `"start"` \| `"phase-end"` \| `"stop"`), `topic` (required. Topic and feature_cycle are interchangeable), `phase`, `outcome`, `next_phase`, `agent_id`, `goal` (optional, `start` only: 1–2 sentence plain-text statement of feature intent; used as the step-2 query signal by `context_briefing` and hook injection when no explicit `task` is provided; max 1 024 bytes), `tags` (optional, `start` only: an array of opaque run-identity labels — e.g. `workflow:v1.3`, `mode:batch`, `arm:A`. Set whole-set-once at cycle start; the first tag-bearing start locks the set and every later start is a no-op — there is no post-start mutation. Non-empty strings, stored and returned verbatim; the engine applies no vocabulary, allow-list, or prefix enforcement (any `namespace:` prefix carries meaning to the reader only). Surfaced per-run in `context_cycle_review`). **Proper use of context_cycle provides unimatrix deep visibility into your workflow**|
| `context_cycle_review` | Analyze observation data for a work cycle (Retrospective). Parses session telemetry, detects hotspots, computes metrics, and renders and stores the `# Unimatrix Cycle Review —` report. Persists durable per-cycle aggregate columns (phase durations/transitions/rework, rework ratio, knowledge-reuse served-count, transcript throughput in bytes, behavioral-signal counts, compaction count, and two distinct reload metrics) for cross-cycle comparison. Surfaces any run-identity `tags` recorded at cycle start (via `context_cycle`) in both JSON (a `tags` field) and markdown (a `## Tags` section); a cycle with no tags renders no section. A metric whose source data is empty for the cycle renders **"unavailable"** rather than a misleading `0`; the content-opaque behavioral-signal counts render with a coarse/directional qualifier, never as exact auditable counts. The review is fully non-destructive — it never purges the transcript buffer and has no purge verb; reclamation is delegated to the backstops (TTL sweep, hold-cap eviction, session-close). Verbatim candidates are returned only when the read-only scoped `transcript: { phase?, anchor?, match?, window? }` retrieval block is supplied (all-optional, AND-composed filters over the existing candidate pipeline via a snapshot); each returned session carries per-session loss info (`matched`, `search_complete`, `elided_bytes`, `provenance`) so a no-match over a lossy/`Reconstructed` session is INDETERMINATE, never a silent false negative. Omit `transcript` for the lean observation-only summary; `transcript: {}` returns the full candidate set under the per-cycle cap. Candidates are response-transient, never persisted, for the calling agent to extract into `context_store`. Use `force=true` to recompute the report from durable observations (never retrieves candidates, never purges); `auto_close=true` writes the `cycle_stop` event synchronously before the review when the cycle has no stop yet. | After a work cycle completes, to better understand what worked and what didn't during the cycle. Key params: `feature_cycle` (required), `evidence_limit`, `transcript` (optional scoped-retrieval block — `phase`, `anchor`, `match`, `window`; omit for summary only, `{}` for the full candidate set), `force` (bool, default false — when true, recomputes the report from durable observations even if a stored record exists), `auto_close` (bool, default false — when true and no `cycle_stop` exists, the cycle is stopped synchronously before the review pipeline), `format` ("markdown" default, "json"; the former `"summary"` alias returns an invalid-params error). |
| `context_edge` | Standalone edge lifecycle management on existing entries: add a typed relationship, remove one that no longer holds, or redirect one when a target entry is superseded. Pure graph operation — no embedding recompute, no confidence update. Requires `Capability::Write`. Source entry must be active (not quarantined or deprecated). | When you need to declare, retract, or retarget a typed graph relationship between existing entries without creating a new entry version. Primary use case: retargeting `Advances` or `Prerequisite` edges after a supersession. Key params: `mode` (required: `"add"` \| `"remove"` \| `"redirect"`), `source_id` (required), `edge_type` (required — one of 13 agent-meaningful types: `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo`, `Prerequisite`, `Contradicts`, `Supports`), `target_id` (required), `new_target_id` (required for redirect only). |
| `context_tag` | Mutate a single tag on an existing entry in place — `add`, `remove`, or `replace` — on the non-hashed tag lane. A lightweight, audited **fast path** parallel to `context_correct`: it changes only the tag, leaving the entry's content, hash chain, edges, embedding, and full learning vector (confidence, access/helpful counts) untouched — no supersession version is minted and no learning signal is reset. **Value-opaque**: the engine writes any tag string without interpreting it — there is no tag vocabulary or allow-list. `replace` is atomic (prior tag removed + new inserted in one transaction). Every mutation emits a dedicated audit event (`action`, derived namespace, tag, prior/new value, `agent_id`, timestamp); `agent_id` is audit-only, never an authorization input. Requires `Capability::Write`. Refuses a quarantined entry; a deprecated entry may still be tagged. | When a volatile per-entry tag (e.g. a status label) changes frequently and you want to update it without dragging the change through the heavy `context_correct` path or resetting the entry's learning history. Key params: `id` (required), `action` (required: `"add"` \| `"remove"` \| `"replace"`), `tag` (required), `agent_id` (optional, audit-only). |
| `context_graph` | Graph read operations on the knowledge graph. Seven modes, all requiring `Capability::Read`. `chain`: walks the full supersession history of an entry (ancestors and descendants) via recursive SQL CTE; returns ordered `Vec<EntryRecord>` with per-direction `truncated` flags; 50-hop safety cap. `current`: follows `superseded_by` to the terminal active entry via SQL CTE; 50-hop cap; error if chain terminates at an orphaned deprecated entry. `neighbors`: retrieves entries connected by typed edges; depth=1 queries the live database (all committed writes reflected immediately); depth>1 queries the in-memory graph (may lag recent writes by up to one tick interval); returns flat `Vec<EdgeRecord>`; `Supersedes` excluded from default all-types traversal. `subgraph`: bounded multi-seed BFS returning both nodes and edges (enough to reconstruct the subgraph locally); honors `edge_types` and `direction` (`incoming`/`outgoing`/`both`) filtering during traversal; `max_depth=1` queries the live database (all committed writes reflected immediately) while `max_depth>1` queries the in-memory graph (may lag recent writes by up to one tick interval); 200-node hard cap; `EdgeRecord.direction` is always `"outgoing"` (canonical stored direction); returns `{ nodes, edges, truncated, seed_ids, depth_reached }`. `inverse`: SQL LEFT JOIN antijoin returning active entries of a given `category` that have no incoming edges of ALL specified `missing_edge_types` (AND semantics); queries the live database (no staleness); `limit` default 100, max 500. `filter`: combined category + optional property + optional edge-count filter via correlated SQL subquery against the live database; property filters: `min_age_days`, `min_confidence`, `max_confidence`; edge-count filters: `min_edge_count`, `max_edge_count` (require `edge_types` when present); `limit` default 100, max 500. `path`: shortest BFS path between two entries over the in-memory graph (outgoing edges only, tick-window staleness applies); returns `{ found, from_id, to_id, hops, length }` where `from_id` is not in `hops`; `found: false` when no path exists within depth or when either ID is absent from the current snapshot. **Response axes:** `format` is serialization-only (`json`; `markdown` is rejected with `ERROR_INVALID_PARAMS` until a graph-markdown renderer ships — no silent JSON fallback) and a separate `detail` axis (`summary` \| `full`, default `summary`) controls verbosity. Under `detail=summary` (the default) the node-bearing modes (`chain`, `current`, `subgraph`, `inverse`, `filter`) return a lean node projection `{id, title, category, tags, status, confidence, content_preview, content_truncated}` — `content_preview` is the first ≤256 bytes of `content` floored to a UTF-8 char boundary (no ellipsis) and `content_truncated` flags whether content was elided — and edges as `{source_id, target_id, relation_type, depth}`; the projected `status` is **lifecycle** status (`active`/`deprecated`/`proposed`/`quarantined`), NOT capability delivery status. `detail=full` returns the complete `EntryRecord`/`EdgeRecord` payload unchanged. `neighbors`/`path` carry no node bodies and accept-and-ignore `detail`. | When you need to navigate the knowledge graph without semantic search. Use `chain` to audit correction history; `current` to resolve a deprecated entry to its live successor; `neighbors` for typed-edge neighbor retrieval; `subgraph` to retrieve a bounded evidence or dependency graph from seed entries; `inverse` to find entries of a category with no incoming edges of a given type (orphan/gap detection); `filter` to combine category, property, and edge-count constraints in a single query; `path` to find the shortest typed-edge route between two entries. Key params: `mode` (required: `"chain"` \| `"current"` \| `"neighbors"` \| `"subgraph"` \| `"inverse"` \| `"filter"` \| `"path"`), `id` (chain, current, neighbors), `seed_ids` (subgraph), `from_id` / `to_id` (path), `category` (inverse, filter), `missing_edge_types` (inverse), `edge_types` (neighbors, subgraph, filter, path; absent or empty = all types excluding `Supersedes`), `direction` (chain: `"forward"` \| `"backward"` \| `"both"`; neighbors and subgraph: `"incoming"` \| `"outgoing"` \| `"both"` — on subgraph it filters which edges are traversed and returned, though every returned EdgeRecord still carries the canonical `"outgoing"` label), `depth` (neighbors default 1, range 1–10; path default 5, range 1–10), `max_depth` (subgraph default 3, range 1–10), `max_nodes` (subgraph, max 200), `limit` (inverse and filter, default 100, max 500), `resolve_supersessions` (neighbors, subgraph, path — default true; substitutes deprecated endpoints with their terminal active successors, pass `false` for the raw as-stored topology), `min_age_days`, `min_confidence`, `max_confidence`, `min_edge_count`, `max_edge_count` (filter mode property and edge-count constraints), `detail` (`"summary"` default \| `"full"`; verbosity — projected on node-bearing modes, accepted-and-ignored on neighbors/path), `format` (serialization — `"json"`; `"markdown"` rejected). |

**`context_search` vs `context_lookup`**: `context_search` uses semantic similarity (natural language). `context_lookup` uses exact filters (topic, category, tags, status). Use search when exploring; use lookup when you know what you want.

**`context_correct` vs `context_deprecate` vs `context_quarantine`**: `context_correct` supersedes with a new version (hash-chained). `context_deprecate` marks as outdated (no replacement). `context_quarantine` isolates from all results (Admin-only, reversible).

**`context_tag` vs `context_correct`**: both change an existing entry, but `context_tag` mutates only a tag in place — a fast path that preserves the entry's content, hash chain, edges, and learning vector — while `context_correct` supersedes the entry with a new hash-chained version and resets its learning signal. `context_tag` grants no new privilege: anything it can tag, `context_correct` can already tag. It is a cheaper route for volatile tags, not an access-control boundary on tags.

---

## Skills Reference

Unimatrix ships 10 Claude Code skills via the npm package. Skills are platform-native `/command` files installed automatically by `npx unimatrix init`.

Skills marked (MCP) require the server to be running and configured.

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/uni-init` | Initialize Unimatrix in a repository — CLAUDE.md setup + agent orientation recommendations. | First-time setup of a repo. |
| `/uni-seed` | Populate foundational knowledge through human-directed, gated exploration. (MCP) | After installation, before relying on search. |
| `/uni-retro` | Post-merge retrospective — extracts patterns, procedures, and lessons from shipped features. (MCP) | After a feature PR is merged. |
| `/uni-knowledge-search` | Semantic search across Unimatrix knowledge. (MCP) | Exploring a topic, finding related decisions or patterns. |
| `/uni-knowledge-lookup` | Deterministic lookup by feature, category, or entry ID. (MCP) | When you know what you want. |
| `/uni-query-patterns` | Query component patterns and conventions before designing or implementing. (MCP) | Before writing pseudocode or code. |
| `/uni-store-adr` | Store an architectural decision record. (MCP) | After each design decision. |
| `/uni-store-lesson` | Store a lesson learned from a failure or gate rejection. (MCP) | After bugfixes and unexpected issues. |
| `/uni-store-pattern` | Store a reusable implementation pattern. (MCP) | When a gotcha or reusable solution emerges. |
| `/uni-store-procedure` | Store or update a technical how-to procedure. (MCP) | When a technique evolves or is discovered. |

---

## Knowledge Categories

Unimatrix uses 5 built-in knowledge categories. Category discipline matters for retrieval quality — miscategorized entries surface in wrong contexts during semantic search.

| Category | Description | Example |
|----------|-------------|---------|
| `lesson-learned` | Lessons from failures, gate rejections, unexpected issues. | "Always verify hook latency after adding new UDS handlers — we hit 200ms in col-008." |
| `decision` | Architectural and design decisions (ADRs). | "Use SQLite for local storage — single-file, zero cloud dependency, bundled via rusqlite." |
| `convention` | Project conventions and rules agents should follow. | "All MCP tool handlers follow the execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit." |
| `pattern` | Reusable implementation patterns, gotchas, and solutions. | "Do not hold Store lock across async boundaries — use spawn_blocking for all Store calls." |
| `procedure` | Step-by-step technical procedures (how-to). | "How to add a new MCP tool: 1. Define params struct, 2. Implement handler, 3. Add validation, 4. Add audit event." |


The default category list can be replaced at startup via `[knowledge] categories` in `~/.unimatrix/config.toml`. The 5 built-in categories cover the primary use cases for software delivery; operators targeting other domains can supply a domain-appropriate list.

---

## CLI Reference

The `unimatrix` binary (or `npx unimatrix`) serves as both the MCP server and the hook handler.

### Default Mode (no subcommand)

Bridge mode. Connects to the running daemon's MCP socket and bridges stdin/stdout to it. If no daemon is running, auto-starts one (waits up to 5 seconds for the socket to appear) before bridging. This is what the MCP server configuration invokes — no change to `.mcp.json` is required.

### Subcommands

| Subcommand | Description | Key Flags |
|------------|-------------|-----------|
| `serve --daemon` | Start the MCP server as a detached background daemon. Daemonizes (fork/setsid), binds the MCP UDS socket (`unimatrix-mcp.sock`) and hook IPC socket, starts the background tick loop, and exits the launcher process. Activates the HTTPS listener when `[http] enabled = true`. Fails non-zero if a healthy daemon is already running. Linux and macOS only. | `--daemon` |
| `serve --foreground` | Start the MCP server in PID 1 foreground mode. Identical daemon functionality (UDS listener, HTTP listener when enabled, tick loop, ML inference) without fork/setsid. SIGTERM triggers graceful shutdown. Designed for container deployment where the main process must remain PID 1. Mutually exclusive with `--daemon` and `--stdio`. | `--foreground` |
| `serve --stdio` | Start the MCP server in foreground stdio mode. Identical in behavior to the pre-daemon default — the server runs until stdin closes, then performs graceful shutdown and exits. Use for development and testing. | `--stdio` |
| `stop` | Send SIGTERM to the running daemon and wait for it to exit (up to 10 seconds). Exits 0 on success, non-zero if no daemon is running or the PID file is absent/stale. | None |
| `hook <EVENT>` | Handle a lifecycle hook event from Claude Code, Gemini CLI, or Codex CLI. Reads JSON from stdin, connects to the running server via UDS. Provider-specific event names (e.g., Gemini's `BeforeTool`, `AfterTool`, `SessionEnd`) are normalized to canonical Unimatrix names at the ingest boundary. Designed for use in hook configuration files, not direct user invocation. | Event name as positional arg. `--provider <name>` (`claude-code` \| `gemini-cli` \| `codex-cli`) — required for Codex (shares event names with Claude Code); optional for Gemini (inferred from event name); omit for Claude Code (backward-compatible default). |
| `mcp-bridge <projectHash>` | Pure-JS stdio→HTTPS MCP bridge for cloud/remote-attached clients (routed to JS in `bin/unimatrix.js` — runs on hosts with no platform binary, e.g. macOS/Windows). Reads its credential (`mcp_url`, token, fingerprint) from the out-of-tree store `~/.unimatrix/<projectHash>/remote.json` at spawn time, opens a fingerprint-pinned TLS connection to the bundle's MCP URL, forwards `Authorization: Bearer <token>` only after the leaf fingerprint matches, and proxies newline-delimited stdio JSON-RPC to the cloud's Streamable-HTTP MCP endpoint (capturing and replaying `Mcp-Session-Id`). Wired automatically into `.mcp.json` by `init --bundle <blob>`; not for direct user invocation. Fails loud on certificate-pin mismatch. | `<projectHash>` (positional) |
| `health` | Check daemon liveness by connecting to the MCP UDS socket. Exit 0 when the daemon is running and responsive, exit 1 otherwise. 5-second timeout. No output on success; brief diagnostic on stderr on failure. Used by Docker HEALTHCHECK. | None |
| `client-bundle <slug>` | Emit a per-project connection bundle for attaching a remote client to this server over pinned TLS. Requires the registered project `<slug>` (there is no default-aliased bundle). Reads the data-volume token and the served leaf certificate, and prints a single-line `unimatrix-bundle:` (`v:2`) blob carrying the server-composed MCP and observe endpoint URLs (slug baked in), the token, and the `sha256:` cert-fingerprint. **stdout** is the opaque bundle blob only (pipeable); **stderr** echoes the decoded endpoint URLs and cert-fingerprint for the operator to eyeball, with the token redacted (never printed). Pre-tokio synchronous subcommand, like `health` and `version`. Consume the bundle on the client with `init --bundle <blob>`. | `<slug>` (positional), `--project-dir <PATH>` |
| `project register <slug>` | Register a project slug for serving. Creates the per-slug store (DB, vector index, hash chain, analytics) under `/data/.unimatrix/{slug}/` AND writes the `[[projects]]` routing intent into `config.toml` — no hand-edit required. A daemon restart applies the new routing intent, after which the slug is reachable at `/v1/{slug}/...`. The first project and the Nth project use this identical single command. Rejects a slug that fails the allowlist (`^[a-z0-9][a-z0-9-]{0,62}$`) or equals a reserved route segment. Errors loudly if the slug is already registered and routing. If the slug was previously de-registered but its data dir persists, register **re-attaches** to the preserved store rather than starting a new chain. | `<slug>` (positional) |
| `project list` | List the registered project slugs. | None |
| `project delete <slug>` | De-register a slug: removes it from `[[projects]]` routing while **preserving** its on-disk data (DB, vector index, hash chain, analytics). Re-registering the same slug re-attaches to the preserved store. To destroy the data as well, pass `--purge`, which requires re-typing the slug via `--confirm <slug>` — the only operation that destroys a hash chain, and it is deliberately loud. | `<slug>` (positional), `--purge`, `--confirm <slug>` |
| `export` | Export the knowledge base to JSONL format (format_version 2, 11 tables). No running server required. Prints a one-line count summary (entries exported, audit rows exported, resolved output path) to stderr on success. | `--output <PATH>` (defaults to stdout), `--slug <name>` (target the per-slug store `{base}/<slug>/unimatrix.db` — base derived from `--project-dir` — instead of the path-hash store; means "a store dir under the base," not a registered project; fails loud naming the resolved path if no store exists; see "Backup and restore a per-slug project"), `--skip-quarantined` (omit quarantined entries and their dependents from the export — requires `--confirm`), `--confirm` (acknowledge non-exact snapshot when `--skip-quarantined` is active) |
| `import` | Import a knowledge base from a JSONL export file (accepts format_version 1 and 2). Re-embeds entries and rebuilds vector index. | `--input <PATH>` (required), `--slug <name>` (restore into the per-slug store `{base}/<slug>/unimatrix.db` and rebuild its vector index — base derived from `--project-dir`; hard-errors with no `--force` override while a live daemon holds the store, and refuses a target whose audit log is non-empty; use the canonical `register → stop → import --slug → start` sequence in "Backup and restore a per-slug project"), `--skip-hash-validation`, `--force` (drop existing data including graph_edges, observations, cycle_events, and derived metric tables) |
| `verify` | Run the correction-chain integrity check against the live project database (direct read-only DB scan, no running server required — same access pattern as `export`/`import`). Walks all supersedes chains, recomputing each entry's `content_hash` and asserting every non-empty `previous_hash` equals its predecessor's `content_hash`; empty (genesis / forward-only-legacy) links are treated as unverifiable, not broken. Exit 0 with a concise summary (entries/chains checked) when the corpus verifies clean; non-zero exit naming the offending entry id(s) and the break type (content-hash vs chain-link mismatch) on any detected break. | None (uses the `--project-dir` global flag) |
| `version` | Print version and exit. With `--project-dir`, also initializes the database. | `--project-dir <PATH>` |
| `model-download` | Download the ONNX embedding model to cache. Used by npm postinstall; safe to run manually to pre-warm the model cache. | None |
| `snapshot` | Create a self-contained SQLite copy of the active database using `VACUUM INTO`. Includes all tables (entries, query_log, graph_edges, co_access, sessions, and all analytics tables). Refuses with a non-zero exit code if `--out` resolves to the same path as the live database. | `--out <PATH>` (required), `--project-dir <PATH>` |
| `eval scenarios` | Mine the `query_log` table from a snapshot and write eval scenarios in JSONL format. Each scenario includes query text, retrieval context, baseline result set (soft ground truth), and source path (`mcp` or `uds`). | `--db <PATH>` (required), `--out <PATH>` (required), `--retrieval-mode mcp\|uds\|all` (default `all`), `--limit <N>` |
| `eval run` | Replay eval scenarios through one or more configuration profile TOML files in-process, producing one JSON result file per scenario. Computes P@K, MRR, Kendall tau, rank change list, CC@k (Category Coverage at k), ICD (Intra-query Category Diversity), and latency delta per scenario per profile. Opens snapshot read-only; produces no writes to the snapshot. | `--db <PATH>` (required), `--scenarios <PATH>` (required), `--configs <TOML,...>` (required), `--out <DIR>` (required), `--k <N>` (default 5) |
| `eval report` | Aggregate per-scenario JSON result files into a Markdown report. Report contains: summary table (P@K, MRR, CC@k, ICD with delta columns), notable ranking changes, latency distribution, entry-level analysis, distribution analysis (CC@k range per profile and top improved/degraded scenarios by CC@k), and zero-regression check section. Human-reviewed artifact only — no automated pass/fail gate. | `--results <DIR>` (required), `--out <PATH>` (required), `--scenarios <PATH>` (optional, annotates queries) |

### Global Flags

| Flag | Description |
|------|-------------|
| `--project-dir <PATH>` | Override automatic project root detection. |
| `--verbose` / `-v` | Enable debug-level logging to stderr. |

---

## Architecture Overview

Unimatrix is a 9-crate Rust workspace that ships as a single binary.

### Storage

SQLite local database (`unimatrix.db`). 21 tables. Schema version 25. Zero cloud dependency — all data stays on your machine.

### Vector Search

384-dimension HNSW vector index (in-memory, persisted to disk). Dot product similarity.

### Embedding

Local ONNX model (all-MiniLM-L6-v2) via ONNX Runtime. No API calls. MicroLoRA adaptive layer tunes embeddings to project-specific usage.

### Hook Integration

Two hook clients. The single-binary `hook` subcommand communicates with the running MCP server via Unix domain socket (UDS) IPC, with a sub-50ms round-trip budget. The pure-JS, zero-dependency hook client in the `@dug-21/unimatrix` npm package (`lib/hook-client/`) selects its transport from configuration: with remote config present it POSTs `HookRequest` frames to the server-composed observe URL carried in the connection bundle over HTTP with Bearer auth (the client composes no path — it posts verbatim to the URL the server authored; no binary or ONNX model required); with no remote config it connects on Unix to the daemon's hook IPC socket (`unimatrix.sock`) over UDS, framing `HookRequest` messages byte-identically to the Rust hook (4-byte big-endian length-prefixed frames, 1 MiB cap). UDS local mode is Unix-only; Windows always uses the HTTP path. Across both transports the client streams transcript deltas on fire-and-forget events so the server's per-session buffer stays authoritative, is fail-open (exit 0 always), and keeps bounded client state (per-session `last_offset` plus a disk event queue) under `~/.unimatrix/{project-hash}/hook-client/`. Remote mode is configured via `init --remote` (see "Wire into your project").

### MCP Transport

Two transport surfaces: Unix Domain Socket (local) and HTTPS (network).

**UDS (local):** Daemon mode (default): Unimatrix runs as a persistent background daemon (`unimatrix serve --daemon`) that accepts MCP connections over a Unix Domain Socket (`unimatrix-mcp.sock`, 0600 permissions). Claude Code spawns a lightweight bridge process (the default `unimatrix` invocation) per session; the bridge connects stdin/stdout to the daemon's UDS socket. The daemon survives client disconnection — background tick, vector index, and all in-memory state persist across sessions. Up to 32 concurrent MCP sessions are supported.

**HTTPS (network):** When `[http] enabled = true` in config.toml, the server starts an HTTPS listener on the content port (default 8443). Any MCP-compatible client (Claude Code, Codex CLI, Gemini CLI) can connect over the network using a static bearer token for authentication. TLS termination via rustls is default-on when cert/key paths are configured; set `[tls] enabled = false` for reverse-proxy deployments. A path-dispatching tower service routes requests: `GET /health` (unauthenticated, returns version and schema version) and the per-slug routes under `/v1/{slug}/...`. Every served request carries a registered project slug: `/v1/{slug}/tools/...` for the MCP protocol and `/v1/{slug}/observe` for remote telemetry (content-negotiated: `Accept: text/plain` returns server-formatted injection text for injection-bearing responses, while `application/json` or no `Accept` header returns the JSON envelope as the default). Both MCP and observe resolve their store once per request through the same routing funnel — the server is the sole authority on route shape, and the slug is validated and resolved to a store at the routing edge before any store access, so cross-project mis-targeting is unrepresentable rather than merely rejected. There is no no-slug / default-project route: a request with no registered slug resolves no servable store. Up to 32 concurrent HTTP sessions (configurable). See [docs/client-setup.md](docs/client-setup.md) for per-client connection instructions.

Foreground mode (container): `unimatrix serve --foreground` runs the full daemon (UDS listener, HTTP listener when enabled, tick loop, ML inference) as PID 1 without fork/setsid. Used by the container `ENTRYPOINT`. SIGTERM triggers graceful shutdown.

Stdio mode (explicit): `unimatrix serve --stdio` starts the server in foreground stdio mode. Identical to the pre-daemon behavior; use for development and testing. HTTP listener does not activate in stdio mode.

The hook IPC socket (`unimatrix.sock`) and the MCP socket (`unimatrix-mcp.sock`) are separate files. Hook IPC uses the existing length-framed binary protocol; MCP sessions use newline-delimited JSON-RPC over the MCP socket or HTTPS.

### Data Layout

```
~/.unimatrix/
  config.toml                # Global config (optional — see Configuration section)
~/.unimatrix/{project-hash}/
  config.toml                # Per-project config override (optional)
  unimatrix.db               # SQLite knowledge database (schema v27)
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  unimatrix-mcp.sock         # Unix domain socket for MCP sessions (daemon mode)
  unimatrix.log              # Daemon stdout/stderr log (append mode)
  token                      # Bearer token for HTTPS auth (generated on first run, mode 0600)
  remote.json                # Out-of-tree remote-attach credential store (mode 0600; written by remote `init` — holds token, mcp_url, observe_url, fingerprint)
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # Index metadata
  hook-client/               # Remote HTTP hook client state (mode 0700; created by remote mode only)
    offsets/                 # Per-session transcript last_offset files
    queue/                   # Disk event queue for graceful degradation
    health.json              # Content-free health breadcrumb (no token, no transcript bytes)
~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

In a cloud/container deployment, each registered slug gets its own isolated subtree under the data volume — `/data/.unimatrix/{slug}/` holds that project's `unimatrix.db`, `vector/` index, hash chain, analytics, and an optional operator-placed per-slug `config.toml` (see Configuration), with no cross-project sharing. A container serves nothing until at least one slug is registered (single-project cloud is simply N=1). The local STDIO/UDS install is separate and unaffected: it keeps the path-hash layout above (`~/.unimatrix/{project-hash}/`) and requires no slug.

### Crate Workspace (9 crates)

| Crate | Responsibility |
|-------|---------------|
| `unimatrix-store` | SQLite storage engine — entries, indexes, audit log, migrations |
| `unimatrix-vector` | HNSW vector index — build, search, persist, compact |
| `unimatrix-embed` | ONNX embedding pipeline — model loading, tokenization, inference |
| `unimatrix-core` | Core traits, domain types, async wrappers, query filters |
| `unimatrix-engine` | Shared business logic — confidence scoring, project paths, search ranking |
| `unimatrix-adapt` | Adaptive embedding pipeline — MicroLoRA training, state persistence |
| `unimatrix-observe` | Observation pipeline — hotspot detection, metric computation, retrospective analysis |
| `unimatrix-learn` | Shared ML infrastructure — training reservoirs, EWC++ state, neural models, model versioning |
| `unimatrix` | MCP server — tool handlers, hook IPC, agent registry, audit, content scanning |

---

## Security Model

### HTTP Authentication

HTTPS transport uses static bearer token authentication. A 32-byte (256-bit) cryptographically random token is generated on first run, stored at `{data_volume}/token` (mode 0600), and validated on every HTTP request using constant-time comparison (`subtle::ConstantTimeEq`). The `/health` endpoint is the sole unauthenticated path. The `BearerValidator` trait enables enterprise extension (JWT/OAuth) without modifying the core auth path.

### Certificate Pinning

The OSS trust model for the self-signed serving certificate is **fingerprint pinning**, not CA trust. On first boot the server generates a self-signed cert+key and computes its fingerprint as `sha256:<lowercase-hex>` over the served leaf certificate's DER bytes. The `client-bundle` command carries that fingerprint in the connection bundle; the client pins the exact certificate by it (custom `checkServerIdentity` compare, no certificate-authority path), and the fingerprint is computed byte-identically on both stacks. A presented certificate that does not match is rejected with a clear, diagnosable mismatch error. Rotating the certificate therefore requires re-emitting the bundle (`client-bundle`) and re-running `init --bundle <blob>` on each client — see [docs/cert-rotation.md](docs/cert-rotation.md). CA-trust / SAN-based hostname validation is the enterprise/reverse-proxy path, not the OSS posture.

### Trust Hierarchy

Four-tier model: System > Privileged > Internal > Restricted. Unknown agents auto-enroll as Restricted on first contact (read + search only). `context_enroll` (Admin-only) promotes or modifies agent trust and capabilities. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability.

### Capabilities

Four capabilities gate tool access: `read`, `write`, `search`, `admin`.

### Project Scoping

Token authorizes, slug scopes data, certificate secures transport — three separate concerns. In OSS single-tenant serving the slug is a **knowledge-integrity boundary, not an access-control boundary**: project identity is taken from the transport (the URL-path slug, validated at the routing edge against `^[a-z0-9][a-z0-9-]{0,62}$` and the reserved set `v1`/`health`/`observe`/`tools`), never from the request payload, so an agent has no field in which to name another project and a wrong slug cannot escape `/data/.unimatrix/{slug}/`. Binding each client to exactly one project removes the ability to mis-target a write into another project's hash chain. The `BearerValidator` trait remains the seam where enterprise adds per-project JWT/RBAC authorization on top of this scoping.

### Content Scanning

Every write operation (`context_store`, `context_correct`) scans content for injection patterns (~25+ patterns including prompt injection, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected before storage.

### Transcript Handling

Streamed session transcript deltas (raw conversation bytes, possibly secret-bearing) accumulate in a per-session in-memory buffer only — they never reach disk, SQL, or logs. In-memory plus purge-on-close is the secrets guarantee: there is no content scanner for raw transcript. The buffer is bounded by `retention.transcript_buffer_max_bytes` (default 4 MiB; oldest bytes dropped on overflow) and purged at session close and the staleness sweep per `retention.transcript_retention` — cycle review no longer purges (`context_cycle_review` is fully non-destructive, with no purge verb), so reclamation is delegated entirely to session close, the TTL sweep, and held-buffer cap eviction. To keep multi-turn buffers alive across the per-turn drain so the cycle-review distillation path is non-empty, a drained buffer is moved into a bounded held-buffer store rather than freed; it continues merging deltas, is re-adopted on re-registration, and is reclaimed by the held-count cap (`transcript_hold_max_sessions`) or the TTL stale sweep (`transcript_hold_ttl_secs`). Purging a non-empty buffer emits a content-free `transcript_session_purged` audit event (session ID, byte count, timestamp — never content). The buffer has two readers: the server-side PreCompact transcript-tail block, and the cycle-review snapshot seam that feeds the `transcript_candidates` distillation. Distilled candidates are response-transient — selected blocks ride the cycle-review response only and are never written to SQL, files, logs, or the memoized cycle-review record. A server crash loses in-flight transcript by design.

A content-free fold runs alongside the buffer as deltas arrive: it accumulates only a running byte total, a delta count, and per-class behavioral-signature match counts (configured via `[transcript_signals]` — see Configuration). The fold is a scalar counter, never a query over the assembled transcript, and carries no content field, so no transcript bytes escape through it; the resulting class counts are directional, not precise. The held-buffer store (`transcript_hold_max_sessions`/`transcript_hold_ttl_secs`) is a verified startup precondition for this fold: it must be wired and enabled, and startup fails loud if it is not, because the fold must survive to cycle review.

### Audit Trail

Append-only audit log records every operation with agent identity (who performed the action), session context (which session, feature cycle), and operation outcome (success/failure). Each audit row carries four additional compliance columns: `credential_type` (how the caller authenticated — `"none"` for stdio, `"static_token"` for HTTP bearer, `"jwt"` for enterprise JWT), `capability_used` (the capability gate evaluated for the tool call), `agent_attribution` (transport-attested client identity sourced from `clientInfo.name` at the MCP `initialize` handshake — not the spoofable agent-declared `agent_id`), and `metadata` (a JSON object carrying `client_type` and extensible for future keys). Append-only enforcement is maintained by DDL triggers on the `audit_log` table: UPDATE and DELETE operations are rejected at the database level.

### Hash-Chained Corrections

Each correction links to the entry it supersedes via SHA-256 `content_hash` (the `previous_hash` chain), so the correction history is tamper-**recorded**: `unimatrix verify` and import-time validation recompute every entry's content hash and check each chain link, failing loud and naming any entry whose content or link is inconsistent. This detects accidental corruption and single-point API-surface tampering; it does not defend against a coordinated raw-database-write adversary (out of tier — that requires a cryptographic cascade and external anchor, tracked separately).

### Observation Ingest Constraints

Three hard limits apply to all observation events before any processing:

- **Payload size**: events with a payload exceeding 64 KB are rejected with a `PayloadTooLarge` error.
- **JSON nesting depth**: payloads nested more than 10 levels deep are rejected with a `NestingTooDeep` error.
- **Source domain format**: `source_domain` must match `^[a-z0-9_-]{1,64}$` at both domain pack registration and event ingest. Invalid values are rejected with an `InvalidSourceDomain` error — they do not silently coerce or pass through.

---

## Acknowledgments

Unimatrix's hook-driven delivery architecture draws directly from [ruvnet's](https://github.com/ruvnet) pioneering work on [claude-flow](https://github.com/ruvnet/claude-flow) (Ruflo) and ruvector. The core insight — that agent knowledge systems only deliver value when knowledge reaches agents automatically, without requiring explicit tool calls — shaped the entire Cortical Implant design. The adaptive embedding pipeline builds on patterns explored in ruvector's vector search architecture. We learned from both systems and are grateful for the open exploration that made this work possible.

---

## License

MIT OR Apache-2.0
