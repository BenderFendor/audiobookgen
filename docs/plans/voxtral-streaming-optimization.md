# Plan: Voxtral streaming narration and speed optimization

This plan covers making Voxtral INT4 narration feel live: click any sentence
and playback starts near-instantly, stays ahead of the listener, and never
stutters. Workstream A is a just-in-time streaming scheduler; workstream B is
raw throughput on the RTX 3060, ordered profile-first with ConvRot W4A4 as a
measured later phase, never a default. Status: DRAFT — iterate here.

## 1. Where we are today (measured, 2026-07-16, RTX 3060 12 GB)

Source: `reports/benchmarks/voxtral-rtx3060-2026-07-16.md`, PR #3.

| Metric | Value | Meaning |
|---|---|---|
| Decode-loop speed (Compatibility) | 17.7 FPS | Model emits 12.5 frames per audio-second; the loop alone is 1.4x realtime |
| Model-time RTF | 0.71 | Decode loop only |
| Wall-clock for a 5.36 s sentence | 6.94 s (RTF ~1.29) | Prefill + decode + codec + postprocess — this is why it feels slow |
| Model load | 282.5 s | CPU-first load + per-layer HQQ quantization, every worker start |
| VRAM peak | 3.8 GB | Lots of headroom on 12 GB |

Quantization today: only the 26-layer LLM backbone is quantized (HQQ W4A16,
torchao tinygemm). Acoustic flow transformer and codec are BF16.

Known inefficiency in the frame loop: with `flow_steps=3` and CFG, the
midpoint solver makes **8 separate `predict_velocity()` calls per frame**
(2 intervals × 2 midpoint evaluations × 2 CFG branches) — the in-code
comment claiming 4 is wrong. Each call is batch-1 with fresh allocations
(`randn`, `linspace`, `zeros_like`, `.clone()`, concatenations) every frame.

Architecture today: Rust supervises a JSONL worker; `queue_generation` runs
"current_and_next" or "full_book"; sentences are cached as WAVs; the reader
plays cached sentences and errors with "not generated yet" if you click ahead
of the queue. No playhead-anchored prefetcher, no intra-sentence streaming.

## 2. Goals and done criteria

1. **Click-to-listen latency**: click any sentence → audio starts in < 2 s
   (cold sentence, warm model), < 200 ms if cached.
2. **Gapless playback**: once playing, the buffer never underruns on a full
   chapter.
3. **Throughput**: no FPS number is attached until the M1 profile shows how
   frame time splits among backbone decode, acoustic flow, launch overhead,
   and codec. The functional bar is: wall-clock generation comfortably faster
   than playback so the buffer only grows (wall RTF ≤ ~0.5 as a working
   figure, revisited after profiling).
4. **Cold start**: worker ready in < 20 s instead of 282 s.
5. Proven by the benchmark suite plus a real E2E on a full public-domain
   chapter. No new heavyweight eval infrastructure in the repo (no Whisper/
   UTMOS pipelines); quality gates are deterministic comparisons + listening
   (§5).

## 3. Workstream A — just-in-time streaming scheduler

Core design: **generation is a priority queue anchored to the playhead**, not
a batch job. Playback and generation run concurrently; the scheduler keeps
"seconds of audio buffered ahead of the playhead" above a safety floor.

### A1. Playhead-anchored priority queue (Rust supervisor)

- Priority = distance from the playhead in reading order. Clicking a sentence
  re-anchors the queue: clicked sentence becomes priority 0; in-flight work
  for far-away sentences is cancelled between chunks (hooks already exist).
- Cached sentences are skipped (cache already keyed by narrator profile).
- Background fill: while the buffer is healthy, keep generating forward
  through the chapter, then the book, so long sessions converge to fully
  cached.

### A2. Buffer-health pacing

- Track `buffered_seconds_ahead` = cached duration between playhead and the
  first ungenerated sentence.
- Start playback as soon as the clicked sentence is done; keep generating
  while playing. With wall RTF ≤ 0.5, every played second buys two generated
  seconds, so the buffer grows monotonically.
- If the buffer drops below a floor (e.g. 3 s), show a subtle "buffering"
  state on the next sentence instead of erroring.

### A3. Reader UX

- Sentence click = play from here (sentences are already clickable and
  individually playable). Default play = resume last-listened position, with
  start of the current chapter as fallback for a fresh book.
- First-click latency escape hatch: if a cold click can't hit < 2 s, the
  scheduler may synthesize the first sentence on Compatibility while the
  lookahead buffer uses Balanced (decided; only if needed after M3).
- Background fill runs to the entire book while listening — the GPU stays
  busy and the book converges to fully cached.
- Highlight follows playback (exists). The "not generated yet" error is
  replaced by the scheduler: clicking always works; worst case you wait
  time-to-first-audio.

### A4. Intra-sentence streaming (phase 2, optional)

