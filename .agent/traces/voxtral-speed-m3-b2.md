# Worksheet: Voxtral speed M3-B2 (batched CFG)

**Goal**: Halve the number of acoustic forward passes per frame by running
the CFG conditional and unconditional branches as one batch-2 forward
(`docs/plans/voxtral-streaming-optimization.md` workstream B2).

**Files changed**:
- `services/tts-worker/audiobookgen_worker/voxtral_int4/inference.py` — new
  `_predict_velocity_cfg` helper; midpoint solver now makes 4 batch-2 calls
  per frame instead of 8 batch-1 calls; corrected the stale "4 passes"
  comment.

**Commands run**:
- `benchmark_voxtral.py --suite --profiles compatibility,quality,balanced`
  → `reports/benchmarks/voxtral-b2-batched-cfg-2026-07-16.md`
- spectral/duration comparison vs baseline WAVs (scratchpad script)
- hardware-gated GPU test (`test_voxtral_gpu`, 1 test, OK)
- CPU suite: 10 passed

**Tests added**: none — covered by the benchmark suite, the GPU invariant
test, and the SPEEDLOG comparison protocol.

**Assumptions**:
- The acoustic transformer treats batch rows independently, so batch-2 CFG
  is the same equation. Confirmed by healthy outputs, but exact floats
  differ (matmul reduction order), and the autoregressive loop amplifies
  that into a different rendering: durations shift up to 4 s on the fixed
  corpus, relative spectral L1 0.8-2.6 vs baseline.

**Risk tier**: medium — measured +21% (compatibility), +49% (quality),
+42% (balanced) FPS, but the output is a different sample of the model's
distribution. Blinded listening vs the baseline WAVs is the outstanding
acceptance gate; WAV pairs live in
`reports/benchmarks/voxtral-baseline-suite-2026-07-16-wavs/` and
`reports/benchmarks/voxtral-b2-batched-cfg-2026-07-16-wavs/`.

**Rollback**: revert inference.py to tag `voxtral-speed-m1-m2`.

**Status**: done (code + measurement); listening gate pending user.
