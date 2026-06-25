---
slug: /
sidebar_position: 1
title: Introduction
---

# Heimdall — Smart Transaction Infrastructure Stack

Heimdall is a seven-layer Solana transaction infrastructure stack that observes the network in real time, submits transactions intelligently via Jito bundles, tracks outcomes across commitment levels, and uses a two-tier AI agent to autonomously detect failures and decide retry strategies.

Built for the **Superteam Earn** bounty: *Smart Transaction Infrastructure Stack*.

## Key Capabilities

| Capability | Implementation |
|---|---|
| **Network Observation** | Yellowstone gRPC slot stream + per-bundle signature watchers + RPC fallback |
| **Bundle Submission** | Jito block engine with dynamic tip calculation (Jito API + balance proxy) |
| **Commitment Tracking** | Processed → Confirmed → Finalized with per-stage latency measurement |
| **Failure Classification** | Three-field decomposition: type + stage + recovery guidance |
| **AI Agent** | Two-tier pipeline: local deterministic rules → Gemini LLM reasoning → fallback |
| **Retry Strategy** | Exponential backoff (2s→4s→8s→16s), max 4 attempts, full lineage tracking |
| **Failure Escalation** | Hold mode after 3 consecutive failures, non-recoverable failure detection |
| **Monitoring** | Live TUI dashboard, 7 HTTP endpoints, SSE streaming, Markdown evidence export |

## Technology Stack

| Runtime | Responsibility | Language |
|---|---|---|
| **Core (L1–L5)** | Network observation, transaction intelligence, bundle submission, confirmation tracking, operational state | Rust (tokio async) |
| **Agent (L6)** | AI decision pipeline, retry reasoning, failure escalation | TypeScript (Bun) |
| **Monitor (L7)** | HTTP API, SSE streaming, evidence generation, metrics | TypeScript (Bun) |
| **CLI** | Operational commands, TUI monitoring terminal | TypeScript (Bun) |

## Quick Start

```bash
# 1. Configure
cp .env.example .env   # Fill in real values

# 2. Build
cd core && cargo build

# 3. Run
cd core && cargo run                             # Terminal 1: L1-L5
cd services/agent && bun run src/index.ts        # Terminal 2: L6
cd services/monitoring && bun run src/index.ts   # Terminal 3: L7

# 4. Monitor
bun run cli/heimdall.ts monitor                  # Live TUI dashboard
```

Or use Docker Compose:

```bash
docker compose up --build
```
