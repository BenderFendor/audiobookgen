# Worksheet: M7 first-sound-fast + M8 product-path harness

**Goal**: Land the first slice of `docs/plans/product-hardening.md` — M7
("first sound, fast": DF-1, DF-8, DF-9, DF-24-late-validation) and the
foundation of M8 (headless product-path E2E gate, DF-34/DF-35) — per user
request to work from the plans in `docs/plans/`. Scope was narrowed from the
full 36-defect/10-milestone register to this slice via an explicit user
choice (M8 + M7 recommended option), consistent with the plan's own stated
sequencing ("M8 harness gates M7 done-criteria, land together").

**Files changed**:
- `apps/desktop/src-tauri/Cargo.toml` — dev-dependencies: `tauri` with the
  `test` feature, `tempfile`, `zip` (all already workspace deps).
- `apps/desktop/src-tauri/src/commands.rs`:
  - DF-1: removed `file_sha256`; replaced `voxtral_cache_discriminator`'s
    full-file hashing with `file_identity` (size, mtime), matching
    `voxtral_int4/runtime.py::_quant_cache_meta`'s existing discriminator
    pattern. Added `profile_override` parameter (feeds DF-9).
  - DF-9: added `AppRuntime.urgent: Arc<Mutex<Option<Uuid>>>`, set by
    `anchor_generation`. `run_generation` overrides the Voxtral profile to
    `compatibility` for exactly the urgent fragment when the configured
    profile isn't already `compatibility`. `engine_options` gained a
    `voxtral_profile_override` parameter.
  - DF-24: extracted `installed_voxtral_voices` (shared by
    `list_engine_status` and `create_narration_profile`); profile creation
    now rejects voice names absent from the installed embedding directory.
  - M8: `emit_model_progress`, `download_engine_model`, `preview_voice`,
    `queue_generation`, `anchor_generation`, `spawn_generation`,
    `run_generation` made generic over `R: tauri::Runtime` (was the concrete
    default `AppHandle` = `AppHandle<Wry>`), enabling `tauri::test`'s
    `MockRuntime` in tests without a real windowing system.
  - Added `#[cfg(test)] mod product_path_tests`: a fixture-EPUB builder, the
    mock-engine product-path E2E, and an `#[ignore]`d real-hardware DF-1
    regression guard.
- `src/components/ReaderStudio.tsx` — DF-8: `generationRef` mirrors the live
  `generation` prop; `playFragment`'s wait loop surfaces the worker's live
  progress message and replaces the fixed 180s deadline with "no new
  progress event for 120s."
- `docs/Log.md`, `docs/plans/product-hardening.md` — status entries.

**Commands run** (all evidence, not "should work"):
- `cargo check -p audiobookgen-desktop` / `cargo check --workspace
  --all-targets` after each edit (iterative, hook-enforced).
- `cargo test --workspace` — 8/8 desktop unit tests + 1 ignored, 10/10 core
  tests, all green.
- `cargo clippy --workspace --all-targets` — clean.
- `npm run typecheck` — clean (the project has no `lint` npm script; the
  repo-wide oxlint hook fails independently of this change on missing
  `tsgolint`, reproduced on unmodified `main` via `git stash`).
- `cargo test -p audiobookgen-desktop -- --ignored --nocapture
  real_voxtral_discriminator` against the real installed Voxtral model at
  `/mnt/Big storage/AudiobookGen/models/voxtral-4b-tts`: **903.587 µs**,
  down from the 83s measured in the plan's ground-truth audit.

**Tests added**:
- `commands::product_path_tests::mock_generation_reaches_every_fragment_through_the_real_commands`
  — the M8 foundation: real `import_epub_impl` → `run_generation` through
  `tauri::test::MockRuntime` with `AUDIOBOOKGEN_WORKER_MOCK=1`, asserting
  every fragment gets a cached, on-disk segment.
- `commands::product_path_tests::real_voxtral_discriminator_is_fast_not_an_8gb_hash`
  (`#[ignore]`) — DF-1 regression guard against the real model file.

**Assumptions**:
- Cache-key mismatch already triggers regeneration for any parameter change
  (verified in the existing `save_generated_segment`/`generated_segment`
  flow), so a DF-9 urgent/compatibility-tagged cache entry is naturally
  replaced the next time that sentence is generated without urgency — no new
  "refresh" mechanism was needed.
- `(size, mtime)` is an acceptable identity proxy for the weights and voice
  files because the weights checksum is already verified once at install
  time (`engines/voxtral.py::ensure_model`) and the quantized-weight cache
  already trusts the same proxy for the same file.
- The pre-existing worker venv at
  `~/.local/share/io.audiobookgen.desktop/worker-venv` (this app's own
  managed-python data directory convention) was reused by the harness test
  when present, for speed; the test falls back to `AppRuntime`'s normal
  uv-managed bootstrap otherwise (untested in this session — no network
  bootstrap was exercised).

**Risk tier**: medium. Touches the hot generation path (`run_generation`,
cache-key derivation) and a public struct's method signatures
(`engine_options`, `voxtral_cache_discriminator`), but every change is
additive/behavior-preserving outside the two defects being fixed, and is
covered by a real end-to-end test plus a real-hardware measurement.

**Rollback**: `git revert` the commit touching `apps/desktop/src-tauri/src/commands.rs`,
`apps/desktop/src-tauri/Cargo.toml`, and `src/components/ReaderStudio.tsx`.
No schema or on-disk cache format changed; previously cached Voxtral segments
will simply regenerate once (cache-key discriminator format changed).

**Status**: done for the scoped slice (M7's four defects; M8's harness
foundation). Not done: M8's remaining latency/gapless/progress-shape
assertions, the four-latency SPEEDLOG extension, the B2 listening gate, CI
wiring for the new test, and M7's own done-criteria numbers on real hardware
through the harness — all called out as outstanding in
`docs/plans/product-hardening.md`'s updated status lines.
