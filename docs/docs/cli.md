---
sidebar_position: 7
title: CLI Reference
---

# CLI Reference

Heimdall includes a command-line interface for operational control.

## Usage

```bash
bun run cli/heimdall.ts <command>
```

## Commands

### `monitor`

Live TUI monitoring dashboard connected to the SSE `/events` stream.

Features:
- Real-time slot pulse and network status (slot gap, tip median, uptime)
- Bundle outcomes table with stage, failure type, tip, and retry lineage
- AI agent decision history with confidence bars, source, and risk assessment
- Color-coded stages (green=Finalized, cyan=Confirmed, yellow=Processed, red=Failed)

```bash
bun run cli/heimdall.ts monitor
```

Press `Ctrl+C` to exit.

### `status`

Print current system status including health and metrics.

```bash
bun run cli/heimdall.ts status
```

### `evidence`

Download a judge-ready Markdown evidence report and save it locally.

```bash
bun run cli/heimdall.ts evidence
# → heimdall-evidence-1719158400000.md
```

### `health`

Check health of all services with latency timing.

```bash
bun run cli/heimdall.ts health
```

### `start`

Launch all three services (core, agent, monitoring) in one command.

```bash
bun run cli/heimdall.ts start
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `MONITOR_URL` | `http://localhost:3000` | L7 monitoring service URL |
