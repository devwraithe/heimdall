import dotenv from "dotenv";
import path from "path";
dotenv.config({ path: path.resolve("../../.env") });

import { connectToL5 } from "./client";
import { getMetrics, recordDecision } from "./store";
import type { AgentDecision } from "./store";

const PORT = 3000;

function buildResponse(data: unknown): Response {
  return new Response(JSON.stringify(data, null, 2), {
    headers: { "Content-Type": "application/json" },
  });
}

function generateEvidenceReport(): string {
  const m = getMetrics();
  const snap = m.latestSnapshot;
  const lines: string[] = [];

  lines.push("# Heimdall — Operational Evidence Report\n");
  lines.push(`Generated: ${new Date().toISOString()}\n`);
  lines.push("## System Health\n");
  lines.push(`| Metric | Value |`);
  lines.push(`|---|---|`);
  lines.push(`| Current Slot | ${snap?.currentSlot ?? "N/A"} |`);
  lines.push(
    `| Latest Finalized Slot | ${snap?.latestFinalizedSlot ?? "N/A"} |`,
  );
  lines.push(`| Tip Median (lamports) | ${snap?.tipMedianLamports ?? "N/A"} |`);
  lines.push(`| Total Submitted | ${m.totalBundlesSubmitted} |`);
  lines.push(`| Total Finalized | ${m.totalBundlesFinalized} |`);
  lines.push(`| Total Failed | ${m.totalBundlesFailed} |`);
  lines.push(`| Total Retries | ${snap?.totalRetries ?? 0} |`);
  lines.push(`| Retries Succeeded | ${snap?.totalRetriesSucceeded ?? 0} |`);
  lines.push(`| Retries Exhausted | ${snap?.totalRetriesExhausted ?? 0} |`);
  lines.push(`| Uptime (seconds) | ${m.uptimeSeconds} |`);
  lines.push("");

  if (snap && snap.recentOutcomes.length > 0) {
    lines.push("## Recent Bundle Outcomes\n");
    lines.push(
      "| Bundle ID | Slot | Stage | Failure Type | Failure Stage | Tip | Retry | Original |",
    );
    lines.push("|---|---|---|---|---|---|---|---|");
    for (const o of snap.recentOutcomes) {
      lines.push(
        `| ${o.bundleId.slice(0, 12)}... | ${o.slot} | ${o.stage} | ${o.failureReason || "—"} | ${o.failureStage || "—"} | ${o.tipLamports} | ${o.retryAttempt > 0 ? `#${o.retryAttempt}` : "—"} | ${o.originalBundleId ? o.originalBundleId.slice(0, 12) + "..." : "—"} |`,
      );
    }
    lines.push("");
  }

  if (m.recentDecisions.length > 0) {
    lines.push("## AI Agent Decisions\n");
    lines.push(
      "| Time | Action | Classification | Tip | Confidence | Source | Risk |",
    );
    lines.push("|---|---|---|---|---|---|---|");
    for (const d of m.recentDecisions) {
      lines.push(
        `| ${new Date(d.timestamp).toISOString()} | ${d.shouldRetry ? "RETRY" : "HOLD"} | ${d.failureClassification} | ${d.suggestedTipLamports} | ${(d.confidence * 100).toFixed(0)}% | ${d.source} | ${d.observedRisk.slice(0, 50)}... |`,
      );
    }
    lines.push("");
  }

  return lines.join("\n");
}

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);

    if (req.method === "POST" && url.pathname === "/decisions") {
      try {
        const decision = await req.json();
        recordDecision(decision as AgentDecision);
        return buildResponse({ accepted: true });
      } catch (error) {
        return new Response(JSON.stringify({ error: "invalid decision" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        });
      }
    }

    switch (url.pathname) {
      case "/health":
        return buildResponse({
          status: "ok",
          uptime_seconds: getMetrics().uptimeSeconds,
          started_at: new Date(getMetrics().startedAt).toISOString(),
        });

      case "/metrics": {
        const metrics = getMetrics();
        return buildResponse({
          current_slot: metrics.latestSnapshot?.currentSlot ?? 0,
          latest_finalized_slot:
            metrics.latestSnapshot?.latestFinalizedSlot ?? 0,
          tip_median_lamports:
            metrics.latestSnapshot?.tipMedianLamports ?? 0,
          active_bundle_count:
            metrics.latestSnapshot?.activeBundleCount ?? 0,
          total_bundles_submitted: metrics.totalBundlesSubmitted,
          total_bundles_failed: metrics.totalBundlesFailed,
          total_bundles_finalized: metrics.totalBundlesFinalized,
          total_retries: metrics.latestSnapshot?.totalRetries ?? 0,
          retries_succeeded: metrics.latestSnapshot?.totalRetriesSucceeded ?? 0,
          retries_exhausted: metrics.latestSnapshot?.totalRetriesExhausted ?? 0,
          uptime_seconds: metrics.uptimeSeconds,
        });
      }

      case "/outcomes":
        return buildResponse({
          recent_outcomes:
            getMetrics().latestSnapshot?.recentOutcomes ?? [],
        });

      case "/decisions":
        return buildResponse({
          recent_decisions: getMetrics().recentDecisions,
        });

      case "/events": {
        // Server-Sent Events stream of operational snapshots
        const stream = new ReadableStream({
          start(controller) {
            const encoder = new TextEncoder();
            const interval = setInterval(() => {
              const metrics = getMetrics();
              const data = JSON.stringify({
                currentSlot: metrics.latestSnapshot?.currentSlot ?? 0,
                latestFinalizedSlot:
                  metrics.latestSnapshot?.latestFinalizedSlot ?? 0,
                tipMedianLamports:
                  metrics.latestSnapshot?.tipMedianLamports ?? 0,
                activeBundleCount:
                  metrics.latestSnapshot?.activeBundleCount ?? 0,
                totalSubmitted: metrics.totalBundlesSubmitted,
                totalFailed: metrics.totalBundlesFailed,
                totalFinalized: metrics.totalBundlesFinalized,
                uptimeSeconds: metrics.uptimeSeconds,
                recentOutcomes:
                  metrics.latestSnapshot?.recentOutcomes ?? [],
                recentDecisions: metrics.recentDecisions.slice(0, 5),
              });
              controller.enqueue(encoder.encode(`data: ${data}\n\n`));
            }, 1000);

            req.signal.addEventListener("abort", () => {
              clearInterval(interval);
              controller.close();
            });
          },
        });

        return new Response(stream, {
          headers: {
            "Content-Type": "text/event-stream",
            "Cache-Control": "no-cache",
            Connection: "keep-alive",
          },
        });
      }

      case "/evidence": {
        const report = generateEvidenceReport();
        return new Response(report, {
          headers: {
            "Content-Type": "text/markdown; charset=utf-8",
            "Content-Disposition":
              'attachment; filename="heimdall-evidence.md"',
          },
        });
      }

      default:
        return buildResponse({
          name: "Heimdall L7 Monitoring",
          endpoints: [
            "/health",
            "/metrics",
            "/outcomes",
            "/decisions",
            "/events (SSE)",
            "/evidence (Markdown report download)",
          ],
        });
    }
  },
});

console.log(`Heimdall L7 Monitoring running on http://localhost:${PORT}`);
connectToL5();
