# Contributing to AudiobookGen

AudiobookGen is intentionally narrow: EPUB in, local Kokoro narration out, with a synchronized reader and portable exports. Changes should strengthen that workflow without adding a cloud dependency or a second product inside the repository.

## Before changing code

Read:

- `AGENTS.md`
- `docs/ARCHITECTURE.md`
- `docs/RESEARCH.md`
- `docs/TESTING.md`

## Pull requests

A pull request should contain one coherent product or infrastructure change and explain:

- the user-visible behavior
- the failure or limitation it addresses
- format, storage, or compatibility consequences
- tests added
- commands run

Parser fixes require a minimal non-copyrighted EPUB fixture. Text-normalization fixes require source text and expected spoken text. Worker fixes require a protocol regression test that runs without downloading Kokoro whenever possible.

## Boundaries

Do not add:

- telemetry enabled by default
- required accounts or hosted services
- DRM removal
- PDF/OCR dependencies in the EPUB runtime
- an LLM or ASR model in the normal generation path
- book-text uploads
- arbitrary shell command fields in the worker protocol
- a second durable database outside the Rust core

Large optional engines must use an adapter and remain outside the default installation.

## Generated files and dependencies

Do not commit model weights, generated narration, imported EPUBs, build output, private book text, or temporary worker environments. Dependency changes must preserve license notices and explain installer-size or memory impact.

## Quality bar

A feature is not complete when only its happy-path interface exists. Cover malformed EPUBs, cancellation, process failure, restart, stale cache, unavailable FFmpeg, missing models, and unsupported platform behavior where relevant.
