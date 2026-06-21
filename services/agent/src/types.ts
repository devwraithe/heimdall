// Mirrors BundleOutcomeSummary from heimdall.proto
export interface BundleOutcomeSummary {
  bundleId: string;
  slot: number;
  stage: string;
  failureReason: string;
  tipLamports: number;
  blockhash: string;
  submittedAt: number;
}

// Mirrors OperationalSnapshot from heimdall.proto
export interface OperationalSnapshot {
  currentSlot: number;
  latestFinalizedSlot: number;
  tipMedianLamports: number;
  recentOutcomes: BundleOutcomeSummary[];
  activeBundleCount: number;
  snapshotAt: number;
}

// The agent's retry decision
export interface RetryDecision {
  shouldRetry: boolean;
  reason: string;
  refreshBlockhash: boolean;
  suggestedTipLamports: number;
  failureClassification: string;
}
