# Worksheet: Voxtral speed M1 (baseline suite) + M2 (quantized cache)

**Goal**: Execute milestones M1 and M2 of
`docs/plans/voxtral-streaming-optimization.md`: a fixed-corpus benchmark
suite with per-phase timing breakdown, the SPEEDLOG ledger with a baseline
row, and a quantized-weight disk cache cutting the 282 s cold start.

**Files changed**:
- `services/tts-worker/audiobookgen_worker/voxtral_int4/audio.py` — optional
  `timings` field on `GeneratedAudio`.
- `services/tts-worker/audiobookgen_worker/voxtral_int4/inference.py` —
  opt-in per-phase timing (setup, prefill, decode loop split into backbone /
  acoustic / overhead via CUDA events, codec, postprocess).
- `services/tts-worker/audiobookgen_worker/voxtral_int4/runtime.py` —
  `collect_timings` pass-through; quantized-weight cache (save after slow
  quantize, meta-device + `load_state_dict(assign=True)` fast restore, keyed
  metadata, best-effort failure handling).
- `services/tts-worker/scripts/benchmark_voxtral.py` — `--suite` mode with
  fixed five-sentence corpus, per-profile aggregates, compiled profiles last.
- `reports/benchmarks/SPEEDLOG.md` — new ledger.
- `docs/VOXTRAL.md`, `docs/Log.md` — documented cache + suite.

**Commands run**:
- `pytest tests/ -q --ignore=tests/gpu` (10 passed)
- `benchmark_voxtral.py --suite --profiles compatibility,quality,balanced`
  with the app worker venv (`~/.local/share/io.audiobookgen.desktop/worker-venv`)
  and `PYTHONPATH=services/tts-worker` (repo code shadows installed package)

**Tests added**: none yet (timing is opt-in and exercised by the suite; cache
verified by a real double-load run — see status).

**Assumptions**:
- Weight identity for the cache key uses file size + mtime, not a re-hash of
  the 9 GB safetensors; full checksum was verified at install time.
- Only registered buffers (`inv_freq`, `alibi_slopes`) exist in the model;
  `freqs_cis` is recomputed on demand, so meta-device construction plus
  `assign=True` restore is safe.

**Risk tier**: medium (touches the production load path; cache failures fall
back to the previous slow path).

**Rollback**: delete `<model dir>/quantized-cache/`; revert the four worker
files; SPEEDLOG/docs are additive.

**Status**: done.

- Baseline suite recorded: compatibility 16.1 FPS / wall RTF 1.04 (backbone
  50% / acoustic 50%), quality 7.6 FPS / RTF 1.71 (acoustic 77%), balanced
  23.2 FPS / decode RTF 0.54 (wall skewed by prefill shape recompiles —
  logged as an M3 follow-up). Codec negligible.
- Quantized cache verified end-to-end: cold start 119-165 s slow path →
  5.4 s cached; generated WAV bitwise identical (sha256 d6314f7a…) across
  both paths. Two restore bugs found and fixed during verification:
  `load_state_dict` shape validation rejects the reconstruction's tolerated
  codec qk_norm mismatch (assign by name instead, with a meta-leftover
  check), and torchao subclasses require `map_location="cuda:0"`, not bare
  "cuda".
- 10 CPU worker tests pass; ruff clean.