Frames are autoregressive and the codec is convolutional, so codec decode can
run on partial code chunks (e.g. every 25 frames ≈ 2 s) with an overlap
window, handing PCM to playback before the sentence finishes. Cuts
time-to-first-audio for long sentences from `sentence_duration × RTF` to
roughly `chunk_duration × RTF + codec_time`. Risks: seam artifacts at chunk
boundaries (needs overlap-add validation); the end-of-audio token makes the
last chunk short. Only if A1–A3 + workstream B miss the < 2 s goal.

## 4. Workstream B — raw throughput

Profile-first ordering. Phases B1–B4 are no-quality-loss changes (identical
math, identical weights); the ConvRot phase (B5) comes only after they land
and only where a benchmark says it wins.

### B0. Profile before touching anything

Nsight Systems + PyTorch profiler on the 3060 against BF16 and current HQQ
baselines. Deliverable: a frame-time breakdown — backbone decode step,
acoustic flow solver, kernel-launch overhead, codec — checked into
`reports/benchmarks/`. Every later phase cites this breakdown.

### B1. Persist the quantized model (cold start: 282 s → seconds)

The 282 s is CPU load + per-layer HQQ quantization repeated every start.
Serialize the quantized state to disk next to the weights, keyed by (model
checksum, torchao/HQQ versions, quant config). Load becomes mmap → CUDA.
Zero quality risk; biggest single papercut.

### B2. Batch the CFG conditional/unconditional passes

Turn the 8 batch-1 `predict_velocity()` calls per frame into 4 batch-2 calls:
concatenate `(x_t, x_t)` and `(llm_hidden, zeros)` on the batch dim, run one
forward, chunk, blend with the same CFG equation. The acoustic transformer
already propagates batch size, so no architectural change. Bitwise-equivalent
math up to reduction order; on a 3060 batch-2 should also occupy the GPU
better than two serial batch-1 calls.

### B3. Capture the entire acoustic solver in one CUDA graph

Today only `predict_velocity()` is compiled; the Python loop, tensor
creation, CFG arithmetic, midpoint updates, and FSQ quantization live outside
the graphed region. The Voxtral paper identifies the flow transformer as the
central bottleneck and got its latency wins by capturing the complete ODE
solver, not isolated calls. Graph the whole per-frame solve:

- fixed timestep tensors, persistent noise buffer, batched cond/uncond
  hidden-state buffer;
- both midpoint evaluations + CFG blending;
- FSQ clamp/scale/round/code conversion;
- persistent output and working buffers.

Per-frame interface becomes `copy inputs → graph.replay() → copy codes out`.
No change to weights, CFG, solver order, or numerics.

### B4. Remove frame-loop allocations; overlap codec decode

- Replace per-frame `randn`/`linspace`/`zeros_like`/`.clone()`/concats with
  persistent buffers (`normal_()` into an existing tensor keeps the same
  distribution). Required for graph capture anyway; synchronize only when
  required.
- Codec: decode completed chunks on a side CUDA stream while the backbone
  keeps producing codes, instead of the current serial tail after all frames.
  Mainly cuts end-to-end latency, not FPS; combines naturally with A4.

### B5. ConvRot W4A4 for the backbone (Ampere kernel, measured dispatch)

Context: ConvRot (rotation-based plug-and-play 4-bit quantization,
arXiv:2512.03673) enables W4A4 by applying a block-Hadamard rotation H so
`y ≈ Dequant(Q4(xH) · Q4(HᵀW))`. **Do not transplant the public repo**: its
fast path is NVFP4/Blackwell; its fallback rotates activations then calls a
normal linear, which would make Voxtral slower. The useful part is the idea,
implemented as a fused SM86 kernel.

Why it might not pay here: ConvRot's headline results are diffusion
transformers with many tokens per launch; Voxtral decode is M=1 (M=2 after
B2). Weights are already 4-bit, so W4A4 buys no weight-traffic reduction over
W4A16 — the gain must come from faster INT4×INT4 tensor-core compute beating
tinygemm at tiny M. APEX4 (arXiv:2606.08761) shows W4A4 can win on consumer
Ampere (3090), but that guarantees nothing at M=1 on a 3060.

Favorable geometry: model width 3,072, attention projection 4,096, FFN 9,216
— all divide by 256, so block rotations of 64 or 256 need no padding.

Approach:

1. **Offline**: fixed signed block-Hadamard H per selected layer; rotate the
   weight input side (`W_rot = WHᵀ`); HQQ-style optimized scales/zeros; pack
   in the SM86 kernel's layout; store rotation seed/signs + scales. Runtime
   never rotates weights.
2. **Runtime fused kernel**: load BF16 activations → in-register/shared-mem
   Hadamard (64 or 256) → per-token/group activation scale → pack signed
   INT4 → INT4×INT4 tensor-core GEMM → INT32 accumulate → apply scales →
   BF16 out. Rotated activations never touch global memory (standalone
   rotation roughly doubles activation traffic).
3. **Two kernel families**: a decode kernel (M=1/2, low launch overhead,
   weight-streaming, shape-specialized, persistent/grouped where practical)
   and a prefill kernel (larger M, standard tiled INT4 GEMM, easier W4A4
   wins). One generic Triton kernel will not be optimal for both.
