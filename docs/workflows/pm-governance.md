# PM Governance — How the Whole System Fits Together

This diagram shows the human-driven governance layer that wraps the automated swarm sessions. Understanding this layer explains how the project stays on-vision, how workflow problems get fixed, and why protocol changes are intentionally slow.

---

## The Governance Loop

```mermaid
flowchart TD
    UNIZERO(["/uni-zero — Vision Guide Session\nHuman-driven, conversational\nRuns before new features begin"])

    subgraph UNIZERO_DOES [What uni-zero does]
        UZ1[Maintains PRODUCT-VISION.md]
        UZ2[Maintains Unimatrix vision entries\nso agents always brief from current vision]
        UZ3[Reviews roadmap position\nand feature ordering]
        UZ4[Scopes research spikes\nfor unknown questions]
        UZ5[Creates GitHub Issues\nfor approved work items]
    end

    UNIZERO --> UNIZERO_DOES
    UNIZERO_DOES --> SCOPECHECK{Is the feature\nscoped and approved?}
    SCOPECHECK -->|No — explore more| UNIZERO
    SCOPECHECK -->|Yes — SCOPE.md exists\nand human approves| DESIGN

    DESIGN[Design Session\nSee design-session.md\nProduces 8 artifacts + GitHub Issue]
    DESIGN --> HUMANREVIEW{Human reviews\ndesign artifacts\nand alignment report}
    HUMANREVIEW -->|Variances require changes| UNIZERO
    HUMANREVIEW -->|Approved — proceed| DELIVERY

    DELIVERY[Delivery Session\nSee delivery-session.md\nImplements, tests, opens PR]
    DELIVERY --> PRREVIEW{Human reviews PR\nand security findings}
    PRREVIEW -->|Blocking findings| FIX[Address blocking items\nvia bugfix protocol]
    FIX --> PRREVIEW
    PRREVIEW -->|Approved| MERGE[Merge PR\ngh pr merge --squash --delete-branch]

    MERGE --> RETRO

    subgraph RETRO ["/uni-retro — Post-Merge Retrospective\nHuman-triggered after merge"]
        R1[Runs context_cycle_review\nfor the shipped feature]
        R2[Spawns architect to extract\npatterns, lessons, procedures]
        R3[Stores reusable knowledge\nin Unimatrix]
        R4[Shows human: hotspots, outliers,\nrecommendations from session data]
        R1 --> R2 --> R3 --> R4
    end

    RETRO --> HUMANRETRO{Human reviews\nretro findings}
    HUMANRETRO -->|No protocol changes needed| NEXTFEATURE([Next feature — back to uni-zero])
    HUMANRETRO -->|Protocol improvement identified| PROTOUPDATE

    subgraph PROTOUPDATE [Protocol Update — Intentionally Slow]
        PU1[Human reads retro recommendation]
        PU2[Human evaluates: is this a real\nworkflow problem or a one-off?]
        PU3[Human drafts protocol change]
        PU4[Human reviews change\nagainst other protocol steps\nfor consistency]
        PU5[Human commits change\nto .claude/protocols/uni/]
        PU1 --> PU2 --> PU3 --> PU4 --> PU5
    end

    PROTOUPDATE --> NEXTFEATURE

    style UNIZERO fill:#e8f4f8
    style RETRO fill:#e8f4f8
    style HUMANREVIEW fill:#f8d7da
    style PRREVIEW fill:#f8d7da
    style HUMANRETRO fill:#f8d7da
    style PROTOUPDATE fill:#fff3cd
    style SCOPECHECK fill:#f8d7da
```

---

## Why Each Step Is Human-Driven

| Step | Why human, not automated |
|------|--------------------------|
| Vision alignment (uni-zero) | Product direction requires judgment — the system cannot decide what to build |
| Design artifact review | Variances in the alignment report may require scope changes, not just acknowledgment |
| PR review and merge | Security findings may require human judgment on acceptable risk |
| Retro review | Hotspot data requires context: is a pattern a real problem or a measurement artifact? |
| Protocol updates | Changing the protocols changes how all future sessions run — this requires deliberate human approval, not automated feedback loops |

---

## What uni-zero Does NOT Do

- Does not modify code in `crates/`
- Does not run Design, Delivery, or Bugfix protocols
- Does not create feature implementation artifacts (IMPLEMENTATION-BRIEF, ARCHITECTURE.md, etc.)
- Does not commit or push code
- Does not store ADRs, patterns, or lessons in Unimatrix (those belong to delivery and retro sessions)

uni-zero is purely strategic: vision, roadmap, scope, research spike initiation, and GitHub Issue creation.

---

## What uni-retro Does NOT Do

- Does not change protocols automatically based on hotspot recommendations
- Does not merge PRs
- Does not create new features or GitHub Issues
- Does not supersede ADRs without human approval

uni-retro extracts knowledge from shipped work and presents findings to the human. The human decides what to act on.

---

## Why Protocol Changes Are Intentionally Slow

Protocols define how every future swarm session runs. An automated feedback loop from retro findings to protocol edits would mean one bad session could corrupt the workflow for all subsequent sessions. Protocol changes require the human to:

1. Read the recommendation and validate it against multiple sessions (not just one)
2. Trace the change through the protocol to check for side effects
3. Confirm the change doesn't break gate dependencies or agent role boundaries

This is not a limitation — it is the control mechanism that keeps the swarm system trustworthy.
