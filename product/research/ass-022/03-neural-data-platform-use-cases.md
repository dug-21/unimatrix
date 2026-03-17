# ASS-022/03: Unimatrix × Neural Data Platform — Novel Use Cases

**Date**: 2026-03-16
**Type**: Innovation / use case theorization
**Reference**: github.com/dug-21/neural-data-platform

---

## 1. What the Neural Data Platform Is

The Neural Data Platform (NDP) is a configuration-driven, time-series data system built in Rust for edge deployment. It solves the problem of ingesting, cleaning, and making queryable the raw output of environmental sensors. Its architecture:

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  Data Sources│    │   Bronze     │    │   Silver     │    │    Gold      │
│              │───►│  (Parquet)   │───►│(TimescaleDB) │───►│(ML Features) │
│ - MQTT       │    │  Immutable   │    │  Typed &     │    │  (future)    │
│ - HTTP APIs  │    │  Raw Data    │    │  Cleaned     │    │              │
│ - OpenWeather│    │  + WAL       │    │  Hypertables │    │              │
│ - NWS Fcst   │    │              │    │  + DQ rules  │    │              │
└──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
                                                │
                                        15 MCP Tools ──► AI Agents
                                        Grafana Dashboards
```

Current deployments monitor: PM2.5, PM10, CO2, temperature, humidity (indoor/outdoor), barometric pressure, and 7-day weather forecasts.

**The gap NDP does not fill**: NDP is excellent at *what the sensors report*. It does not manage *what those readings mean* — the interpretations, thresholds, anomaly patterns, calibration histories, and source attributions that turn raw numbers into actionable environmental intelligence.

That gap is exactly where Unimatrix belongs.

---

## 2. The Conceptual Split: Data vs. Knowledge

The most important framing for this integration:

```
NDP answers: "What did sensor 7 report at 14:32 yesterday?"
Unimatrix answers: "When sensor 7 has reported similar patterns historically,
                   what did they mean, and what was done about them?"
```

| Layer | Managed By | Examples |
|-------|-----------|---------|
| Raw time-series data | NDP (Bronze → Silver) | PM2.5 = 47.3 µg/m³ at 2026-03-16T14:32:00Z |
| Quality-checked data | NDP (Silver DQ rules) | PM2.5 within valid range, freshness OK |
| ML features | NDP (Gold, future) | 24h rolling mean, hourly delta, spectral peaks |
| **Interpretive knowledge** | **Unimatrix** | "This spike pattern historically correlates with agricultural burning events 40km west during Santa Ana conditions" |
| **Regulatory knowledge** | **Unimatrix** | "EPA NAAQS 24h PM2.5 standard is 35 µg/m³; this reading triggers AQI 'Unhealthy for Sensitive Groups'" |
| **Calibration history** | **Unimatrix** | "Sensor 7 was recalibrated on 2026-02-12 after a 15% drift detected; correction applied to readings after that date" |
| **Anomaly findings** | **Unimatrix** | "The February 14 spike was attributed to controlled burn permit #2026-CB-0443" |
| **Source attribution** | **Unimatrix** | "Nearby freeway I-405 contributes baseline NOx of ~8 ppb; isolate before attributing to other sources" |

---

## 3. Use Case 1: Environmental Pattern Memory

### The Problem

An AI agent analyzing current air quality has NDP's 15 MCP tools to query historical data. It can ask "what was PM2.5 yesterday?" and get an answer. But it cannot ask "has this pattern happened before, what caused it, and what should we do?"

Every similar pollution event has to be re-diagnosed from scratch because the *knowledge about that event* lives nowhere. It might be in an analyst's email, a PDF report, a Slack message — or it is simply lost.

### The Solution Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      AI Environmental Agent                      │
└──────────┬──────────────────────────────────────┬───────────────┘
           │                                      │
    NDP MCP Tools (15)                   Unimatrix MCP Tools (12)
    "What happened?"                     "What does it mean?"
           │                                      │
    ┌──────▼──────┐                    ┌───────────▼──────────┐
    │     NDP     │                    │      Unimatrix       │
    │ TimescaleDB │                    │   Knowledge Engine   │
    │             │                    │                      │
    │ Raw readings│                    │ - Anomaly patterns   │
    │ DQ flags    │                    │ - Source attributions│
    │ Forecasts   │                    │ - Regulatory context │
    │ Aggregates  │                    │ - Calibration history│
    └─────────────┘                    │ - Corrected findings │
                                       │ - Expert knowledge   │
                                       └──────────────────────┘
```

