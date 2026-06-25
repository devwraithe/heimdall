---
sidebar_position: 9
title: Failure Classification
---

# Failure Classification

Heimdall uses a three-field failure decomposition for every detected failure:

| Field | Purpose | Example |
|---|---|---|
| `failure_reason` | Machine-readable failure category | `ExpiredBlockhash` |
| `failure_stage` | Where in the pipeline the failure occurred | `execution` |
| `recovery` | Human-readable recovery guidance | `Fetch a fresh blockhash and resubmit` |

## Failure Types

| Type | Description | Stage | Recovery |
|---|---|---|---|
| `ExpiredBlockhash` | Blockhash exceeded 150-slot validity window | `execution` | Fetch fresh blockhash, resubmit with updated transaction |
| `FeeTooLow` | Jito rejected the bundle due to insufficient tip | `submission` | Increase tip amount, consider current tip competition |
| `ComputeExceeded` | Transaction exceeded compute unit limit | `execution` | Reduce compute usage or increase compute budget |
| `BundleFailure` | Jito reported generic bundle failure | `execution` | Inspect transaction logs, check for conflicts |
| `Unknown(msg)` | Unclassified failure with raw error message | varies | Investigate the raw error message |

## Non-Recoverable Failures

The following failure types are flagged as **non-recoverable** — the AI agent will not retry them:

- `compute_exceeded` — The transaction itself is too expensive
- `program_error` — The on-chain program returned an error
- `account_not_found` — A required account doesn't exist

These require fixing the transaction payload before resubmission.

## Classification in Code

### Rust (L4 Tracker)

```rust
pub enum FailureReason {
    ExpiredBlockhash,
    FeeTooLow,
    ComputeExceeded,
    BundleFailure,
    Unknown(String),
}

pub enum FailureStage {
    PreSubmission,
    Submission,
    Execution,
    Confirmation,
}
```

### TypeScript (L6 Agent)

The agent classifies failures as strings matching the Rust enum names, allowing consistent cross-language handling.

## Where Classification Appears

1. **Lifecycle log** (`core/lifecycle.log`) — `failure_reason`, `failure_stage`, `recovery` fields
2. **gRPC snapshot** — `BundleOutcomeSummary.failure_reason/stage/recovery`
3. **GET /outcomes** — HTTP API response
4. **GET /evidence** — Markdown report table
5. **TUI monitor** — Color-coded failure columns
