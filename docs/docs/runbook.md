---
sidebar_position: 6
title: Runbook
---

# Operational Runbook

## Launch Sequence

1. Start the Rust core: `cd core && cargo run`
2. Wait for `L5 gRPC server starting port=50051`
3. Start the agent: `cd services/agent && bun run src/index.ts`
4. Wait for `Stream created, waiting for data...`
5. Start monitoring: `cd services/monitoring && bun run src/index.ts`
6. Wait for `L7 Monitoring running on http://localhost:3000`

Or use `bun run cli/heimdall.ts start` to launch all three at once.

## Generating Fresh Evidence

1. Set `BLOCKHASH_MODE=normal` in `.env`
2. Start all services and let 10+ bundles finalize
3. Stop the core, set `BLOCKHASH_MODE=fault_injected`
4. Restart core and let 2+ bundles fail (triggers retry pipeline)
5. Copy `core/lifecycle.log` to `core/final_lifecycle.log`
6. Download the evidence report: `bun run cli/heimdall.ts evidence`

## Running Tests

### Rust (6 tests)

```bash
cd core && cargo test
```

### TypeScript (6 tests)

```bash
cd services/agent && bun test
```

### End-to-End Smoke Test

```bash
bash scripts/smoke-test.sh
```

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---|---|---|
| `ECONNREFUSED 127.0.0.1:50051` | Rust core not running | Start core first |
| `Rate limited` in agent output | Gemini API rate limit | Wait 30s, agent auto-recovers |
| `Max retry attempts reached` | Bundle exhausted all 4 retries | Check failure type in logs |
| `HOLD MODE active` | 3+ consecutive retry failures | Wait 60s for cooldown |
| `Non-recoverable` in decisions | `compute_exceeded` or `program_error` | Fix the transaction payload |
| SSE stream disconnects | L5 restarted | L7 auto-reconnects in 2s |
| No bundles appearing | Leader schedule mismatch | Check `LEADER_WINDOW` env var |
