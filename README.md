# Heimdall

A smart Solana transaction infrastructure stack that observes the network in
real time, submits transactions intelligently via Jito bundles, tracks outcomes
across commitment levels, and uses a two-tier AI agent to autonomously detect
failures and decide retry strategies with exponential backoff.

Built for the Superteam Earn bounty: **Smart Transaction Infrastructure Stack**.

---

## Project Resources

| Resource                            | URL                                                  |
| ----------------------------------- | ---------------------------------------------------- |
| Documentation Site                  | [heimdall-rosy.vercel.app](heimdall-rosy.vercel.app) |
| Architecture Document               | [ARCHITECTURE.md](./ARCHITECTURE.md)                 |
| Operational Evidence Report         | [EVIDENCE.md](./EVIDENCE.md)                         |
| Live Monitoring (when running)      | `http://localhost:3000`                              |
| Evidence Export (when running)      | `GET http://localhost:3000/evidence`                 |
| SSE Real-Time Stream (when running) | `GET http://localhost:3000/events`                   |

---

## System Overview

Heimdall is a 7-layer transaction infrastructure stack split across two runtimes:

- **Core (Rust)** — L1 through L5: network observation, transaction intelligence,
  bundle submission, confirmation tracking, and operational state
- **Services (TypeScript/Bun)** — L6 and L7: AI decision agent and monitoring

```
L1 — Network Observation      Yellowstone gRPC slot stream + transaction signature watcher
L2 — Transaction Intelligence Candidate deduplication and channel to L3
L3 — Transaction Submission   Jito bundle construction, dynamic tips, exponential backoff retry
L4 — Confirmation Tracking    Lifecycle tracking, three-field failure classification
L5 — Operational State Engine gRPC server streaming snapshots to L6/L7
L6 — AI Decision Layer        Two-tier pipeline: Local Rules → Gemini LLM (with fallback)
L7 — Monitoring & Control     HTTP API + SSE stream + evidence export
```

---

## Architecture

Full C4 architecture diagrams (Context, Container, Component), channel topology,
failure handling strategy, and infrastructure decisions are available in
[ARCHITECTURE.md](./ARCHITECTURE.md).

> **Bounty requirement**: Publish `ARCHITECTURE.md` to a public Notion page,
> Google Doc, or Figma URL and link it here before submission.

**Inter-layer communication:**

- L1 → L2: `tokio::sync::mpsc` channel carrying `NetworkEvent`
- L2 → L3: `tokio::sync::mpsc` channel carrying `TransactionCandidate`
- L3 → L4: `tokio::sync::mpsc` channel carrying `SubmissionRecord`
- L1 → L4: `tokio::sync::mpsc` channel carrying `SlotConfirmation` (slot + optional signature)
- L5 → L6: gRPC server-side streaming (`OperationalSnapshot` every 500ms)
- L5 → L7: gRPC server-side streaming (same subscription)
- L6 → L5: gRPC unary RPC (`RetryRequest` → `RetryResponse`)
- L6 → L7: HTTP POST `/decisions` (agent decision recording)

---

## Prerequisites

- Rust toolchain (`rustc`, `cargo`) — stable
- Bun runtime
- `protoc` — Protocol Buffers compiler (`brew install protobuf` on macOS)
- Solana CLI (for keypair generation)
- SolInfra account — Yellowstone gRPC endpoint + RPC endpoint
- Jito block engine access
- Google Gemini API key (free tier at aistudio.google.com)

---

## Setup

### 1. Clone the repository

```bash
git clone <repo-url>
cd heimdall
```

### 2. Configure environment variables

Copy `.env.example` to `.env` at the project root and fill in the real values.
All services read the root `.env`; there is no separate `services/.env` file.

```bash
cp .env.example .env
```

### 3. Generate or use an existing Solana keypair

```bash
# Generate a new keypair
solana-keygen new --outfile ~/.config/solana/id.json

# Fund it on devnet if testing there
solana airdrop 2 --url devnet
```

### 4. Build the Rust workspace

```bash
cd core
cargo build
```

### 5. Install TypeScript dependencies

```bash
cd services/agent
bun install

cd ../monitoring
bun install
```

---

## Running Heimdall

### Option A: Three separate terminals

#### Terminal 1 — Rust core (L1–L5)

```bash
cd core
cargo run
```

Expected output:

```
INFO observation: Connecting to Yellowstone endpoint=https://fra.grpc.solinfra.dev:443
INFO observation: Connected successfully
INFO observation: Slot stream open, listening...
INFO observation: L5 gRPC server starting port=50051
INFO intelligence::consumer: Intelligence engine running...
INFO tracking::tracker: Outcome tracker running...
```

