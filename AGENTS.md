# AudiobookGen agent guide

## Product rules
- Audiobook generation is the primary workflow; reading is secondary but first-class.
- EPUB 2/3 only in the first release. Both reflowable and fixed-layout books must import.
- English narration only. One Kokoro narrator per narration profile, with one active profile per book.
- Never mutate the imported EPUB. Derived reader assets and narrated EPUB exports are separate.
- Sentence is the smallest playback, cache, regeneration, and highlighting unit.
- No telemetry. No account is required. Model files download on first use.
- OCR, PDF ingestion, voice cloning, and on-device Android generation are out of scope.

## Engineering rules
- Rust owns durable state, EPUB parsing, job scheduling, sync events, and exports.
- Python owns Kokoro inference only and communicates with newline-delimited JSON.
- The Next.js app is a static export embedded in Tauri; do not add server actions or API routes.
- Every durable write must be atomic or transactional.
- Every generated segment key must include source text, normalization version, model checksum field, voice, and speed.
- Add fixture-based regression tests for every parser or normalizer failure.
- Do not add an LLM or ASR dependency to the runtime path.
- Do not add generated build state to the repository.
