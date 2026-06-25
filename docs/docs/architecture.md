---
sidebar_position: 2
title: Architecture
---

# System Architecture

Heimdall is a seven-layer transaction infrastructure stack split across a Rust core and Bun/TypeScript services.

## System Context

```mermaid
flowchart LR
  Validator["Solana Validator Network"]
  Yellowstone["Yellowstone Geyser gRPC"]
  Jito["Jito Block Engine"]
  RPC["Solana RPC"]
  TipAPI["Jito Tip Floor API"]
  Agent["Gemini AI Agent"]
  Monitor["Monitoring Service"]
  Heimdall["Heimdall Core (Rust, L1-L5)"]

  Validator --> Yellowstone
  Yellowstone --> Heimdall
  Heimdall --> Jito
  Heimdall --> RPC
  Heimdall --> TipAPI
  Heimdall --> Agent
  Heimdall --> Monitor
  Agent --> Heimdall
```

## Container View

```mermaid
flowchart TB
  subgraph Rust["Rust Core (tokio async runtime)"]
    L1["L1 Observation\nYellowstone gRPC slot stream\n+ transaction signature watcher"]
    L2["L2 Intelligence\nCandidate deduplication\n+ leader window filtering"]
    L3["L3 Submission\nJito bundle construction\n+ dynamic tips + fault injection\n+ exponential backoff retry"]
    L4["L4 Tracking\nLifecycle tracking\n+ three-field failure classification\n+ Jito status polling"]
    L5["L5 State Engine\ngRPC server streaming\nOperationalSnapshot to services"]
    L1 -->|"NetworkEvent (mpsc)"| L2
    L2 -->|"TransactionCandidate (mpsc)"| L3
    L3 -->|"SubmissionRecord (mpsc)"| L4
    L4 -->|"state update (Arc Mutex)"| L5
  end

  subgraph TypeScript["TypeScript Services (Bun runtime)"]
    L6["L6 Agent\nTwo-tier AI pipeline\nLocal rules + Gemini LLM"]
    L7["L7 Monitoring\nHTTP + SSE + Evidence"]
  end

  L5 -->|"gRPC stream"| L6
  L5 -->|"gRPC stream"| L7
  L6 -->|"RetryRequest (gRPC)"| L5
  L5 -->|"mpsc"| L3
```

## Layer Responsibilities

| Layer | Name | Runtime | Responsibility |
|---|---|---|---|
| L1 | Observation | Rust | Yellowstone gRPC slot stream, transaction signature watchers, RPC fallback |
| L2 | Intelligence | Rust | Candidate deduplication, leader window filtering, slot-to-candidate pipeline |
| L3 | Submission | Rust | Jito bundle construction, dynamic tip calculation, retry execution with backoff |
| L4 | Tracking | Rust | Bundle lifecycle tracking, three-field failure classification, Jito status polling |
| L5 | State Engine | Rust | Operational state aggregation, gRPC server (Subscribe + Retry + Health RPCs) |
| L6 | Agent | TypeScript | Two-tier AI pipeline (local rules + Gemini), failure escalation, hold mode |
| L7 | Monitoring | TypeScript | HTTP API (7 endpoints), SSE streaming, evidence report generation |

## Infrastructure Decisions

| Decision | Rationale |
|---|---|
| Rust for L1–L5 | Network-facing hot path needs deterministic state handling and low overhead |
| Bun/TypeScript for L6–L7 | AI and monitoring layers benefit from rapid iteration and JSON/gRPC integration |
| gRPC control plane | Typed, streaming, language-agnostic communication between Rust and TypeScript |
| `tokio::mpsc` channels | Decoupled Rust layers without network hops; non-blocking communication |
| NDJSON lifecycle log | Append-only format auditable after execution; structured for programmatic parsing |
| Exponential backoff | Industry-standard retry pattern preventing thundering herd on transient failures |
| Two-tier AI pipeline | Deterministic rules catch obvious cases; LLM handles nuanced reasoning; fallback ensures availability |
| Retry lineage | Full chain of custody from original → retry enables root cause analysis |