#### Terminal 2 — AI agent (L6)

```bash
cd services/agent
bun run src/index.ts
```

Expected output:

```
Heimdall L6 AI Agent starting...
Two-tier pipeline: Local Rules → Gemini LLM (with fallback)
Connecting to L5 at localhost:50051
Stream created, waiting for data...
Snapshot received: slot=... tip=...
```

#### Terminal 3 — Monitoring (L7)

```bash
cd services/monitoring
bun run src/index.ts
```

Expected output:

```
Heimdall L7 Monitoring running on http://localhost:3000
L7 connected to L5 state stream
```

### Option B: Docker Compose

```bash
docker compose up --build
```

This starts all three services with a shared volume for lifecycle data.
The monitoring API is available at `http://localhost:3000`.

### Option C: CLI Tool

Heimdall includes a CLI for operational control:

```bash
# Live TUI monitoring terminal
bun run cli/heimdall.ts monitor

# Quick status check
bun run cli/heimdall.ts status

# Download evidence report
bun run cli/heimdall.ts evidence

# Check all service health
bun run cli/heimdall.ts health

# Launch all services
bun run cli/heimdall.ts start
```

The `monitor` command provides a live TUI dashboard with:

- Real-time slot pulse and network status
- Bundle outcomes table with stage, failure type, and retry lineage
- AI agent decision history with confidence bars and risk assessment
- Retry metrics (total retries, succeeded, exhausted)

---

## Monitoring Endpoints

With L7 running, query system health via HTTP:

```bash
# System health and uptime
curl http://localhost:3000/health

# Live network metrics
curl http://localhost:3000/metrics

# Recent bundle outcomes (with three-field failure classification)
curl http://localhost:3000/outcomes

# Recent AI agent decisions (with confidence and risk)
curl http://localhost:3000/decisions

# SSE real-time stream (connect via EventSource)
curl http://localhost:3000/events

# Download judge-ready evidence report (Markdown)
curl -o evidence.md http://localhost:3000/evidence
```

Sample `/metrics` response:

```json
{
  "current_slot": 428015020,
  "latest_finalized_slot": 428014988,
  "tip_median_lamports": 1065803,
  "active_bundle_count": 0,
  "total_bundles_submitted": 12,
  "total_bundles_failed": 2,
  "total_bundles_finalized": 10,
  "total_retries": 3,
  "retries_succeeded": 2,
  "retries_exhausted": 1,
  "uptime_seconds": 42
}
```

---

## Fault Injection

To trigger the autonomous retry demonstration, set `BLOCKHASH_MODE=fault_injected`
in `.env` or export it in the shell before starting Heimdall.

The AI agent will detect the failure via `getBundleStatuses`, classify it using
three-field decomposition, and output a structured retry decision:

```
=== AGENT DECISION (GEMINI) ===
Action: RETRY
Reason: 2 bundles failed due to expired blockhash...
Failure: expired_blockhash
Refresh blockhash: true
Suggested tip: 1,969,705 lamports
Confidence: 90%
Risk: Blockhash expiry indicates the bundle exceeded the 150-slot validity window.
Source: gemini
==========================================
```

The retry flows through L5 to L3, which applies exponential backoff (2s, 4s,
8s, 16s) for up to 4 attempts. Each retry fetches a fresh blockhash (bypassing
fault injection) and uses the agent's suggested tip.

Reset to `BLOCKHASH_MODE=normal` after demonstration.

---

## Lifecycle Log

Bundle lifecycle data is written to `core/lifecycle.log` as newline-delimited
JSON (NDJSON). Each entry contains:

- Bundle ID, target slot, leader identity
- Tip amount in lamports
- Blockhash used at construction
- Confirmation source (`yellowstone_stream` or `slot_heuristic`)
- Submission, processed, confirmed, and finalized timestamps
- Commitment progression slot numbers
- Latency deltas between stages in seconds
- Final status
- Three-field failure classification:
  - `failure_reason` — machine-readable category
  - `failure_stage` — pipeline stage where failure occurred
  - `recovery` — human-readable recovery guidance

The curated submission log is at `core/final_lifecycle.log`.

---

## README Questions

### Question 1

**What does the delta between `processed_at` and `confirmed_at` tell you about network health at the time of submission?**

From observing Heimdall running on mainnet, the `processed_at` to `confirmed_at`
delta directly reflects how quickly 2/3 of stake-weighted validators vote on the
slot containing your bundle.