### The Unimatrix Domain Pack for Environmental Monitoring

**Categories**:
- `anomaly-pattern` — Documented pollution events and their signatures
- `source-attribution` — Known pollution sources, their typical fingerprints, and seasonal patterns
- `regulatory-threshold` — EPA/AQMD standards, permit conditions, reporting requirements
- `calibration-record` — Sensor calibration events, drift corrections, methodology notes
- `seasonal-baseline` — Established baseline concentrations by season/location
- `health-advisory` — Health guidance at different AQI levels; sensitive population considerations
- `sensor-profile` — Sensor hardware characteristics, known failure modes, maintenance history
- `event-finding` — Post-event analysis documents linking specific dates to causes

**Topic structure** (geographic + pollutant):
- `location:grid-A1` / `location:downtown-LA` / `location:sensor-7`
- `pollutant:pm25` / `pollutant:nox` / `pollutant:co2`
- `source:agricultural-burn` / `source:traffic` / `source:wildfire`

**Trust levels** mapped to data authority:
- `System` → EPA federal reference monitors, CARB certified stations
- `Privileged` → AQMD district-certified sensors, accredited labs
- `Internal` → Community sensor networks (calibrated), research deployments
- `Restricted` → Uncertified low-cost sensors, citizen science data

**The `feature_cycle` field** → maps to `monitoring-period` or `pollution-event-id`
(e.g., "event:wildfire-2026-02" or "season:winter-2026")

**Freshness half-life** — this is the critical config difference:
- Regulatory thresholds: 1–2 year half-life (NAAQS standards change rarely)
- Seasonal baselines: 90-day half-life (relevant to the same season each year)
- Source attributions: 30-day half-life (sources change with conditions)
- Anomaly patterns: 14-day half-life (recent patterns most diagnostic)
- Calibration records: 7-day half-life (very recent calibration is most relevant)
- Health advisories: 180-day half-life (guidance updates infrequently)

A single Unimatrix instance cannot currently support multiple freshness half-lives per category. This is a feature gap and a strong argument for making freshness a per-entry or per-category configurable parameter rather than a global constant.

### The Correction Chain Story

Consider a calibration event:

1. **Initial finding** (trust_source="internal"):
   > "Sensor 7 PM2.5 readings elevated ~15% above collocated reference monitor. Source: unknown."

2. **Correction** (trust_source="privileged"):
   > "Root cause identified: filter loading artifact. Sensor 7 corrected with -12% scaling factor from 2026-02-12T00:00Z forward. Previous readings require retrospective correction in NDP Silver layer."

3. **Retrospective correction** (trust_source="system"):
   > "NDP Silver layer recorrected for Sensor 7 from 2026-01-01 to 2026-02-12. Recalibration confirmed by collocated EPA reference. Confidence in Sensor 7 data fully restored."

Each of these creates a correction chain in Unimatrix. Any AI agent querying for "Sensor 7 reliability" gets the full history, not just the current state. The SHA-256 chain provides tamper-evident provenance — important if the data is used for regulatory reporting or legal proceedings.

---

## 4. Use Case 2: Semantic Pattern Similarity — The Non-Text Embedding Idea

This is the most technically innovative possibility: **using Unimatrix's vector index for pollution pattern fingerprinting rather than text similarity**.

### The Concept

