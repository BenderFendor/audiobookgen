# Testing strategy

AudiobookGen treats parser behavior, narration identity, worker recovery, synchronization, and export structure as product behavior rather than implementation details.

## Fast checks

```bash
python3 scripts/validate_repo.py
PYTHONPATH=services/tts-worker python3 -W error::ResourceWarning -m unittest discover -s services/tts-worker/tests -v
python3 scripts/e2e_mock_worker.py
npm run build
npm run typecheck
npm test
cargo fmt --all -- --check
cargo test -p audiobookgen-core --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p audiobookgen-desktop
```

Frontend regression coverage includes:

- `src/lib/reader.test.ts`: sentence ranges inside one paragraph, repeated text, and whitespace around EPUB drop caps or inline markup.
- `src/lib/tauri.test.ts`: Tauri detection, asset paths, and explicit WAV Blob creation for WebKit playback.
- `services/tts-worker/tests/test_progress.py`: structured progress events with and without byte totals.
- `services/tts-worker/tests/test_voxtral_int4.py`: 24-to-48 kHz propagation, WAV header duration, CFG profile safety, and traversal rejection.
- `apps/desktop/src-tauri/src/voxtral.rs` unit tests: NVIDIA status parsing, compute capability, and the measured 12 GB gate.

The mock worker must remain dependency-light. It verifies the persistent JSON Lines protocol and valid 24 kHz WAV output without downloading Kokoro or importing PyTorch.

## EPUB fixture matrix

Every parser regression should add the smallest EPUB that preserves the failure. The durable matrix is:

- EPUB 2 NCX navigation
- EPUB 3 navigation document
- reflowable chapters
- fixed-layout XHTML with embedded text
- mixed-layout spine
- image-only fixed-layout pages
- nested package and navigation paths
- percent-encoded resource names
- repeated visible sentences
- inline emphasis splitting a sentence across text nodes
- footnotes inline and at chapter end
- captions
- simple and complex tables
- poetry and scene breaks
- malformed but recoverable metadata
- encrypted resources and font obfuscation

Fixtures must not contain copyrighted book text.

## Narration regression corpus

The English corpus should include names, initials, abbreviations, dates, times, currencies, percentages, Roman-numbered chapters, dialogue, ellipses, em dashes, quotations, and long clauses. Each sample records:

- displayed source text
- expected normalized spoken text
- expected sentence boundaries
- expected pause class
- cache-key changes caused by narrator, speed, model, or pronunciation edits

## Audio checks

Inspect generated sentence signal levels with:

```bash
python3 scripts/check_audio_levels.py --require-signal
```

The report labels each 16-bit PCM WAV as `OK`, `LOW`, or `SILENT` and prints duration, RMS level, and peak level in dBFS. Pass file paths to inspect a smaller set.

The official Kokoro backend is evaluated separately from the deterministic mock worker. Release candidates should record:

- time to first playable sentence
- warm real-time factor
- peak resident memory
- omission and repetition failures
- pronunciation failures
- leading/trailing silence
- chapter-boundary loudness
- worker stability over a complete public-domain book

Human A/B listening remains the final quality check. An automatic score alone is not a release gate.

## Export checks

M4B checks verify chapter start/end times and metadata. Narrated EPUB checks verify:

- `mimetype` is stored first and uncompressed
- all OPF manifest references exist
- each narrated content item points to a SMIL overlay
- every SMIL text target exists
- every clip range is monotonic and inside its chapter audio duration
- the package opens in at least two independent EPUB 3 readers

## Crash recovery

Kill the worker during one sentence, restart the application, and resume the same job. Completed sentence files must remain cache hits, `.part.wav` files must never be treated as finished output, and SQLite must remain readable.

## Real worker end-to-end

`python3 scripts/e2e_real_worker.py` drives the actual Kokoro worker through
the app's managed venv and downloaded model (skips cleanly when either is
missing, so CI stays on the mock E2E). It catches import-time breakage in the
kokoro dependency chain that the mock path cannot see. Run it after any change
to `services/tts-worker/pyproject.toml` or the worker bootstrap in
`apps/desktop/src-tauri/src/commands.rs`.

Voxtral release verification requires a Linux NVIDIA host with 12 GB VRAM and compute capability 8.0 or newer. Run `services/tts-worker/scripts/benchmark_voxtral.py` with the installed model, confirm a mono 48 kHz WAV, CFG 1.2, finite signal, correct duration metadata, bounded startup/inference memory, repeated-generation cache reset, and worker shutdown that releases VRAM. GPU tests are hardware-gated and never replaced with mock-generation claims.