In the `final_lifecycle.log`, most bundles show a delta of 0–1 seconds between
`processed_at` and `confirmed_at`. This indicates healthy network conditions —
validators are voting fast, stake is online and responsive, and there is no
significant fork pressure causing validators to withhold votes.

When this delta grows to 3–6 seconds (as seen in entries like `562e23339e23` with
`latency_processed_secs: 6`), it signals degraded network health at that moment:
validators may be catching up after a burst of slots, the network may be
experiencing higher-than-normal load, or there is mild fork ambiguity causing
some validators to delay votes.

A delta above 10 seconds would be a serious warning signal — it means the network
is under significant stress or a fork is being resolved. In a production system,
this is exactly the kind of signal L2 should use to hold new submissions until
conditions stabilize.

A delta of 0 seconds doesn't mean the bundle was confirmed instantly — it means
the confirmation happened within the same second as processing, which on Solana's
400ms slot cadence is the normal expected behavior under healthy conditions.

---

### Question 2

**Why should you never use finalized commitment when fetching a blockhash for a time-sensitive transaction?**

Blockhashes expire after approximately 150 slots (~60 seconds) on Solana. The
`finalized` commitment level lags behind the network tip by roughly 31–32 slots
— the time it takes for a supermajority of stake to vote on a slot AND for that
vote count to itself be finalized.

If you fetch a blockhash at `finalized` commitment, you are fetching a blockhash
that is already 31–32 slots old before your transaction is even constructed. On a
150-slot expiry window, you have consumed roughly 20% of your validity window
before the transaction leaves your machine. By the time your transaction is
submitted, propagated, queued by a leader, and executed — especially under any
network congestion — there is a meaningful probability the blockhash has expired.

Heimdall uses `confirmed` commitment for all blockhash fetches. `confirmed`
blockhashes are 0–2 slots old at fetch time and have already been voted on by
2/3 of stake, meaning they are extremely unlikely to be rolled back. This gives
your transaction the full ~150-slot validity window while still using a
commitment level that is safe against forks.

`processed` commitment would give an even fresher blockhash but carries the risk
of using a blockhash from a slot that gets orphaned, causing immediate transaction
failure. `confirmed` is the correct tradeoff for time-sensitive bundle submission.

---

### Question 3

**What happens to your bundle if the Jito leader skips their slot?**

If the Jito-enabled leader skips their slot, your bundle is silently dropped.
Jito bundles are only valid for the specific leader rotation they were submitted
for — they are not re-queued for the next leader. The block engine routes your
bundle to the expected leader's TPU; if that leader produces no block, the bundle
has nowhere to land.

From observing Heimdall on mainnet, this surfaces in two ways. First,
`getBundleStatuses` returns an empty `value: []` array — the bundle ID is
unknown to Jito because it was never executed. Second, the bundle simply
disappears from L4's tracking window without ever reaching `Finalized`.

This is distinct from an expired blockhash failure where the bundle is rejected
at execution time. A skipped slot means the bundle was never attempted at all.

The correct response is to detect the empty `getBundleStatuses` result, fetch a
fresh blockhash (the original may still be valid), recalculate the tip based on
current conditions for the new leader, and resubmit. This is precisely what
Heimdall's autonomous retry agent does — the two-tier pipeline classifies the
failure, decides the retry parameters with a confidence score, and L3 resubmits
with exponential backoff (up to 4 attempts).

In practice, Jito leaders skip slots at a low but non-zero rate. Production
systems must treat every bundle submission as potentially needing a retry and
build the retry path as a first-class concern, not an afterthought.

---

## Project Structure

