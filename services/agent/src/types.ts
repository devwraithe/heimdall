// Mirrors BundleOutcomeSummary from heimdall.proto
export interface BundleOutcomeSummary {
  bundleId: string;
  slot: number;
  stage: string;
  failureReason: string;
  failureStage: string;
  recovery: string;
  tipLamports: number;
  blockhash: string;
  submittedAt: number;
  // Retry lineage
  originalBundleId: string;
  retryAttempt: number;
}

// Mirrors OperationalSnapshot from heimdall.proto
export interface OperationalSnapshot {
  currentSlot: number;
  latestFinalizedSlot: number;
  tipMedianLamports: number;
  recentOutcomes: BundleOutcomeSummary[];
  activeBundleCount: number;
  snapshotAt: number;
  // Retry metrics
  totalRetries: number;
  totalRetriesSucceeded: number;
  totalRetriesExhausted: number;
}

// The agent's retry decision
export interface RetryDecision {
  shouldRetry: boolean;
  reason: string;
  refreshBlockhash: boolean;
  suggestedTipLamports: number;
  failureClassification: string;
  confidence: number;
  observedRisk: string;
}

export interface AgentDecision extends RetryDecision {
  timestamp: number;
  source: "local_rules" | "gemini" | "fallback";
}

export type RetryRequest = {
  failedBundles: BundleOutcomeSummary[];
  refreshBlockhash: boolean;
  suggestedTipLamports: number;
  failureClassification: string;
  confidence: number;
  observedRisk: string;
};

export type RetryResponse = {
  accepted: boolean;
  message: string;
};
