# Worksheet: Voxtral speed M4 (playhead-anchored streaming scheduler)

**Goal**: Replace generate-then-play with a playhead-anchored generation
queue: clicking any sentence starts narration there, a fill job keeps
generating ahead of the listener, and the old "not generated yet" dead-end
becomes a short buffering state
(`docs/plans/voxtral-streaming-optimization.md` workstream A1-A3).

**Files changed**:
- `apps/desktop/src-tauri/src/commands.rs` — `anchor`/`anchor_job` state on
  AppRuntime; `anchored_pick` (pure, unit-tested) selects the first pending
  sentence at or after the playhead, wrapping to earlier text; the
  generation loop re-reads the anchor between sentences; new commands
  `anchor_generation` (re-anchor + reuse-or-start full-book fill job) and
  `set_generation_anchor` (anchor follow without starting a job);
  `queue_generation` refactored onto a shared `spawn_generation`.
- `apps/desktop/src-tauri/src/lib.rs` — command registration.
- `src/lib/tauri.ts` — `anchorGeneration`, `setGenerationAnchor` bindings.
- `src/components/ReaderStudio.tsx` — playFragment anchors the queue on
  every play; when a sentence is not cached it starts/reuses the fill job,
  shows "Preparing narration…", polls the segment (600 ms, 180 s deadline,
  superseded by newer clicks), then plays. Auto-advance inherits the same
  path, so chapter playback is gapless as long as generation outruns
  playback (wall RTF 0.45 on Balanced after M3).

**Commands run**: `cargo test --workspace` (all pass; desktop 7 incl. 4 new
anchored_pick tests), `cargo clippy --workspace --all-targets -- -D warnings`
(clean), `npm run typecheck`, `npm test` (17), `npm run build` (static
export OK).

**Tests added**: 4 unit tests for `anchored_pick` (no anchor, jump forward,
wrap-around, between-positions).

**Assumptions**:
- Sentence-level preemption is enough: an in-flight sentence finishes (a few
  seconds) before the queue re-anchors.
- Resume-position default play already existed via saved progress; decisions
  from the plan (resume default, full-book background fill) are honored.

**Risk tier**: medium — touches the production generation loop; existing
modes (current_and_next, full_book, selected) keep reading order when no
anchor is set.

**Rollback**: revert the four files to tag `voxtral-speed-m3-b3a`.

**Status**: done (code + automated verification). Remaining release checks:
live desktop click-to-play session on real hardware, and the M3-B2 listening
gate (WAV pack already delivered).