```
heimdall/
├── core/                          # Rust workspace (L1–L5)
│   ├── observation/               # L1 — Network Observation
│   │   └── src/
│   │       ├── main.rs            # Entry point, Yellowstone stream, signature watcher
│   │       └── leader.rs          # Leader schedule fetcher
│   ├── intelligence/              # L2 — Transaction Intelligence
│   │   └── src/consumer.rs        # Candidate deduplication
│   ├── submission/                # L3 — Transaction Submission
│   │   └── src/
│   │       ├── submitter.rs       # Bundle submitter with retry support
│   │       ├── blockhash.rs       # Normal / FaultInjected / FetchFresh modes
│   │       ├── tip.rs             # Jito API + balance fallback tip calculator
│   │       └── bundle.rs          # Payload + tip transaction construction
│   ├── tracking/                  # L4 — Confirmation Tracking
│   │   └── src/
│   │       ├── tracker.rs         # Outcome tracker with three-field classification
│   │       ├── types.rs           # BundleOutcome, FailureReason, FailureStage
│   │       └── lifecycle_log.rs   # NDJSON log writer
│   ├── state/                     # L5 — Operational State Engine
│   │   └── src/
│   │       ├── server.rs          # gRPC Subscribe + Retry + Health RPC
│   │       └── engine.rs          # OperationalState → OperationalSnapshot mapper
│   ├── shared/                    # Shared types across crates
│   │   └── src/
│   │       ├── types.rs           # SubmissionRecord, BundleOutcomeSummary, channels
│   │       └── engine.rs          # OperationalState struct + retry metrics
│   └── Dockerfile                 # Multi-stage Rust build
├── services/                      # TypeScript workspace (L6–L7)
│   ├── agent/                     # L6 — AI Decision Layer
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── agent.ts           # Two-tier pipeline: local rules + Gemini + escalation
│   │       ├── agent.test.ts      # 6 unit tests for rules engine
│   │       ├── client.ts          # gRPC client + L7 decision push
│   │       ├── index.ts           # Main loop with rate limiting
│   │       └── types.ts           # TypeScript interfaces
│   └── monitoring/                # L7 — Monitoring & Control
│       ├── Dockerfile
│       └── src/
│           ├── index.ts           # HTTP + SSE + evidence export
│           ├── client.ts          # gRPC client to L5
│           ├── store.ts           # Metrics store + retry tracking
│           └── types.ts           # TypeScript interfaces
├── cli/                           # CLI Tool
│   ├── heimdall.ts                # CLI with TUI monitor, status, evidence, health
│   └── package.json
├── scripts/
│   └── smoke-test.sh              # End-to-end pipeline validation
├── proto/
│   └── heimdall.proto             # gRPC service definition (Subscribe + Retry + Health)
├── ARCHITECTURE.md                # Architecture document (publish externally)
├── EVIDENCE.md                    # Operational evidence report
├── docker-compose.yml             # 3-service orchestration
├── .env.example                   # Environment variable template
└── core/final_lifecycle.log       # Curated submission lifecycle log
```

---

## Key Design Decisions

**Rust/TypeScript boundary via gRPC**
L5 exposes an `OperationalStateService` via tonic/gRPC. L6 and L7 subscribe
as clients. This gives typed, streaming, language-agnostic communication
across the runtime boundary without polling overhead.

**Two-tier AI reasoning pipeline**
A deterministic local rules engine runs first, producing a decision with
confidence score and risk assessment. Gemini LLM then refines the decision
with independent reasoning against hard constraints. If Gemini is unreachable,
the local rules decision is used as a fallback. This ensures the system never
depends on a single external API for operational decisions.

**Dynamic tip calculation**
Tips are never hardcoded. L3 queries the Jito recommended tip API first and
falls back to live tip-account balances if that API is unavailable. A floor
(1,000 lamports) and cap (100,000 lamports) keep the value in a sane range.

**Exponential backoff retry**
Retry attempts use exponential backoff (2s, 4s, 8s, 16s) with a maximum of
4 attempts per bundle. Each retry fetches a fresh blockhash (bypassing fault
injection) and uses the agent's suggested tip. After max attempts, the bundle
is dropped and the failure is logged.

**Three-field failure classification**
Every failure is decomposed into `failure_reason` (machine-readable category),
`failure_stage` (pipeline location), and `recovery` (human-readable guidance).
This matches production-grade failure semantics and gives judges full
transparency into failure handling.

**Fault injection via `BLOCKHASH_MODE`**
An environment variable controls whether L3 fetches a real blockhash or
returns `Hash::default()` (all zeros). This makes fault injection a
first-class feature without source changes. The retry path's `fetch_fresh()`
bypasses this mode, ensuring retries produce a different outcome.

**Yellowstone signature confirmation**
After each bundle submission, L1 spawns a dedicated Yellowstone gRPC
transaction subscription filtered by the bundle's signatures. This provides
sub-second confirmation without relying solely on slot-range heuristics.
The heuristic remains as a fallback; the `confirmation_source` field tracks
which path confirmed each bundle.

---

## Infrastructure

- **Solana cluster:** Mainnet
- **gRPC provider:** SolInfra Yellowstone (`fra.grpc.solinfra.dev:443`)
- **RPC provider:** SolInfra (`fra.rpc.solinfra.dev`)
- **Bundle submission:** Jito block engine (`mainnet.block-engine.jito.wtf`)
- **Dynamic tips:** Jito tip floor API (`bundles.jito.wtf`)
- **AI model:** Google Gemini `gemini-2.5-flash` (with local rules fallback)
- **L5 gRPC port:** 50051
- **L7 HTTP port:** 3000

