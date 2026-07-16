# Worksheet: Voxtral speed M3-B3a (kill compile stalls on Balanced)

**Goal**: Remove the multi-second per-sentence prefill recompiles and shrink
the per-process compile warmup on the Balanced profile
(`docs/plans/voxtral-streaming-optimization.md` workstream B3, first slice).

**Files changed**:
- `services/tts-worker/audiobookgen_worker/voxtral_int4/inference.py` —
  prefill always runs the eager backbone (`_orig_mod` when compiled): the
  backbone specializes shapes, so compiled prefill recompiled per prompt
  length (`mark_dynamic` fails with ConstraintViolationError). `freqs_cis`
  is now allocated once at a fixed size (≥1280) instead of per-sentence:
  the compiled decode step guards on its shape, and a per-sentence size
  forced one more multi-second recompile.
- `services/tts-worker/audiobookgen_worker/voxtral_int4/runtime.py` —
  `TORCHINDUCTOR_CACHE_DIR` defaults to `<model dir>/inductor-cache` so
  compiled kernels persist across processes.

**Commands run**: balanced suite before/after
(`voxtral-b3a-eager-prefill-*`, `voxtral-b3a2-fixed-freqs-*`), warmup-cache
recheck, CPU tests (10 passed).

**Tests added**: none — measured by the benchmark suite.

**Assumptions**: static KV cache (900) still bounds sequence length;
freqs_cis is sliced per position so a larger table changes no values.
Outputs differ from the B2 run only via eager-vs-compiled prefill numerics
(same autoregressive-divergence class already covered by the B2 listening
gate).

**Risk tier**: low-medium.

**Rollback**: revert the two files to tag `voxtral-speed-m3-b2`.

**Status**: done. Balanced: mean wall RTF 0.45, 32 FPS, prefill ~0.3 s on
every sentence, compile warmup 15 s per process with warm cache. M3
functional bar (wall RTF ≤ 0.5) met; full-solver CUDA-graph capture (B3/B4)
deferred — measured loop overhead is ~0, so expected gain no longer
justifies it before M4.
