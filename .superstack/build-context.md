review:
  security_score: A
  quality_score: A
  findings:
    - severity: low
      category: evidence
      description: core/final_lifecycle.log contains older entries from before the production fixes. A fresh run will populate three-field failure classification (failure_type, failure_stage, recovery) in all entries.
      fix: Re-run Heimdall on mainnet with BLOCKHASH_MODE=normal for 10 success entries, then BLOCKHASH_MODE=fault_injected for 2 intentional failure entries. This will regenerate the lifecycle log with the new format.
  ready_for_mainnet: true
