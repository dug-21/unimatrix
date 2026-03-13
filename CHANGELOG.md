# Changelog

All notable changes to Unimatrix are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/).

## [0.5.7] - 2026-03-13

### Fixes
- init: set LD_LIBRARY_PATH for binary invocations during init

## [0.5.6] - 2026-03-13

### Fixes
- CI: build arm64 on ubuntu-22.04 to target glibc 2.35

## [0.5.5] - 2026-03-13

### Fixes
- CI: switch npm packages to public access

## [0.5.4] - 2026-03-13

### Fixes
- CI: disable x64 build, arm64-only release (#247)

## [0.5.3] - 2026-03-13

### Fixes
- CI: add test retry for transient CI failures (#247)

## [0.5.2] - 2026-03-13

### Fixes
- CI: download embedding model before tests in release pipeline (#245)

## [0.5.1] - 2026-03-13

### Fixes
- CI: install ORT on CI runner and add linux-arm64 build (#243)

## [0.5.0] - 2026-03-13

### Features
- Quarantine state restoration — schema v7→v8, multi-status quarantine, restore to pre-quarantine status (#142)
- col-011 knowledge architecture — specialized coordinators, skills, and feedback loop
- col-010b — evidence synthesis & lesson-learned persistence (re-delivery) (#78)
- col-010 P0 — session lifecycle persistence + injection log (#77)
- col-002 retrospective pipeline (#58)
- crt-006 adaptive embedding pipeline with integration tests (#49)
- crt-005 coherence gate — f64 scoring, lambda metric, maintenance actions
- crt-001 usage tracking implementation (#25)
- nxs-003 embedding pipeline — unimatrix-embed crate (#5)
- nxs-002 vector index — unimatrix-vector crate (#2)

### Fixes
- Server: resolve ghost process, tick contention, and handler timeouts (#236)
- Server: add agent_id to CycleParams so context_cycle resolves caller identity (#230)
- Registry: permissive auto-enroll grants Write to unknown agents (#228)
- Session: resolve feature_cycle attribution gaps (#198)
- Context_status deadlock + async blocking store calls (#176)
- Content-based attribution fallback for retrospective (#162)
- Batch spawn_blocking DB writes to prevent blocking pool saturation (#158)
- Resolve deadlock in scan_sessions_by_feature_with_status (#152)
- Server recovery — PidGuard race, db retry, transport logging (#146)
- Embed model retry on failure + abort tick handle on shutdown (#120)
- Init order — run migration before create_tables for v5 databases (#104)
- Align SQLite Store API with redb signatures (#95)
- Drop ServiceLayer in shutdown to release Arc<Store> refs (#92)
- Stdin size limit and data directory permissions hardening
- PID file mechanism and retry loop for stale DB lock (#23)
