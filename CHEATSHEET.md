# Heimdall Cheatsheet

## 1. Setup (one-time)

```bash
cp .env.example .env          # Fill in real values
cd core && cargo build         # Build Rust core
cd services/agent && bun install
cd services/monitoring && bun install
```

## 2. Start (3 terminals)

```bash
# Terminal 1 — Core (L1-L5)
cd core && cargo run

# Terminal 2 — Agent (L6)  ← wait for "L5 gRPC server starting"
cd services/agent && bun run src/index.ts

# Terminal 3 — Monitor (L7)
cd services/monitoring && bun run src/index.ts
```

Or all at once:
```bash
bun run cli/heimdall.ts start
```

Or Docker:
```bash
docker compose up --build
```

## 3. Monitor

```bash
bun run cli/heimdall.ts monitor    # Live TUI dashboard
bun run cli/heimdall.ts status     # Quick status
bun run cli/heimdall.ts health     # Service health
curl http://localhost:3000/metrics  # JSON metrics
```

## 4. Generate Evidence

```bash
# Step 1: Run with normal blockhash (10+ bundles)
# .env → BLOCKHASH_MODE=normal
cd core && cargo run

# Step 2: Switch to fault injection (2+ failures)
# .env → BLOCKHASH_MODE=fault_injected
cd core && cargo run

# Step 3: Save evidence
cp core/lifecycle.log core/final_lifecycle.log
bun run cli/heimdall.ts evidence
```

## 5. Test

```bash
cd core && cargo test              # 6 Rust tests
cd services/agent && bun test      # 6 agent tests
bash scripts/smoke-test.sh         # E2E smoke test (needs L7 running)
```

## 6. Docs

```bash
cd docs && npm run build           # Build docs site
cd docs && npx serve build         # Preview locally at :3000
cd docs && npx vercel --prod       # Deploy to Vercel
```

## Key URLs (when running)

| URL | What |
|---|---|
| `http://localhost:3000/health` | Health check |
| `http://localhost:3000/metrics` | Live metrics + retry stats |
| `http://localhost:3000/outcomes` | Bundle outcomes |
| `http://localhost:3000/decisions` | AI agent decisions |
| `http://localhost:3000/events` | SSE real-time stream |
| `http://localhost:3000/evidence` | Download Markdown report |

## Key Env Vars

| Var | Values | Purpose |
|---|---|---|
| `BLOCKHASH_MODE` | `normal` / `fault_injected` | Toggle fault injection |
| `LEADER_WINDOW` | `4` (default) | Slots ahead to submit |
| `TIP_PREMIUM_PERCENT` | `10` (default) | % above median tip |
| `TIP_MIN_LAMPORTS` | `1000` (default) | Minimum tip floor |

## Troubleshooting

| Problem | Fix |
|---|---|
| `ECONNREFUSED :50051` | Start core first |
| `Rate limited` | Wait 30s, auto-recovers |
| `HOLD MODE` | 3 failed retries → 60s cooldown |
| No bundles | Check `LEADER_WINDOW` and leader schedule |