---

## Verification Guide

### For Judges

1. **Verify bundle IDs on Jito Explorer**: Copy any `bundle_id` from
   `core/final_lifecycle.log` and search at [explorer.jito.wtf](https://explorer.jito.wtf/)
2. **Verify slots on Solana Explorer**: Copy any `slot` number and search at
   [explorer.solana.com](https://explorer.solana.com/)
3. **Inspect lifecycle log**: Open `core/final_lifecycle.log` — each line is
   NDJSON with full commitment-stage progression, three-field failure
   classification, and retry lineage
4. **Check retry lineage**: Look for entries with `retry_attempt > 0` and their
   `original_bundle_id` pointing back to the first submission
5. **Run the smoke test**: `bash scripts/smoke-test.sh` (requires L7 running)
6. **Download evidence**: `curl http://localhost:3000/evidence` for a formatted
   Markdown report
7. **Use the TUI**: `bun run cli/heimdall.ts monitor` for a live terminal
   dashboard

### What to look for in the logs

| Field                    | Where to find it | What it tells you                                          |
| ------------------------ | ---------------- | ---------------------------------------------------------- |
| `bundle_id`              | lifecycle.log    | Unique Jito bundle identifier, verifiable on Jito Explorer |
| `slot`                   | lifecycle.log    | Target slot, verifiable on Solana Explorer                 |
| `tip_lamports`           | lifecycle.log    | Real tip paid, demonstrates dynamic tip calculation        |
| `latency_processed_secs` | lifecycle.log    | Time from submission to processed confirmation             |
| `confirmation_source`    | lifecycle.log    | Whether confirmed via Yellowstone stream or slot heuristic |
| `failure_reason`         | lifecycle.log    | Machine-readable failure category                          |
| `failure_stage`          | lifecycle.log    | Where in the pipeline the failure occurred                 |
| `recovery`               | lifecycle.log    | Human-readable recovery guidance                           |
| `original_bundle_id`     | lifecycle.log    | Links a retry to its original submission                   |
| `retry_attempt`          | lifecycle.log    | Which retry attempt this was (0 = original)                |

---

## Running Tests

### Rust tests (6 tests)

```bash
cd core && cargo test
```

Tests cover: tip calculation, failure classification, lifecycle latency, bundle
status parsing.

### TypeScript tests (6 tests)

```bash
cd services/agent && bun test
```

Tests cover: local rules engine (no failures, expired blockhash, fee too low,
non-recoverable failures, critical failure rate, hold mode escalation).

### Smoke test (end-to-end)

```bash
# Start all services first, then:
bash scripts/smoke-test.sh
```

Validates: health endpoint, metrics, outcomes, decisions, SSE stream, evidence
export, decision POST.

---

## Runbook

### Launch sequence

1. Start the Rust core: `cd core && cargo run`
2. Wait for "L5 gRPC server starting port=50051"
3. Start the agent: `cd services/agent && bun run src/index.ts`
4. Wait for "Stream created, waiting for data..."
5. Start monitoring: `cd services/monitoring && bun run src/index.ts`
6. Wait for "L7 Monitoring running on http://localhost:3000"

Or use `bun run cli/heimdall.ts start` to launch all three at once.

### Generating fresh evidence

1. Set `BLOCKHASH_MODE=normal` in `.env`
2. Start all services and let 10 bundles finalize
3. Stop the core, set `BLOCKHASH_MODE=fault_injected`
4. Restart core and let 2 bundles fail (these trigger the retry pipeline)
5. Copy `core/lifecycle.log` to `core/final_lifecycle.log`
6. Download the evidence report: `curl -o evidence.md http://localhost:3000/evidence`

### Troubleshooting

| Symptom                        | Likely cause                      | Fix                           |
| ------------------------------ | --------------------------------- | ----------------------------- |
| "ECONNREFUSED 127.0.0.1:50051" | Rust core not running             | Start core first              |
| "Rate limited" in agent output | Gemini API rate limit             | Wait 30s, agent auto-recovers |
| "Max retry attempts reached"   | Bundle exhausted all 4 retries    | Check failure type in logs    |
| "HOLD MODE active"             | 3+ consecutive retry failures     | Wait 60s for cooldown         |
| "Non-recoverable" in decisions | compute_exceeded or program_error | Fix the transaction payload   |
