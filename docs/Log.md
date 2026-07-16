# Development log

This log records user-visible behavior, architecture, setup, and verification
changes so release notes remain grounded in repository artifacts.

## 2026-07-16 — Voxtral speed program: solver speedups and streaming playback

- Batched CFG conditional/unconditional velocity passes (8 batch-1 → 4
  batch-2 acoustic calls per frame): compatibility 16→19.5 FPS, quality
  7.6→11.3, balanced 23→33 FPS on the RTX 3060. Blinded listening vs the
  baseline WAVs is the outstanding acceptance gate.
- Eliminated Balanced compile stalls: eager prefill, fixed-size freqs_cis,
  persistent inductor cache. Every sentence now prefills in ~0.3 s; mean
  wall RTF 0.45 at 32 FPS; compile warmup 64 s → 15 s per process.
- Playhead-anchored streaming playback: clicking any sentence anchors the
  generation queue there and playback starts as soon as that sentence is
  ready ("Preparing narration…" instead of the old "not generated yet"
  dead-end). A full-book fill job follows the playhead, wraps to earlier
  text once everything ahead is generated, and skips cached sentences. New
  commands: `anchor_generation`, `set_generation_anchor`.
- Ledger: `reports/benchmarks/SPEEDLOG.md`. Remaining release checks: B2
  listening gate; live desktop click-to-play session on real hardware.

## 2026-07-16 — Voxtral speed program: benchmark suite and quantized cache

- Added `--suite` mode to `benchmark_voxtral.py`: fixed five-sentence corpus,
  per-phase timing breakdown (prefill / backbone / acoustic solver / loop
  overhead / codec) via CUDA events, aggregate FPS and wall-RTF per profile.
- Added `reports/benchmarks/SPEEDLOG.md`, the running before/after ledger for
  every Voxtral optimization.
- Added a quantized-weight disk cache under `<model dir>/quantized-cache/`;
  first load quantizes and writes it, later loads restore straight to CUDA.
  Stale or unreadable caches fall back to the slow path and are rewritten.
- Plan: `docs/plans/voxtral-streaming-optimization.md`.

## 2026-07-16 — Direct Voxtral 4B on 12 GB CUDA

- Replaced the temporary managed vLLM server design with one supervised stdio worker and direct selective HQQ INT4 inference.
- Added CPU-first safetensors loading, per-layer backbone quantization, BF16 acoustic/codec stages, CFG-safe profiles, static-cache reset, typed audio, and typed worker failures.
- Repaired the upstream 24/48 kHz mismatch and added WAV rate/duration regression coverage.
- Added explicit CC BY-NC 4.0 acceptance, installed-voice enumeration, CUDA compatibility status, download/load/generation progress, and a stop-to-free-VRAM action.
- Measured a real RTX 3060 12 GB compatibility run in `reports/benchmarks/voxtral-rtx3060-2026-07-16.md`.