Instead of embedding text descriptions of anomaly events, embed *the pollution signatures themselves* as vectors. When a new anomaly occurs, the agent queries Unimatrix not with a text query but with a **sensor reading vector** — and finds historically similar pollution patterns.

### How It Would Work

**Step 1: Feature extraction from NDP**

NDP's Gold layer (currently future, but conceptually defined) produces ML features from TimescaleDB aggregates:

```
For a 24h pollution window centered on an anomaly peak:
- Mean, std, min, max for each pollutant (PM2.5, PM10, NO2, CO, O3)
- Time of peak, duration above threshold, rise rate, decay rate
- Wind speed/direction during peak (from NWS data)
- Temp, humidity, pressure during peak
- Day-of-week, season, distance from source categories
→ ~60-100 dimensional feature vector, normalized
```

**Step 2: Project to 384-dim for HNSW**

A lightweight projection layer (linear + normalization) maps the ~80-dim feature vector to 384 dimensions — matching Unimatrix's current HNSW index. This projection can be trained on historical events where source attribution is known (supervised) or learned from co-occurring patterns (unsupervised).

**Step 3: Store the fingerprinted event in Unimatrix**

```
context_store(
  title: "PM2.5 spike 2026-03-15 14:32 Grid-A1",
  content: "Peak 87 µg/m³ at 14:32, sustained 3.2h, wind from SW 340°.
            Confirmed source: agricultural burn permit #2026-CB-0557,
            Kern County. AQI reached 158 (Unhealthy).
            Sensitive groups advised to remain indoors.",
  category: "anomaly-pattern",
  topic: "location:grid-A1",
  tags: ["agricultural-burn", "pm25", "santa-ana"],
  // The embedding is computed NOT from the text but from the sensor feature vector
)
```

**Step 4: Query at the next anomaly**

When a new PM2.5 spike occurs at Grid-A1, extract the same feature vector from NDP's current readings, project to 384-dim, and run `context_search`. The HNSW index returns entries whose *sensor signatures* are most similar — regardless of the text description.

The result: "This current spike most closely resembles the 2026-03-15 agricultural burn event, the 2025-11-03 wildfire smoke event, and the 2025-08-22 Mojave dust event — with the agricultural burn being 0.91 cosine similar. Here are the findings from each."

### Why This Matters

The agent now has **pattern-matched historical knowledge** without anyone having to write the right keywords. A text search for "high PM2.5 afternoon westerly winds" might miss an event described as "pollution spike post-noon with Santa Ana conditions." The sensor fingerprint doesn't have this ambiguity problem.

### What Needs to Change in Unimatrix

- The embedding pipeline currently only handles text (ONNX + tokenizers). To support feature vector embeddings, the `EmbedService` trait needs an alternative implementation that accepts `Vec<f32>` directly (no tokenization needed).
- The `EmbedAdapter` currently concatenates `title + ": " + content`. A `VectorEmbedAdapter` would accept pre-computed vectors and skip the text embedding step.
- The `EmbedConfig` model selection would need a new option: `"passthrough"` — for domains where the caller provides their own embeddings.

This is a relatively small change — the `VectorStore` trait is already indifferent to what's embedded. The vector index stores f32 vectors of any dimension. The embedding pipeline is the only assumption.

---

## 5. Use Case 3: Sensor Network Knowledge Inheritance

### The Problem of Sensor Fleet Management

Environmental monitoring networks have dozens to hundreds of sensors with individual personalities: calibration drift histories, known failure modes, seasonal biases, collocation agreements with reference monitors. This knowledge is almost never systematically managed. It lives in spreadsheets, in the memory of the field technician who installed them, or nowhere.

### Unimatrix as Sensor Fleet Memory

Each sensor becomes an agent in the Unimatrix trust model:

