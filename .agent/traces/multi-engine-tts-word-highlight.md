# Multi-engine TTS, word highlighting, Models page

**Goal:** Click/right-click a sentence in the reader to start narration there; highlight the current word during playback; support all 28 Kokoro English narrators; add Maya1 (quantized GGUF) and Voxtral-4B-TTS-2603 engines; store all model weights on `/mnt/Big storage`; add a Models page for download/setup/tuning of each engine.

**Files changed:**
- `services/tts-worker/audiobookgen_worker/engines/{__init__,base,kokoro,maya1,voxtral}.py` (new engine registry; `engine.py` removed)
- `services/tts-worker/audiobookgen_worker/{main,protocol}.py` (engine/options/model_status protocol, word_timings in responses)
- `services/tts-worker/pyproject.toml` (`[maya1]` extra: llama-cpp-python, snac, torch)
- `services/tts-worker/tests/test_worker.py` (timings assertions, multi-engine mock test)
- `crates/audiobookgen-core/src/{model,db,cache,worker}.rs` (AppSettings, WordTiming, profiles.engine + generated_segments.word_timings_json migrations, settings kv table, engine-aware cache key preserving legacy Kokoro keys, wire protocol)
- `crates/audiobookgen-core/tests/{cache_profiles,epub_pipeline}.rs`
- `apps/desktop/src-tauri/src/{commands,lib}.rs` (models root defaulting to `/mnt/Big storage/AudiobookGen/models`, per-engine model dirs + legacy Kokoro fallback, extras-aware venv install with worker restart, list_engine_status / download_engine_model / get_app_settings / update_app_settings / get_generated_segment, per-engine voice validation)
- `src/lib/{types,tauri,voices,reader}.ts`, `src/lib/voices.test.ts`
- `src/components/{AppShell,ReaderStudio,ModelsView}.tsx`, `src/app/globals.css`
- `docs/ARCHITECTURE.md`, `AGENTS.md`

**Commands run:**
- `cargo test --workspace` — 10 passed
- `cargo clippy --workspace --all-targets` — 0 warnings
- `pytest services/tts-worker/tests` — 5 passed
- `npx tsc --noEmit` — clean; `npx vitest run` — 17 passed
- `python scripts/e2e_mock_worker.py`, `python scripts/validate_repo.py` — passed
- `npm run build` — static export OK
- `python scripts/e2e_real_worker.py` — real Kokoro synthesis OK
- Ad-hoc real-worker check: word_timings present (9 words, monotonic, within duration) from real Kokoro model

**Tests added:** worker mock timings + multi-engine protocol test; `voices.test.ts` for estimateWordTimings/wordIndexAt.

**Assumptions (could be wrong):**
- Maya1 GGUF prompt/token layout follows the published reference (SOH 128259, SNAC code base 128266, 7-token frames); real synthesis not run — needs the 3.4 GB download to verify.
- Voxtral vLLM-Omni serve flags in the Models page command may need adjustment to the installed vllm-omni version; the `/v1/audio/speech` request shape follows the model card.
- Voxtral preset voice list is a best-effort subset; free-form names pass through to the server.
- Kokoro timing/word-count mismatches are handled by index scaling in `wordIndexAt`.

**Risk tier:** medium (DB migrations are additive; cache keys unchanged for Kokoro).

**Rollback:** `git revert` the commit; the added SQLite columns/tables are ignored by older builds.

**Status:** done (real inference verified for Kokoro; Maya1/Voxtral verified at protocol/mock level, real weights not downloaded).
