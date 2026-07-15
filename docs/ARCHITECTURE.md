# Architecture

## Product boundary

AudiobookGen is an audiobook production application with a companion reader. EPUB is the source of truth. The imported file is immutable; generated audio, synchronization metadata, and exports are derived artifacts.

The first release accepts EPUB 2 and EPUB 3. Reflowable and fixed-layout spine items are imported when they contain readable XHTML text. No OCR path exists in the runtime.

## Process model

The application has three layers:

1. A statically exported Next.js interface rendered by Tauri's system webview.
2. A Rust core responsible for all durable state and filesystem changes.
3. One long-lived Python worker responsible only for Kokoro inference.

There is no local HTTP server. Tauri commands carry user actions into Rust, and Tauri events report generation progress to the interface. Rust communicates with Python through newline-delimited JSON on standard input and output. Audio bytes never travel through JSON; the worker writes a temporary WAV and returns its path and metadata.

## Import transaction

Import follows a review-then-commit workflow:

1. Inspect the ZIP container, `container.xml`, package document, manifest, spine, metadata, navigation, encryption metadata, and XHTML resources.
2. Show selected chapters and parser warnings.
3. Reinspect the source and verify its SHA-256 before committing.
4. Copy the original EPUB into a content-owned book directory.
5. Parse only selected chapters according to footnote, caption, and table policy.
6. Create deterministic narration fragments and the initial profile.
7. Insert the book, chapters, fragments, and profile in one SQLite transaction.

An already imported source hash resolves to the existing book instead of creating a duplicate.

## Narration compiler

The compiler preserves two representations:

- `source_text`: exact readable text used to locate and display the sentence.
- `spoken_text`: normalized English sent to Kokoro.

The parser identifies headings, prose, dialogue, captions, tables, footnotes, and scene breaks. The sentence planner limits fragment size to stay below Kokoro's phoneme-context ceiling. Pause duration is metadata, not synthesized silence, so it can be adjusted without changing the model output.

A sentence cache key includes:

- normalized spoken text
- planned pause
- Kokoro voice
- narration speed
- model revision and checksum field
- normalization version
- planner version

Pronunciation rules are applied immediately before generation. Changing one rule changes only cache keys for sentences containing the affected text.

## Generation and recovery

A generation job chooses full-book, selected-chapter, or current-plus-next fragments. The scheduler is deliberately serial for the first release because a single Kokoro model already uses substantial memory and sentence-level parallel inference would compete for CPU and RAM on older machines.

For every fragment:

1. Check the database record and cached WAV.
2. Generate into a uniquely named `.part.wav` file.
3. Validate the worker response.
4. Atomically rename the file into the content-addressed cache.
5. Record the segment in SQLite.
6. Emit progress to the interface.

A crash can lose only the sentence currently being written. Completed cache files remain reusable. Cancellation is checked between sentences. Active generation starts a platform sleep inhibitor and releases it when the job exits.

## Worker lifecycle

The worker starts on the first model or generation operation and remains loaded. Unless `AUDIOBOOKGEN_PYTHON` is supplied, the desktop app creates a private virtual environment under application data and installs the worker package there. The model snapshot is downloaded into a separate model directory. At inference time the worker passes explicit local config, weight, and voice paths to Kokoro; it does not depend on a network lookup after installation.

Requests are serialized through the worker supervisor. This keeps the protocol simple and prevents concurrent calls from interleaving model output or stdout responses.

## Storage

SQLite runs in WAL mode with foreign keys enabled. The database stores metadata and references; large EPUB and audio files remain on disk.

```text
app-data/
├── library.sqlite3
├── books/<book-id>/source.epub
├── books/<book-id>/cover.<ext>
├── cache/segments/<cache-key>.wav
├── models/kokoro-82m/
├── worker-venv/
└── exports/
```

The reader uses EPUB locations plus source-text hashes. Reading and listening positions are stored separately but can be linked in the interface.

## Export

Internal segments stay mono 24 kHz PCM WAV so individual sentences can be regenerated without generational codec loss.

- M4A export renders each chapter and encodes AAC.
- M4B export renders the book, writes FFmetadata chapter ranges, and encodes one AAC container.
- Narrated EPUB export copies the original package, injects sentence targets into derived XHTML, embeds chapter audio, generates SMIL Media Overlays, and updates the package manifest and metadata.

FFmpeg is isolated behind the export module. Generation and ordinary reading do not require it.

## Security boundaries

- Imports are restricted to `.epub` ZIP packages.
- ZIP paths are normalized before use.
- DRM metadata is detected and rejected; no decryption code exists.
- Kokoro voices are selected from an allow-list, preventing path injection through voice names.
- The renderer disables scripted EPUB content.
- Tauri's asset protocol is scoped to application data and resources.
- No telemetry, account, remote API, or cloud book-text path is present.