4. **Hybrid layer selection**, not uniform W4A4:

   | Component | Initial format |
   |---|---|
   | FFN `w1`/`w2`/`w3` | ConvRot W4A4 candidate (most params + FLOPs per block) |
   | Attention `wq`, `wo` | Benchmark W4A4 vs HQQ |
   | Attention `wk`, `wv` | Keep HQQ W4A16 initially |
   | Embeddings, final norm, semantic head | BF16 |
   | Acoustic transformer, codec | BF16 initially; ConvRot investigated last |

5. **Dispatch rule**: per linear layer, use ConvRot W4A4 only when
   `measured_convrot_latency(shape) < measured_hqq_tinygemm_latency(shape)`.
   No architectural-purity override.
6. Before any kernel work: a standalone quantization-error simulator to test
   rotation sizes 64 vs 256 on FFN activations and pick mixed-precision
   layers by measured quality sensitivity.

### Explicitly not doing

- Transplanting the ConvRot repository or its BF16 fallback path.
- Speculative decoding of the backbone.
- Lowering CFG below 1.2 (existing hard rule).
- Whisper/UTMOS/eval-model pipelines in the repo (see §5).
- Batching multiple sentences per forward (breaks the latency model;
  revisit only for offline full-book export).

## 5. Measurement and quality gates (lightweight, no new eval fluff)

Extend the existing `benchmark_voxtral.py` into a fixed suite; no new eval
dependencies or models added to the repo.

- **Corpus**: 5 fixed sentences (short/medium/long/dialogue/numbers) + 1
  public-domain chapter; fixed voice, seed, solver config, CFG, and initial
  noise for every comparison.
- **Metrics per run**: decode FPS, wall RTF, time-to-first-audio, cold-start
  seconds, VRAM peak → JSON + Markdown in `reports/benchmarks/`.
- **Speed ledger**: `reports/benchmarks/SPEEDLOG.md` — one row per landed
  optimization: date, change, commit, and before → after for each metric on
  the fixed corpus (same seed/voice/profile). Every B-phase change adds a row
  before it merges; the ledger is the single answer to "what did the cheap
  wins get us." The M1 baseline is row zero.
- **Gates for math-identical changes (B1–B4)**: existing regression + GPU
  test suites pass; outputs numerically match baseline within reduction-order
  tolerance on the fixed corpus; no change in premature-termination or
  frame-limit behavior. That's sufficient — these change no weights or math.
- **Gates for quantization changes (B5)**: everything above, plus spectral
  distance vs baseline on the fixed corpus, long-form numerical stability,
  and blinded A/B listening on the chapter. Transcription/naturalness/speaker
  scoring stays *outside* the repo — if we want it, it's a one-off manual
  check, not committed infrastructure.

## 6. Milestones

Status 2026-07-16: M1-M4 landed (tags `voxtral-speed-m1-m2`,
`voxtral-speed-m3-b2`, `voxtral-speed-m3-b3a`, `voxtral-speed-m4-scheduler`).
Cold start 5 s, Balanced wall RTF 0.45 at 32 FPS, click-anywhere streaming
playback wired. Outstanding: B2 listening gate, live desktop E2E. B3/B4 full
CUDA-graph capture deferred (measured loop overhead ~0); ConvRot (M5) still
plan-only pending a re-profile.

1. **M1 — Profile + baseline suite** (B0): frame-time breakdown and baseline
   numbers for all three profiles, including wall RTF and time-to-first-audio.
2. **M2 — Cold start** (B1): worker ready < 20 s.
3. **M3 — Solver speed** (B2 → B3 → B4): batched CFG, full-solver CUDA graph,
   allocation-free frame loop, codec overlap. Re-profile; set the real FPS
   target here.
4. **M4 — Streaming scheduler** (A1–A3): click-to-listen < 2 s warm, gapless
   full-chapter playback proven by E2E.
5. **M5 — ConvRot phase** (B5): **plan-only for now.** Committed only if the
   M3 re-profile shows backbone GEMMs still dominate frame time. When it
   runs: simulator → mixed-precision selection → SM86 decode kernel →
   prefill kernel → per-shape dispatch; only layers that win the benchmark
   ship; acoustic transformer investigated last.
6. **M6 — Stretch** (A4 intra-sentence streaming): only if M3/M4 miss the
   latency goal.

Expected safest first win (per Jordan's analysis): batched CFG + full-solver
CUDA graphing. Expected eventual backend: hybrid HQQ W4A16 + ConvRot W4A4,
not uniform ConvRot.

## 7. Resolved decisions

- "ConvRot" = rotation-based W4A4 (arXiv:2512.03673), not Nunchaku/SVDQuant.
  Repo is not transplanted; the idea is reimplemented as a fused SM86 kernel
  behind a measured dispatch rule.
- ConvRot phase is plan-only until the M3 re-profile justifies it.
- No Whisper/UTMOS/eval-model infrastructure lands in the repo.
- No FPS target until M1 profiling shows the frame-time breakdown.
- First click may mix profiles (Compatibility first sentence, Balanced
  buffer) if needed to hit < 2 s after M3.
- Default play = resume last position; chapter start as fallback.
- Background generation fills the entire book while listening.
