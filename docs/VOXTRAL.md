# Voxtral 4B INT4 integration

This document covers Voxtral installation, architecture, licenses, 12 GB GPU
profiles, benchmarking, troubleshooting, and the repaired 48 kHz audio contract.

## Why this is not vLLM-Omni

Mistral's official vLLM deployment path recommends at least 16 GB VRAM. AudiobookGen instead adapts the custom PyTorch reconstruction in [`TheMHD1/voxtral-int4`](https://github.com/TheMHD1/voxtral-int4) at commit `93d3e21`. It reconstructs the language backbone, acoustic flow transformer, codec, tokenizer, voice injection, static GQA cache, and optimized generation path directly. No HTTP server is opened; Rust supervises the newline-delimited JSON worker and serializes all model calls.

## 12 GB loading path

The official safetensors state dictionary is loaded on CPU. Each of the 26 language-backbone layers moves to CUDA and is quantized independently with torchao `Int4WeightOnlyConfig`, HQQ parameter selection, group size 64, and `tile_packed_to_4d`. Already compressed layers remain on CUDA while unprocessed BF16 layers stay on CPU. Only after compression are the acoustic transformer, codec, embeddings, and normalization layers moved to CUDA in BF16.

This avoids the reference implementation's temporary full-BF16 CUDA peak. The model-level request gate permits one active inference call, and every request resets the static KV cache. Stopping the narration worker unloads all engines and releases GPU memory.

Profiles:

- Balanced: three midpoint flow steps, CFG 1.2, compilation enabled after compatibility checks, 500-frame ceiling.
- Quality: eight flow steps, CFG 1.2, no compilation, 700-frame ceiling.
- Compatibility: three flow steps, CFG 1.2, no compilation, 350-frame ceiling.

CFG 1.0 is not a production speed mode. An OOM never changes CFG.

## Installation and licenses

The Models screen reports GPU name, VRAM, and compute capability before installation. The user must explicitly accept CC BY-NC 4.0 before the official model snapshot and its 20 reference voice embeddings are downloaded. Model weights live under the configured models directory, outside the repository. The adapted reference inference code is MIT according to its upstream README; torchao and HQQ retain their own licenses.

The worker environment is created with Python 3.12 and uv's automatic PyTorch backend selection. The Voxtral extra declares PyTorch, torchao, HQQ, safetensors, SciPy, tiktoken, NumPy, SoundFile, and Hugging Face Hub through direct or base dependencies.

## Repaired sample-rate contract

The reference generator decodes at 24 kHz, low-pass filters, resamples to 48 kHz, and peak-normalizes. Several reference callers then labeled the post-processed array as 24 kHz, causing half-speed, low-pitch playback. AudiobookGen returns a typed `GeneratedAudio` carrying samples and sample rate together; duration is always `len(samples) / sample_rate`, and WAV serialization uses the carried 48 kHz value. `test_voxtral_int4.py` locks this behavior with a one-second sine-wave regression.

## Benchmark

Run on a CUDA host:

```bash
PYTHONPATH=services/tts-worker \
  .venv/bin/python services/tts-worker/scripts/benchmark_voxtral.py \
  --model-dir /path/to/voxtral-4b-tts \
  --output reports/benchmarks/voxtral-local.json \
  --profile compatibility
```

The command emits JSON and Markdown with model checksum, software versions, GPU/driver, profile parameters, duration, FPS, real-time factor, PyTorch allocated/reserved/peak memory, external GPU memory, and basic waveform validation. Do not compare every GPU against the reference repository's RTX 3090 FPS.

## Troubleshooting

- CUDA unavailable: install a CUDA-capable PyTorch wheel; the Models screen must not report support until `torch.cuda.is_available()` succeeds.
- Compute capability below 8.0: use Kokoro or Maya1; this INT4 packing path is unsupported.
- Less than 12 GB VRAM: blocked because no measured lower-memory profile exists.
- OOM: close other GPU programs, use Compatibility, and reduce per-sentence frame limits. Never lower CFG.
- Compile failure: select Compatibility and retain the failure details; it uses the same quantization and CFG without `torch.compile`.
- Slow or muffled output: inspect the WAV header. Voxtral post-processed files must be mono 48 kHz, never a 48 kHz array labeled as 24 kHz.
- Missing end-of-audio or invalid output: the sentence remains failed and retryable; the book is not marked complete.

## Known limitations

Voxtral speed is currently fixed at 1.0 and unsupported values are rejected. Word timing is estimated; sentence timing uses actual generated duration. Balanced compilation has not yet been accepted as a release default on every CUDA/PyTorch combination, so Compatibility remains the diagnostic fallback. Exact cross-device determinism is not claimed.
