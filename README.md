# Heimdall

A smart Solana transaction infrastructure stack that observes the network in
real time, submits transactions intelligently via Jito bundles, tracks outcomes
across commitment levels, and uses an AI agent to autonomously detect failures
and decide retry strategies.

Built for the Superteam Earn bounty: **Smart Transaction Infrastructure Stack**.

---

## System Overview

Heimdall is a 7-layer transaction infrastructure stack split across two runtimes:

- **Core (Rust)** — L1 through L5: network observation, transaction intelligence,
  bundle submission, confirmation tracking, and operational state
- **Services (TypeScript/Bun)** — L6 and L7: AI decision agent and monitoring

```
L1 — Network Observation      Yellowstone gRPC slot stream + leader schedule
L2 — Transaction Intelligence  Candidate deduplication and channel to L3
L3 — Transaction Submission   Jito bundle construction, dynamic tips, fault injection
L4 — Confirmation Tracking    Lifecycle tracking, getBundleStatuses polling
L5 — Operational State Engine gRPC server streaming snapshots to L6
L6 — AI Decision Layer        Gemini agent reasoning about failures and retries
L7 — Monitoring & Control     HTTP endpoints exposing live metrics
```

---

## Architecture

Full C4 architecture diagrams (Context, Container, Component) are available in
the public architecture document.

**Inter-layer communication:**

- L1 → L2: `tokio::sync::mpsc` channel carrying `NetworkEvent`
- L2 → L3: `tokio::sync::mpsc` channel carrying `TransactionCandidate`
- L3 → L4: `tokio::sync::mpsc` channel carrying `SubmissionRecord`
- L1 → L4: `tokio::sync::mpsc` channel carrying `SlotConfirmation`
- L5 → L6: gRPC server-side streaming (`OperationalSnapshot` every 500ms)
- L5 → L7: gRPC server-side streaming (same subscription)

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

Create a `.env` file at the project root:

```env
# SolInfra Yellowstone gRPC
GRPC_ENDPOINT=https://fra.grpc.solinfra.dev:443
GRPC_X_TOKEN=your_solinfra_api_key

# SolInfra RPC
RPC_ENDPOINT=https://fra.rpc.solinfra.dev/sol?api_key=your_solinfra_api_key

# Jito block engine
JITO_URL=https://mainnet.block-engine.jito.wtf/api/v1

# Solana keypair
KEYPAIR_PATH=/path/to/your/solana/keypair.json

# Google Gemini
GEMINI_API_KEY=your_gemini_api_key
```

Create a `.env` file in `services/`:

```env
GEMINI_API_KEY=your_gemini_api_key
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

Heimdall runs as three separate processes. Open three terminals.

### Terminal 1 — Rust core (L1–L5)

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

### Terminal 2 — AI agent (L6)

```bash
cd services/agent
bun run src/index.ts
```

Expected output:

```
Heimdall L6 AI Agent starting...
Connecting to L5 at localhost:50051
Stream created, waiting for data...
Snapshot received: slot=... tip=...
```

### Terminal 3 — Monitoring (L7)

```bash
cd services/monitoring
bun run src/index.ts
```

Expected output:

```
Heimdall L7 Monitoring running on http://localhost:3000
L7 connected to L5 state stream
```

---

## Monitoring Endpoints

With L7 running, query system health via HTTP:

```bash
# System health and uptime
curl http://localhost:3000/health

# Live network metrics
curl http://localhost:3000/metrics

# Recent bundle outcomes
curl http://localhost:3000/outcomes

