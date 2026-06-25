---
sidebar_position: 11
title: Verification Guide
---

# Verification Guide for Judges

This document explains how to independently verify that Heimdall is a real, working system.

## 1. Verify Bundle IDs on Jito Explorer

1. Open `core/final_lifecycle.log`
2. Copy any `bundle_id` (e.g., `562e2333...`)
3. Visit [Jito Explorer](https://explorer.jito.wtf/)
4. Search for the bundle ID
5. Verify the slot number and status match the log entry

## 2. Verify Slots on Solana Explorer

1. Copy any `slot` number from the lifecycle log (e.g., `428136003`)
2. Visit [Solana Explorer](https://explorer.solana.com/)
3. Search for the slot
4. Verify it exists and matches the timestamp range

## 3. Inspect the Lifecycle Log

Each line in `core/final_lifecycle.log` is a JSON object with:

| Field | What it proves |
|---|---|
| `bundle_id` | Real Jito bundle, verifiable on explorer |
| `slot` | Real Solana slot, verifiable on explorer |
| `tip_lamports` | Dynamic tip calculation from live data |
| `latency_processed_secs` | Real-time tracking of commitment progression |
| `confirmation_source` | Which mechanism confirmed (Yellowstone/Jito/heuristic) |
| `failure_reason` | Machine-readable failure category |
| `failure_stage` | Pipeline location of failure |
| `recovery` | Actionable recovery guidance |
| `original_bundle_id` | Retry lineage — links to first submission |
| `retry_attempt` | Which retry attempt (0 = original) |

## 4. Check Retry Lineage

Look for entries with `retry_attempt > 0`. These should have:
- A non-empty `original_bundle_id` pointing to an earlier entry
- Incrementing attempt numbers (1, 2, 3, 4)
- Different `bundle_id` but same `slot` as the original

## 5. Run the Smoke Test

```bash
# Start all services first, then:
bash scripts/smoke-test.sh
```

The smoke test validates 8 things:
1. L7 health endpoint is reachable
2. Metrics endpoint returns slot data
3. Tip median is non-zero
4. Outcomes endpoint has data
5. Decisions endpoint has data
6. SSE stream is active
7. Evidence export returns Markdown
8. Decision POST is accepted

## 6. Download Evidence Report

```bash
curl -o evidence.md http://localhost:3000/evidence
# or
bun run cli/heimdall.ts evidence
```

The report includes metrics tables, bundle outcomes with retry lineage, and AI decision history.

## 7. Use the TUI Monitor

```bash
bun run cli/heimdall.ts monitor
```

The live dashboard shows real-time slot progression, bundle outcomes, and AI decisions — proving the entire pipeline is functioning end-to-end.

## What to Look For

### Evidence of Real Mainnet Execution
- Non-zero slot numbers (400M+ range for mainnet)
- Tip amounts in the 1M+ lamports range (typical mainnet tips)
- Multiple leader public keys (different validators)
- Sub-second to single-digit latency measurements

### Evidence of Failure Handling
- Entries with `status: "Failed"` and populated `failure_reason`
- Three-field classification (reason + stage + recovery) on every failure
- Retry entries with lineage back to original bundles

### Evidence of AI Decision Making
- Decisions endpoint showing `source: "gemini"` (LLM actually ran)
- Decisions with `source: "fallback"` (graceful degradation works)
- Hold mode entries when consecutive failures exceed threshold
