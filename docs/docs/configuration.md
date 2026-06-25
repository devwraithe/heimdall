---
sidebar_position: 10
title: Configuration
---

# Configuration Reference

All configuration is via environment variables in the root `.env` file.

## Required Variables

| Variable | Description | Example |
|---|---|---|
| `GRPC_ENDPOINT` | Yellowstone gRPC endpoint (with port) | `https://fra.grpc.solinfra.dev:443` |
| `GRPC_X_TOKEN` | Authentication token for gRPC provider | `your_solinfra_token` |
| `RPC_ENDPOINT` | Solana RPC endpoint | `https://fra.rpc.solinfra.dev` |
| `JITO_URL` | Jito block engine API URL | `https://mainnet.block-engine.jito.wtf/api/v1` |
| `KEYPAIR_PATH` | Path to Solana keypair JSON file | `~/.config/solana/id.json` |
| `GEMINI_API_KEY` | Google Gemini API key | `AIza...` |

## Optional Variables

| Variable | Default | Description |
|---|---|---|
| `LEADER_WINDOW` | `4` | Number of upcoming slots to consider for submission |
| `BLOCKHASH_MODE` | `normal` | Set to `fault_injected` to simulate expired blockhash failures |
| `TIP_PREMIUM_PERCENT` | `10` | Percentage premium above median tip |
| `TIP_MIN_LAMPORTS` | `1000` | Minimum tip floor in lamports |
| `L5_ADDRESS` | `localhost:50051` | L5 gRPC server address (for services) |
| `L7_ADDRESS` | `http://localhost:3000` | L7 monitoring URL (for agent) |
| `MONITOR_URL` | `http://localhost:3000` | Monitoring URL (for CLI) |

## Jito Tip Accounts

The `JITO_TIP_ACCOUNTS` variable accepts a comma-separated list of Jito tip account public keys. If not set, the 8 default Jito tip accounts are used.

## Docker Compose

When running via Docker Compose, the following are set automatically:
- `L5_ADDRESS=core:50051` (agent → core)
- `L7_ADDRESS=http://monitoring:3000` (agent → monitoring)
- `LIFECYCLE_LOG_PATH=/app/shared/lifecycle.log` (shared volume)
