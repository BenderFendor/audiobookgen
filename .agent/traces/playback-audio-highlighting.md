# Playback audio and sentence marking

**Goal:** Prove generated audio contains signal, fix WebKit playback, keep the transport visible, and mark the sentence that is currently playing.

**Files changed:**

- `README.md`
- `docs/TESTING.md`
- `papercuts.md`
- `scripts/check_audio_levels.py`
- `src/app/globals.css`
- `src/components/ReaderStudio.tsx`
- `src/lib/reader.ts`
- `src/lib/reader.test.ts`
- `src/lib/tauri.ts`
- `src/lib/tauri.test.ts`
- `.agent/screenshots/playback-highlight-verified.png`
- `.agent/traces/playback-highlight-pre-edit.md`
- `.agent/traces/*.json` watchdog reports for the commands below
- `.agent/traces/playback-audio-highlighting.md`

**Commands run:**

- `gst-inspect-1.0 autoaudiosink`: passed; plugin loaded from `gst-plugins-good` 1.28.4.
- Bounded `gst-launch-1.0 audiotestsrc ... autoaudiosink`: passed with exit 0.
- `python3 scripts/check_audio_levels.py --require-signal <three cached WAVs>`: passed; RMS ranged from -25.7 to -25.4 dBFS and peaks from -9.0 to -2.5 dBFS.
- WebKit remote inspector checks: asset WAV failed with `NotSupportedError`; the same bytes played from an explicit `audio/wav` Blob and advanced to 0.79 seconds.
- Live application check: playback changed to Pause, elapsed time advanced, the current marker count became 1, automatic sentence advance worked, and the visible EPUB sentence was marked.
- `python3 scripts/validate_repo.py`: passed.
- `PYTHONPATH=services/tts-worker python3 -W error::ResourceWarning -m unittest discover -s services/tts-worker/tests -v`: 4 passed.
- `python3 scripts/e2e_mock_worker.py`: passed with a 38,444-byte WAV and four protocol events.
- `npm run build`: passed.
- `npm run typecheck`: passed.
- `npm test`: 11 passed.
- `cargo fmt --all -- --check`: passed.
- `cargo test -p audiobookgen-core --all-targets`: 10 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo check -p audiobookgen-desktop`: passed.

**Tests added:**

- Sentence range mapping for multi-sentence paragraphs, repeated sentences, and whitespace introduced by EPUB drop caps or inline markup.
- WAV object URL creation with an explicit `audio/wav` Blob for WebKit.
- A reusable cached-WAV level check that reports duration, RMS, and peak dBFS.

**Assumptions:** Generated narration remains 16-bit PCM WAV at 24 kHz. The level script labels a peak below -30 dBFS as low but only fails `--require-signal` when every inspected file is silent.

**Risk tier:** medium

**Rollback:** Revert the commit tagged `playback-audio-highlighting`. This restores direct asset-URL playback, the previous paragraph-level reader binding, and the old transport layout.

**Status:** done

Chrome MCP was unavailable because Chrome was not running. The real Tauri/WebKit window was checked through WebKit's inspector and a compositor screenshot instead.