```
Agent Registry:
  sensor-007  → trust: Internal, capabilities: [Write]
  sensor-012  → trust: Restricted, capabilities: [Write]  // uncalibrated
  epa-ref-003 → trust: Privileged, capabilities: [Write, Search]
  analyst-rk  → trust: Privileged, capabilities: [Read, Write, Search, Admin]
  system      → trust: System, capabilities: [Admin]
```

Sensor 7 doesn't literally write to Unimatrix — a software agent acting on its behalf does. When sensor 7 reports an anomaly, the agent stores a knowledge entry attributed to `created_by: "sensor-007"` with `trust_source: "internal"`. When a field technician reviews and confirms the reading, they correct it with `trust_source: "privileged"`. When the EPA reference monitor validates it, the trust escalates to `trust_source: "system"`.

This creates a **data provenance chain** that directly answers regulatory questions: "What is the reliability of this reading, and who has validated it?"

The confidence scoring then reflects real data quality:
- Uncalibrated sensor reading: base confidence ~0.35 (trust_source="restricted" auto-derived)
- Field-validated reading: base confidence ~0.50 (trust_source="internal")
- Collocated reference-confirmed: base confidence ~0.70 (trust_source="privileged")
- EPA method validated: base confidence ~0.90 (trust_source="system")

### Sensor Profile Knowledge

Each sensor's known quirks become searchable knowledge entries:

```
Category: sensor-profile
Topic: sensor-007
Content: "SPS30 optical PM sensor, installed 2025-04-12. Known behavior:
         reads 8-12% high during high humidity (>80% RH) due to hygroscopic
         particle growth. Apply -10% correction when RH > 80%. Last collocated
         with EPA FRM on 2026-01-15, confirmed ±5% accuracy at RH < 75%."
Trust: privileged (field technician documented)
```

When the agent is processing a high-humidity event, `context_briefing` with `role="data-analyst"` and `task="PM2.5 anomaly investigation sensor-007"` returns this sensor profile automatically. The analyst doesn't need to remember where the calibration notes live.

---

## 6. Use Case 4: Regulatory Compliance Memory

### The Problem

Environmental permits are complex, multi-condition documents. Permit conditions change. Enforcement history matters. The relationship between monitoring data and regulatory thresholds requires contextual knowledge that data alone cannot provide.

### Unimatrix as Regulatory Context Engine

```
Category: regulatory-threshold
Topic: pollutant:pm25
Title: "EPA NAAQS PM2.5 24h Standard"
Content: "Primary standard: 35 µg/m³ (24h average). Secondary standard: 35 µg/m³.
         Attainment based on 3-year average of annual 98th percentile 24h values.
         Revised 2024-02-07 (lowered from 65 µg/m³ set in 1997).
         AQI breakpoints: Good 0-12, Moderate 12.1-35.4, USG 35.5-55.4,
         Unhealthy 55.5-150.4, Very Unhealthy 150.5-250.4, Hazardous 250.5+"
Trust: system
```

The *correction chain* captures regulatory evolution:
- 1997 original standard: 65 µg/m³ entry
- 2006 revision: 35 µg/m³ corrects 1997 entry (with `supersedes` link)
- 2024 revision: annual standard lowered to 9 µg/m³ (corrects 2006 for annual metric)

An agent checking historical compliance can traverse the correction chain: "What standard applied on 2025-03-01?" and get the right answer for that date, not the current standard.

This is genuinely impossible with a simple key-value store or a wiki. It requires the correction chain model that Unimatrix provides.

### Health Advisory Knowledge

```
Category: health-advisory
Topic: pollutant:pm25
Tags: [sensitive-groups, outdoor-activity]
Content: "At AQI 101-150 (Unhealthy for Sensitive Groups):
         Reduce prolonged or heavy outdoor exertion.
         Watch for symptoms (coughing, shortness of breath, chest tightness).
         Children, elderly, and those with lung/heart disease should
         limit prolonged outdoor exposure.
         Source: AirNow.gov, EPA guidance 2024."
```

When the NDP monitoring agent detects AQI > 100, `context_briefing` automatically surfaces this advisory for the agent's response — without the agent needing to know what AQI level it's looking at.

