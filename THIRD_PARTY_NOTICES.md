# Third-party notices

AudiobookGen integrates with, links against, or invokes the following projects. This list is not a substitute for the complete dependency metadata produced for a release.

- Kokoro inference library and Kokoro-82M model weights — Apache-2.0
- Misaki grapheme-to-phoneme library — Apache-2.0
- eSpeak NG fallback — GPL-3.0-or-later
- Tauri — Apache-2.0 / MIT
- Next.js and React — MIT
- EPUB.js — BSD-2-Clause
- SQLite — public domain
- FFmpeg — LGPL/GPL depending on the user's installed build and enabled codecs
- PyTorch and Hugging Face libraries — their respective upstream licenses

Release automation must generate an exact software bill of materials, retain license texts required by each dependency, and record checksums for downloaded model artifacts. AudiobookGen does not redistribute FFmpeg in the current implementation; it invokes the executable selected by `AUDIOBOOKGEN_FFMPEG` or found in `PATH`.
