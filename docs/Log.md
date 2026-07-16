# Development log

This log records user-visible behavior, architecture, setup, and verification
changes so release notes remain grounded in repository artifacts.

## 2026-07-16 — Direct Voxtral 4B on 12 GB CUDA

- Replaced the temporary managed vLLM server design with one supervised stdio worker and direct selective HQQ INT4 inference.
- Added CPU-first safetensors loading, per-layer backbone quantization, BF16 acoustic/codec stages, CFG-safe profiles, static-cache reset, typed audio, and typed worker failures.
- Repaired the upstream 24/48 kHz mismatch and added WAV rate/duration regression coverage.
- Added explicit CC BY-NC 4.0 acceptance, installed-voice enumeration, CUDA compatibility status, download/load/generation progress, and a stop-to-free-VRAM action.
- Measured a real RTX 3060 12 GB compatibility run in `reports/benchmarks/voxtral-rtx3060-2026-07-16.md`.