---

## 7. Use Case 5: Cross-Domain Environmental Intelligence

### The Integration Architecture

The most powerful configuration combines NDP's data capabilities with Unimatrix's knowledge capabilities into a unified intelligence layer:

```
              ┌─────────────────────────────────────────┐
              │         Environmental AI Agent           │
              └───────────┬────────────────┬────────────┘
                          │                │
               ┌──────────▼──────┐  ┌──────▼──────────┐
               │   NDP MCP       │  │  Unimatrix MCP   │
               │   (15 tools)    │  │  (12 tools)      │
               └──────────┬──────┘  └──────┬───────────┘
                          │                │
          ┌───────────────▼──┐         ┌───▼──────────────────┐
          │   NDP Data Layer │         │  Unimatrix Knowledge  │
          │                  │         │                        │
          │ Bronze (Parquet)◄│─────────│► Source Attribution   │
          │ Silver (TSDB)    │         │  Anomaly Patterns      │
          │ Gold (Features)  │─────────►  Sensor Profiles       │
          │                  │         │  Regulatory Context     │
          │ Real-time streams│         │  Calibration History    │
          └──────────────────┘         └────────────────────────┘
```

**Concrete agent workflow** (no human in the loop):

1. NDP Silver layer detects PM2.5 > 55 µg/m³ at Grid-A1 (triggers a DQ quality alert)
2. Monitoring agent queries Unimatrix: `context_search("PM2.5 spike Grid-A1", category="anomaly-pattern")`
3. Unimatrix returns top-3 similar historical events with confidence scores and correction chains
4. Agent queries NDP for current meteorological conditions (wind speed/direction, humidity)
5. Agent queries Unimatrix: `context_lookup(topic="source:agricultural-burn", tags=["march", "southwest-wind"])`
6. Unimatrix returns known agricultural burn patterns for this wind condition/season
7. Agent assembles hypothesis: "likely agricultural burn, 85% similar to 2026-03-15 event"
8. Agent stores new entry: `context_store(category="anomaly-pattern", content="PM2.5 spike 2026-03-16...")`
9. Agent queries regulatory context: `context_search("PM2.5 55 µg/m³ AQI threshold health advisory")`
10. Agent generates alert with health guidance and probable source attribution

**Zero human intervention.** Every step is audited. Every piece of knowledge has a trust level. Every finding becomes a new knowledge entry that improves future responses.

---

## 8. Use Case 6: Long-Term Environmental Pattern Learning

### Why This Is Genuinely Novel

The neural-data-platform stores 6 months of sensor data. NDP's TimescaleDB can answer "what was PM2.5 on this date?" for any date in the retention window. But NDP cannot answer "what have we learned about PM2.5 patterns in this region over the past 3 years?"

Unimatrix's confidence evolution addresses this. Anomaly pattern entries that keep getting retrieved (because they match new events) accumulate `access_count` and gain confidence. Entries that stop being retrieved decay in freshness. Over years, a corpus of validated, high-confidence pattern knowledge emerges — not because someone curated it, but because the system learns what is useful.

**The knowledge base becomes a regional environmental memory** that survives sensor replacements, staff turnover, and data retention window limits.

### Contradiction Detection in Environmental Context

Two analysts might attribute the same pollution event to different sources. Unimatrix's contradiction detection surfaces this:

- Entry A (trust: internal): "The March 2026 PM2.5 spike was caused by agricultural burning in Kern County"
- Entry B (trust: privileged): "Chemical fingerprinting via ICP-MS indicates the March 2026 PM2.5 spike was dominated by traffic exhaust, not biomass burning"

Contradiction detected. The system flags both entries, reduces confidence on the lower-trust entry (A), and surfaces the conflict to a human reviewer. The reviewer corrects one entry. The correction chain preserves both the original hypothesis and its refutation — important for scientific reproducibility and regulatory defensibility.

---