# Recent AI agent decisions
curl http://localhost:3000/decisions
```

Sample `/metrics` response:

```json
{
  "current_slot": 428015020,
  "latest_finalized_slot": 428014988,
  "tip_median_lamports": 1065803,
  "active_bundle_count": 0,
  "total_bundles_failed": 2,
  "total_bundles_finalized": 0,
  "uptime_seconds": 42
}
```

---

## Fault Injection

To trigger the autonomous retry demonstration, switch `BlockhashMode` in
`core/observation/src/main.rs`:

```rust
// Inject an expired blockhash to trigger AI agent retry decision
let submitter = BundleSubmitter::new(
    &rpc_url,
    &jito_url_for_l3,
    keypair,
    BlockhashMode::FaultInjected, // ← change this
);
```

The AI agent will detect the failure via `getBundleStatuses`, classify it as
`ExpiredBlockhash`, and output a structured retry decision:

```
=== AGENT RETRY DECISION ===
Reason: All bundles failed due to ExpiredBlockhash...
Failure: ExpiredBlockhash
Refresh blockhash: true
Suggested tip: 2465543 lamports
============================
```

Reset to `BlockhashMode::Normal` after demonstration.

---

## Lifecycle Log

Bundle lifecycle data is written to `core/lifecycle.log` as newline-delimited
JSON (NDJSON). Each entry contains:

- Bundle ID, target slot, leader identity
- Tip amount in lamports
- Blockhash used at construction
- Submission, processed, confirmed, and finalized timestamps
- Commitment progression slot numbers
- Latency deltas between stages in seconds
- Final status and failure classification

The curated submission log is at `core/final_lifecycle.log` — 10 real
mainnet bundle submissions plus 2 ExpiredBlockhash failure cases. All slot
numbers are verifiable at `explorer.jito.wtf`.

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
Heimdall's autonomous retry agent does — it classifies the failure and decides
the retry parameters without hardcoded logic.

In practice, Jito leaders skip slots at a low but non-zero rate. Production
systems must treat every bundle submission as potentially needing a retry and
build the retry path as a first-class concern, not an afterthought.

---

## Project Structure

```
heimdall/
├── core/                          # Rust workspace (L1–L5)
│   ├── observation/               # L1 — Network Observation
│   ├── intelligence/              # L2 — Transaction Intelligence
│   ├── submission/                # L3 — Transaction Submission
│   ├── tracking/                  # L4 — Confirmation Tracking
│   ├── state/                     # L5 — Operational State Engine
│   └── shared/                    # Shared types across crates
├── services/                      # TypeScript workspace (L6–L7)
│   ├── agent/                     # L6 — AI Decision Layer
│   └── monitoring/                # L7 — Monitoring & Control
├── proto/
│   └── heimdall.proto             # gRPC service definition
├── docs/                          # Architecture diagrams
├── core/final_lifecycle.log       # Curated submission lifecycle log
└── .env                           # Environment configuration
```

---

## Key Design Decisions

**Rust/TypeScript boundary via gRPC**
L5 exposes an `OperationalStateService` via tonic/gRPC. L6 and L7 subscribe
as clients. This gives typed, streaming, language-agnostic communication
across the runtime boundary without polling overhead.

**Dynamic tip calculation**
Tips are never hardcoded. L3 samples all 8 Jito tip accounts via RPC, takes
the median balance, and applies a 10% premium. This positions each bundle
competitively without overpaying.

**Fault injection via `BlockhashMode`**
A single enum controls whether L3 fetches a real blockhash or returns
`Hash::default()` (all zeros). This makes fault injection a first-class
feature — toggling it on/off requires one line change and does not affect
any other part of the stack.

**No hardcoded retry logic**
The AI agent owns all retry decisions. L4 detects failure, L5 aggregates
it, L6 reasons about it. The agent prompt gives Gemini network conditions,
failure data, and tip information — it returns structured JSON with
`shouldRetry`, `refreshBlockhash`, `suggestedTipLamports`, and
`failureClassification`. Sequential automation without reasoning does not
qualify as an AI agent.

---

## Infrastructure

- **Solana cluster:** Mainnet
- **gRPC provider:** SolInfra Yellowstone (`fra.grpc.solinfra.dev:443`)
- **RPC provider:** SolInfra (`fra.rpc.solinfra.dev`)
- **Bundle submission:** Jito block engine (`mainnet.block-engine.jito.wtf`)
- **AI model:** Google Gemini `gemini-1.5-flash`
- **L5 gRPC port:** 50051
- **L7 HTTP port:** 3000
