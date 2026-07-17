# Development log

This log records user-visible behavior, architecture, setup, and verification
changes so release notes remain grounded in repository artifacts.

## 2026-07-16 — M7 first-sound-fast + M8 product-path harness

Implements the first slice of `docs/plans/product-hardening.md` (M7, M8).

- **DF-1**: `voxtral_cache_discriminator` no longer re-hashes the 8+ GB
  weights file or the voice embedding on every `run_generation` call; it uses
  `(size, mtime)` identity instead, mirroring the quantized-weight cache's own
  discriminator. Measured on this machine against the real installed model:
  **83 s → 903 µs**. `file_sha256` removed (dead code).
- **DF-9**: first-click profile mixing. `anchor_generation` (fired only when
  a click found no cached audio) marks that one fragment "urgent"; if the
  book's configured Voxtral profile isn't already `compatibility`,
  `run_generation` generates exactly that fragment on `compatibility` to skip
  the Balanced/Quality `torch.compile` stall, while the rest of the buffer
  keeps the configured profile. The override is folded into the cache key, so
  a later non-urgent pass naturally regenerates the sentence at full quality
  (cache-key mismatch triggers regeneration, same as any other parameter
  change).
- **DF-24 (late validation)**: `create_narration_profile` now rejects Voxtral
  voice names that aren't in the installed embedding directory, instead of
  accepting any string up to 80 characters. `list_engine_status` and profile
  creation now share one `installed_voxtral_voices` helper.
- **DF-8**: the reader's wait-for-narration loop now surfaces the worker's
  live progress state (`"Voxtral worker: quantizing backbone layer 12/26"`,
  etc.) instead of a static "Preparing narration…" message, and the give-up
  condition changed from a fixed 180 s total wait to "no new progress event
  for 120 s" — a slow-but-progressing job (quant-cache rebuild, cold compile)
  no longer gets killed just for taking a while.
- **M8**: the `AppHandle`-consuming functions in `commands.rs`
  (`emit_model_progress`, `download_engine_model`, `preview_voice`,
  `queue_generation`, `anchor_generation`, `spawn_generation`,
  `run_generation`) are now generic over `R: tauri::Runtime`, enabling a
  headless product-path test against `tauri::test`'s `MockRuntime`:
  `commands::product_path_tests::mock_generation_reaches_every_fragment_through_the_real_commands`
  builds a fixture EPUB, imports it, and drives `run_generation` with
  `AUDIOBOOKGEN_WORKER_MOCK=1`, asserting every fragment gets a cached
  segment. This is the first E2E that exercises the Rust orchestration layer
  (prior E2E coverage — `voxtral-worker-e2e` — only drove the Python worker
  protocol directly, which is exactly how DF-1 went unnoticed). An
  `#[ignore]`d companion test measures the real discriminator against the
  actual installed Voxtral weights on GPU-equipped machines.
- Not done in this pass (remaining M7/M8 scope, tracked in the plan): the
  full product-path measurement protocol's five latency numbers on real
  hardware, the blinded listening gate (old plan's B2), and CI wiring for the
  new mock-engine test.

## 2026-07-16 — Product audit and hardening plan

- Live-session audit found the first-click Voxtral path broken in practice:
  an 8 GB weights re-hash at every generation job start (measured 83 s on
  the HDD models volume) plus quant-cache load and the Balanced compile
  stall exceed the reader's 180 s give-up deadline. Root process cause: all
  speed benchmarks bypass the Rust command path, so the regression was
  invisible to the ledger.
- Wrote `docs/plans/product-hardening.md`: a 36-entry design-flaw register
  (orchestration, playback engine, reader, narrator data model, worker
  protocol, storage, verification) and milestones M7-M16 extending the
  Voxtral speed plan, including the outstanding B2 listening gate and a
  product-path E2E harness as the new evidence bar. No code changed.

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
