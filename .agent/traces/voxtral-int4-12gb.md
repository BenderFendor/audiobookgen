# Voxtral INT4 12 GB integration worksheet

**Goal:** Replace the temporary manually managed/vLLM Voxtral path with a backend-owned direct selective-HQQ-INT4 worker, install it for CUDA, prove it on an RTX 3060 12 GB, preserve 48 kHz audio through export, and surface model/generation progress.

**Files changed:**

- `README.md`
- `THIRD_PARTY_NOTICES.md`
- `apps/desktop/src-tauri/src/commands.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/voxtral.rs`
- `crates/audiobookgen-core/src/db.rs`
- `crates/audiobookgen-core/src/export.rs`
- `crates/audiobookgen-core/src/model.rs`
- `crates/audiobookgen-core/src/worker.rs`
- `docs/ARCHITECTURE.md`
- `docs/Log.md`
- `docs/PLATFORMS.md`
- `docs/TESTING.md`
- `docs/VOXTRAL.md`
- `papercuts.md`
- `reports/benchmarks/voxtral-rtx3060-2026-07-16.json`
- `reports/benchmarks/voxtral-rtx3060-2026-07-16.md`
- `reports/screenshots/voxtral-models.png`
- `scripts/e2e_voxtral_worker.py`
- `services/tts-worker/audiobookgen_worker/engines/base.py`
- `services/tts-worker/audiobookgen_worker/engines/kokoro.py`
- `services/tts-worker/audiobookgen_worker/engines/maya1.py`
- `services/tts-worker/audiobookgen_worker/engines/voxtral.py`
- `services/tts-worker/audiobookgen_worker/main.py`
- `services/tts-worker/audiobookgen_worker/protocol.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/__init__.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/audio.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/errors.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/inference.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/load_model.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/model.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/runtime.py`
- `services/tts-worker/audiobookgen_worker/voxtral_int4/tekken.py`
- `services/tts-worker/pyproject.toml`
- `services/tts-worker/scripts/benchmark_voxtral.py`
- `services/tts-worker/tests/gpu/test_voxtral_gpu.py`
- `services/tts-worker/tests/test_progress.py`
- `services/tts-worker/tests/test_voxtral_int4.py`
- `services/tts-worker/tests/test_worker.py`
- `services/tts-worker/uv.lock`
- `src/app/globals.css`
- `src/components/AppShell.tsx`
- `src/components/ModelsView.tsx`
- `src/components/ReaderStudio.tsx`
- `src/lib/tauri.ts`
- `src/lib/types.ts`
- `src/lib/voices.ts`
- `.agent/traces/managed-voxtral-cargo-clippy.json`
- `.agent/traces/managed-voxtral-cargo-test.json`
- `.agent/traces/managed-voxtral-npm-build.json`
- `.agent/traces/managed-voxtral-npm-test.json`
- `.agent/traces/maya1-cuda-install.json`
- `.agent/traces/voxtral-int4-runtime-install.md`
- `.agent/traces/voxtral-model-download.md`
- `.agent/traces/voxtral-rtx3060-gpu-tests.md`
- `.agent/traces/voxtral-worker-e2e.md`
- `.agent/traces/voxtral-int4-12gb.md`

**Commands run:**

- Installed `services/tts-worker[voxtral]` with uv into the managed worker venv: passed; PyTorch 2.13.0+cu130, torchao 0.17.0+cu130, HQQ 0.2.8.post1; Maya1 CUDA GPU offload remained enabled.
- Downloaded pinned Hugging Face revision `b81be46c3777f88621676791b512bb01dc1cb970`: passed; weights SHA-256 `66c4fd998db10e1a6d9cc5baa10e6264bf10701ec22ccdc0822c7dcc45dbe55b`.
- Direct RTX 3060 compatibility generation: passed; mono 48 kHz, 5.36 s output, 3.812 GB peak PyTorch allocation.
- Hardware-gated GPU suite: passed in 320.149 s; repeated Compatibility, Quality, and compiled Balanced outputs; memory-growth and 10.5 GB peak assertions passed.
- Production JSONL Voxtral worker E2E: first run exposed stdout protocol pollution; fixed, rerun passed in 85.075 s with structured progress and a 5.36 s mono 48 kHz WAV.
- `npm run tauri -- dev`: passed; desktop binary launched and Models page rendered installed CUDA/Voxtral state. Chrome MCP was unavailable because Chrome was not running; actual Tauri window was captured and inspected.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace --all-targets`: passed, 14 tests total across suites.
- `npm run typecheck`: passed.
- `npm test`: passed, 17 tests.
- `npm run build`: passed; static export built.
- `uv lock --project services/tts-worker --check`: passed, 139 packages resolved.
- `pytest services/tts-worker/tests -q`: passed, 10 CPU tests and 1 hardware-gated skip.
- Ruff check and format check over worker and E2E script: passed.
- `python3 scripts/validate_repo.py`: passed.
- `python3 scripts/e2e_mock_worker.py`: passed.
- `python3 scripts/e2e_real_worker.py`: passed, real Kokoro output at 24 kHz.
- `git diff --check`: passed.

**Tests added:**

- Sample-rate propagation and WAV duration regression for the upstream 24/48 kHz defect.
- All production Voxtral profiles retain CFG 1.2.
- Parent-path traversal rejection.
- Hardware-gated real GPU profiles, static-cache reset/repeat generation, and memory stability.
- Real JSONL worker E2E proving progress, response metadata, and mono 48 kHz serialization.
- Export rate selection ignores legacy 24 kHz scene-break placeholders for 48 kHz narration.
- NVIDIA 12 GB/compute-capability compatibility status.

**Assumptions:** The pinned official model revision remains distributable under its displayed CC BY-NC 4.0 terms. The measured RTX 3060 result supports 12 GB Ampere but does not prove sub-12-GB cards or every driver/PyTorch combination. Automated waveform checks do not replace human listening; exact cross-device determinism is not claimed. One active GPU inference request is intentional.

**Risk tier:** medium

**Rollback:** Revert the feature commit and delete the external `/mnt/Big storage/AudiobookGen/models/voxtral-4b-tts` snapshot plus the managed worker venv if the installed CUDA dependencies must also be removed. Existing imported EPUBs and generated Kokoro/Maya1 audio are unaffected.

**Status:** done
