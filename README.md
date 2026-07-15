# AudiobookGen

AudiobookGen is a local-first desktop application that turns DRM-free EPUB 2 and EPUB 3 books into Kokoro audiobooks. The audiobook workflow is primary: import review, narration generation, chapter delivery, and portable export. A synchronized EPUB reader is included so a listener can follow the current sentence or jump playback by selecting text.

## Current vertical slice

- Imports reflowable, fixed-layout, and mixed EPUB packages with embedded text.
- Reviews spine items before import and lets the user include or exclude chapters.
- Configures footnotes, captions, and tables during import.
- Extracts book metadata, cover art, chapter order, and readable XHTML.
- Normalizes English text deterministically before synthesis.
- Runs one persistent Kokoro worker instead of reloading the model for every sentence.
- Downloads Kokoro on first use, then loads the model and voice files from local storage.
- Generates the current and next chapter or the complete selected book.
- Caches sentence WAV files by text, voice, speed, model, and pipeline version.
- Resumes interrupted jobs without regenerating valid cached sentences.
- Supports several narration profiles for one book, with one narrator per profile.
- Saves reading and listening progress, including the current sentence and audio offset.
- Displays EPUBs in paginated or scrolling mode with sentence click-to-play.
- Stores book-scoped pronunciation corrections without changing displayed EPUB text.
- Exports chapter M4A files, one chaptered M4B, and EPUB 3 Media Overlays.
- Copies a narrated EPUB and progress manifest to USB drives, LAN mounts, or synchronized folders.
- Continues generation when the main window is hidden and inhibits system sleep during active work.
- Collects no telemetry and requires no account.

Image-only fixed-layout pages are displayed but are not narrated. PDF, OCR, DRM removal, voice cloning, cloud inference, and Android generation are intentionally outside the first release.

## Architecture

```text
Next.js static export inside Tauri 2
              │ commands and events
              ▼
Rust desktop shell + audiobookgen-core
  ├── EPUB package inspection and import
  ├── deterministic narration compiler
  ├── SQLite WAL library and progress
  ├── resumable generation scheduler
  ├── sentence audio cache
  ├── M4A / M4B / narrated EPUB export
  └── accountless folder sync package
              │ JSON Lines over stdio
              ▼
Persistent Python Kokoro worker
  ├── local model installation
  ├── Misaki English G2P
  └── 24 kHz sentence synthesis
```

See [Architecture](docs/ARCHITECTURE.md), [Research decisions](docs/RESEARCH.md), [Platform support](docs/PLATFORMS.md), and [Sync package](docs/SYNC.md).

## Development

Prerequisites:

- Node.js 22 or newer
- Rust 1.85 or newer
- Python 3.10 through 3.13
- `espeak-ng` for Kokoro's out-of-dictionary English fallback
- Tauri platform dependencies
- FFmpeg in `PATH` for M4A, M4B, and narrated EPUB export

Install the JavaScript dependencies:

```bash
npm install
```

AudiobookGen normally creates a private Python environment in its application-data directory the first time Kokoro is used. During development, an explicit environment makes iteration faster:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -e services/tts-worker
export AUDIOBOOKGEN_PYTHON="$PWD/.venv/bin/python"
```

On Windows PowerShell, use `.venv\Scripts\Activate.ps1` and set `$env:AUDIOBOOKGEN_PYTHON` to the venv Python executable.

Run the desktop application:

```bash
npm run tauri -- dev
```

Run the checks available without downloading Kokoro:

```bash
python3 scripts/validate_repo.py
PYTHONPATH=services/tts-worker python3 -W error::ResourceWarning -m unittest discover -s services/tts-worker/tests -v
python3 scripts/e2e_mock_worker.py
npm run typecheck
npm test
npm run build
cargo fmt --all -- --check
cargo test -p audiobookgen-core --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p audiobookgen-desktop
```

## Data and privacy

The original EPUB is copied into the local library and never modified. Generated audio, model files, progress, and exports remain in the application-data directory or a destination explicitly selected by the user. Book text is sent only through a local stdio pipe to the Kokoro worker.

## Licensing

AudiobookGen source is AGPL-3.0-or-later. Kokoro model weights and official inference code are Apache-2.0. Dependencies retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md).