## 9. What NDP Would Need from Unimatrix

| NDP Need | Unimatrix Feature | Gap? |
|----------|------------------|------|
| Store sensor anomaly findings | `context_store` (category: anomaly-pattern) | None |
| Retrieve similar historical patterns | `context_search` with sensor-fingerprint embeddings | Embedding pipeline needs passthrough mode |
| Track sensor calibration history | Correction chains | None — maps perfectly |
| Access regulatory thresholds | `context_lookup` (category: regulatory-threshold) | None |
| Sensor trust levels | Agent registry + trust_source | None |
| Multi-freshness-rate by category | Configurable per-category freshness | **Gap** — currently global constant |
| Automated finding storage from anomaly alerts | Hook-driven storage | NDP would need to call Unimatrix MCP on alert trigger |
| Evidence chain for regulatory reporting | Audit log + hash chain | None |
| Conflict resolution between analysts | Contradiction detection + context_correct | None |

The only structural gap for the NDP integration is **configurable per-category freshness decay** and **passthrough embedding mode** (pre-computed feature vectors). Both are localized changes to the confidence and embedding subsystems.

---

## 10. The Broader Vision

The neural-data-platform is one instance of a broader pattern: **any system that generates structured time-series data also generates knowledge** — and that knowledge needs lifecycle management.

| Domain | Data Platform (NDP-equivalent) | Unimatrix Knowledge Layer |
|--------|-------------------------------|--------------------------|
| Environmental monitoring | NDP + TimescaleDB | Anomaly patterns, regulatory context, calibration history |
| Industrial IoT / manufacturing | SCADA + historian DB | Failure modes, maintenance procedures, process parameters |
| Network operations | Network telemetry + InfluxDB | Incident patterns, runbooks, configuration change impacts |
| Medical devices / patient monitoring | Medical time-series DB | Clinical thresholds, device calibration records, care protocols |
| Financial markets | Market data feeds + tick DB | Trading pattern analysis, regulatory reporting history, model validation |
| Genomics / bio | Sequence databases | Variant interpretations, pathway annotations, protocol evolution |
| Climate science | Climate model output + NetCDF | Attribution studies, model validation findings, methodology evolution |

In every case, the pattern is the same: a high-volume data layer captures *what happened*, and Unimatrix manages *what it means* — with corrections, trust attribution, and confidence evolution over time.

The NDP integration is the existence proof of this pattern. Building it would validate Unimatrix's domain-agnosticism claim more concretely than any theoretical analysis.

---

## 11. Recommended Next Steps

If pursuing the NDP integration as a proof of concept:

1. **Create an environmental domain pack** (categories.toml, instructions.md, agents.toml) — 1 day of work after vnc-004 (config externalization) ships.

2. **Add `passthrough` embedding mode** to `EmbedConfig` — allows callers to supply pre-computed vectors, bypassing ONNX + tokenizer. Enables sensor fingerprint embeddings. Estimated: 1 day.

3. **Make freshness configurable per entry or per category** — the most important gap for environmental domains. `freshness_half_life_hours: Option<f64>` on EntryRecord (falls back to global config). Estimated: 2-3 days (schema migration + confidence recomputation).

4. **Build a prototype NDP → Unimatrix bridge** — a small Rust process that subscribes to NDP's anomaly alerts (via webhook or polling) and calls `context_store` via MCP. Demonstrates the integration pattern without modifying either system. Estimated: 2-3 days.

5. **Run the pattern similarity experiment** — take 6 months of NDP historical data, extract pollution fingerprints, project to 384-dim, store in Unimatrix. Then replay a known anomaly event and verify that HNSW retrieval surfaces the right historical matches. This is the empirical test of the sensor fingerprint embedding hypothesis. Estimated: 1 week including feature engineering.

These five steps would produce a working environmental knowledge engine demo built on the exact same Unimatrix binary as the software development use case, with only configuration differences. That is the domain-agnosticism proof of concept.
