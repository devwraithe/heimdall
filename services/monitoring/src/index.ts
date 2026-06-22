import dotenv from "dotenv";
import path from "path";
dotenv.config({ path: path.resolve("../.env") });

import { connectToL5 } from "./client";
import { getMetrics } from "./store";

const PORT = 3000;

function buildResponse(data: unknown): Response {
  return new Response(JSON.stringify(data, null, 2), {
    headers: { "Content-Type": "application/json" },
  });
}

const server = Bun.serve({
  port: PORT,
  fetch(req) {
    const url = new URL(req.url);

    switch (url.pathname) {
      case "/health":
        return buildResponse({
          status: "ok",
          uptime_seconds: getMetrics().uptimeSeconds,
          started_at: new Date(getMetrics().startedAt).toISOString(),
        });

      case "/metrics":
        const metrics = getMetrics();
        return buildResponse({
          current_slot: metrics.latestSnapshot?.currentSlot ?? 0,
          latest_finalized_slot:
            metrics.latestSnapshot?.latestFinalizedSlot ?? 0,
          tip_median_lamports: metrics.latestSnapshot?.tipMedianLamports ?? 0,
          active_bundle_count: metrics.latestSnapshot?.activeBundleCount ?? 0,
          total_bundles_failed: metrics.totalBundlesFailed,
          total_bundles_finalized: metrics.totalBundlesFinalized,
          uptime_seconds: metrics.uptimeSeconds,
        });

      case "/outcomes":
        return buildResponse({
          recent_outcomes: getMetrics().latestSnapshot?.recentOutcomes ?? [],
        });

      case "/decisions":
        return buildResponse({
          recent_decisions: getMetrics().recentDecisions,
        });

      default:
        return buildResponse({
          endpoints: ["/health", "/metrics", "/outcomes", "/decisions"],
        });
    }
  },
});

console.log(`Heimdall L7 Monitoring running on http://localhost:${PORT}`);
connectToL5();
