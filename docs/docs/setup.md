---
sidebar_position: 5
title: Setup Guide
---

# Setup Guide

## Prerequisites

- **Rust toolchain** (`rustc`, `cargo`) — stable channel
- **Bun runtime** — v1.0+
- **protoc** — Protocol Buffers compiler (`brew install protobuf` on macOS)
- **Solana CLI** — for keypair generation
- **SolInfra account** — Yellowstone gRPC endpoint + RPC endpoint
- **Jito block engine access** — mainnet
- **Google Gemini API key** — free tier at [aistudio.google.com](https://aistudio.google.com)

## Step 1: Clone & Configure

```bash
git clone <repo-url>
cd heimdall
cp .env.example .env
```

Edit `.env` with your credentials. See the [Configuration Reference](/configuration) for all options.

## Step 2: Generate Solana Keypair

```bash
solana-keygen new --outfile ~/.config/solana/id.json
solana airdrop 2 --url devnet   # Fund on devnet if testing
```

## Step 3: Build Rust Workspace

```bash
cd core
cargo build
```

## Step 4: Install TypeScript Dependencies

```bash
cd services/agent && bun install
cd ../monitoring && bun install
```

## Step 5: Run

### Option A: Manual (3 terminals)

```bash
# Terminal 1 — Core (L1-L5)
cd core && cargo run

# Terminal 2 — Agent (L6)
cd services/agent && bun run src/index.ts

# Terminal 3 — Monitoring (L7)
cd services/monitoring && bun run src/index.ts
```

### Option B: Docker Compose

```bash
docker compose up --build
```

### Option C: CLI

```bash
bun run cli/heimdall.ts start
```

## Step 6: Verify

```bash
# Check health
bun run cli/heimdall.ts health

# Open TUI monitor
bun run cli/heimdall.ts monitor

# Run smoke test
bash scripts/smoke-test.sh
```
