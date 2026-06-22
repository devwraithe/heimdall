import type { OperationalSnapshot } from "./types";

export interface AgentDecision {
  timestamp: number;
  shouldRetry: boolean;
  reason: string;
  failureClassification: string;
  refreshBlockhash: boolean;
  suggestedTipLamports: number;
}

export interface MetricsStore {
  latestSnapshot: OperationalSnapshot | null;
  recentDecisions: AgentDecision[];
  totalBundlesSubmitted: number;
  totalBundlesFailed: number;
  totalBundlesFinalized: number;
  uptimeSeconds: number;
  startedAt: number;
}

const store: MetricsStore = {
  latestSnapshot: null,
  recentDecisions: [],
  totalBundlesSubmitted: 0,
  totalBundlesFailed: 0,
  totalBundlesFinalized: 0,
  uptimeSeconds: 0,
  startedAt: Date.now(),
};

const seenOutcomeIds = new Set<string>();

export function updateSnapshot(snapshot: OperationalSnapshot): void {
  store.latestSnapshot = snapshot;
  store.uptimeSeconds = Math.floor((Date.now() - store.startedAt) / 1000);

  for (const outcome of snapshot.recentOutcomes) {
    if (seenOutcomeIds.has(outcome.bundleId)) continue;
    seenOutcomeIds.add(outcome.bundleId);

    if (outcome.stage === "Failed") {
      store.totalBundlesFailed++;
    } else if (outcome.stage === "Finalized") {
      store.totalBundlesFinalized++;
    }
  }
}

export function recordDecision(decision: AgentDecision): void {
  store.recentDecisions.unshift(decision);
  // Keep last 20 decisions
  if (store.recentDecisions.length > 20) {
    store.recentDecisions.pop();
  }
}

export function getMetrics(): MetricsStore {
  return store;
}
