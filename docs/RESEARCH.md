# Research decisions

This file records why the first implementation makes its main technical choices. The papers describe stronger long-form models than Kokoro in some dimensions; AudiobookGen uses their findings to structure a lightweight Kokoro pipeline rather than claiming to reproduce their architectures.

## Exact synchronization at the synthesis boundary

Calliope reports that collecting timing while narration is generated avoids drift introduced by generating first and applying forced alignment later. AudiobookGen adopts the same systems principle at sentence granularity: each sentence is a separate known audio segment, so its start and end are exact after chapter assembly. This avoids shipping an ASR or forced-alignment model.

Reference: Hammer, Thambawita, and Halvorsen, “Calliope: A TTS-based Narrated E-book Creator Ensuring Exact Synchronization, Privacy, and Layout Fidelity,” arXiv:2602.10735.

## Paragraph context still matters

ContextSpeech, ParaTTS, and audiobook-context work show that long-form quality depends on cross-sentence context, paragraph position, prosodic history, and boundary behavior. Kokoro does not expose those papers' paragraph encoders, so the application does not pretend sentence splitting solves long-form prosody completely.

The practical response is to preserve paragraph and block structure in the narration compiler, vary pauses by semantic boundary, avoid arbitrary character chunks, and keep the architecture open to passing richer context to a future Kokoro-compatible backend. The current model still synthesizes sentence units because that provides strong recovery, caching, and synchronization properties on modest hardware.

References:

- Xiao et al., “ContextSpeech: Expressive and Efficient Text-to-Speech for Paragraph Reading,” arXiv:2307.00782.
- Xue et al., “ParaTTS: Learning Linguistic and Prosodic Cross-sentence Information in Paragraph-based TTS,” arXiv:2209.06484.
- Xin et al., “Improving Speech Prosody of Audiobook Text-to-Speech Synthesis with Acoustic and Textual Contexts,” arXiv:2211.02336.

## Kokoro timing and model loading

Kokoro's current pipeline exposes generated chunks and predicted token durations. AudiobookGen deliberately starts with sentence highlighting, which is stable across all supported EPUBs and does not require mapping every G2P token back through punctuation and DOM whitespace. Token timing remains a possible later enhancement for optional word highlighting.

The worker creates one `KModel`, reuses it through one language-specific `KPipeline`, and supplies explicit local model and voice files. This follows Kokoro's own model/pipeline separation and avoids loading duplicate model instances.

References:

- `hexgrad/kokoro`, `kokoro/model.py` and `kokoro/pipeline.py`.
- Kokoro-82M model repository and model card.

## EPUB Media Overlays

EPUB 3 Media Overlays use SMIL to associate text references with timed audio clips. This is the portable read-along format produced by the export module. The original source EPUB is not modified; the exporter creates a derived package and validates its own manifest relationships in tests.

Reference: W3C, EPUB 3.3, Media Overlays.

## Why SQLite WAL

Generation updates segment state while the reader and library continue to query progress. WAL allows readers to continue while a writer commits and is supported by a single local SQLite file. Audio remains outside the database to avoid large BLOB rewrites.

Reference: SQLite, “Write-Ahead Logging.”

## Evaluation plan

The project does not treat one automatic score as proof of audiobook quality. Regression work should track:

- human A/B preference on paragraph and chapter samples
- omission, repetition, and pronunciation errors
- time to first playable sentence
- warm real-time factor and peak memory
- silence and loudness consistency at sentence boundaries
- cache hit rate and crash recovery
- synchronization drift after full chapter assembly
- EPUB import coverage across EPUB 2, EPUB 3, reflowable, fixed-layout, malformed, footnote-heavy, table-heavy, and poetry fixtures

Any future speed optimization—ONNX, quantization, alternate execution providers, or native mobile inference—must pass the same audio and pronunciation corpus before replacing the official Kokoro baseline.
