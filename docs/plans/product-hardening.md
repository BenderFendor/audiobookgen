# Plan: product hardening — from working subsystems to a working product

Extends `voxtral-streaming-optimization.md` (M1-M4 landed, M5/M6 reserved).
Covers the full click-to-audio path, reader correctness, the narrator data
model, worker protocol, storage governance, and the verification gap that let
all of this ship. Grounded in the 2026-07-16 code audit and live measurements
on the shipping machine (RTX 3060 12 GB, models on a rotational HDD).
Status: DRAFT — iterate here. Milestones continue the M-numbering at M7.

## 0. The one-sentence diagnosis

Every landed milestone measured the Python runtime in isolation; nothing ever
measured or exercised the path the user actually clicks through, so the
product accumulated defects in exactly the places the benchmarks cannot see:
the Rust orchestration ahead of the worker, the reader around the audio, and
the data model underneath the profiles.

## 1. Ground truth (measured 2026-07-16)

| Fact | Value | Source |
|---|---|---|
| SHA-256 of `consolidated.safetensors` (8.0 GB) | **83 s** (warm-ish page cache; cold is worse) | timed on `/mnt/Big storage` |
| Models volume | rotational HDD (`ROTA=1`, sdc2) | `lsblk` |
| Quantized INT4 cache | 3.65 GB, current (torch 2.13.0+cu130 / torchao 0.17.0 / hqq 0.2.8.post1 all match meta) | `quantized-cache/int4-model.json` |
| Voice embeddings installed | 20 (`ar_male` … `pt_male`) | model dir listing |
| Voices offered in narrator UI | 5, hardcoded | `src/lib/voices.ts:17` |
| Frontend give-up deadline on first click | 180 s, static message | `src/components/ReaderStudio.tsx:159` |
| Per-job Rust-side weights hash | runs before the first sentence of every generation job | `apps/desktop/src-tauri/src/commands.rs:515,1292` |
| Old plan's own outstanding items | B2 listening gate, live desktop E2E | `voxtral-streaming-optimization.md` §6 status |

Worst-case first click today (cold worker, Balanced default, no inductor
cache): 83 s hash + ~30-40 s quant-cache read from HDD + minutes of
`torch.compile` + first synthesis > 180 s deadline. The UI reports
"Narration is taking too long to prepare" — the user experiences "Voxtral
does not work."

## 2. Design-flaw register

Numbered so milestones and worksheets can reference them. Each entry: defect,
location, consequence, fix direction. "Fix" here is direction, not diff.

### A. Latency and orchestration

- **DF-1 Per-job 8 GB hash.** `voxtral_cache_discriminator`
  (`commands.rs:515`) re-hashes the full weights file plus the voice
  embedding at every `run_generation` start. 83 s measured. The checksum was
  already verified at install (`voxtral.py:82`), and the worker's own quant
  cache deliberately keys on size+mtime instead (`runtime.py:112`). Fix:
  record the install-time checksum once (settings or a `model_installs`
  table) and build the discriminator from that stored value plus voice file
  size+mtime. `segment_cache_key` already reserves a `model_sha256` input
  that every profile leaves as `None` (`cache.rs:16-23`) — populate it at
  install and delete `extend_cache_key` entirely (see DF-27).
- **DF-2 No request timeout or liveness.** `request_with_progress`
  (`worker.rs:166`) reads stdout forever. A wedged worker hangs the command,
  the job, and (via DF-4) everything else, silently. Fix: per-request
  deadline that resets on every progress line (progress means alive), plus a
  worker heartbeat; on expiry kill and respawn the worker, fail the request
  with a typed error.
- **DF-3 One mutex serializes all engines and all request types.**
  `request_gate` (`worker.rs:86`) plus the worker's synchronous stdin loop
  (`main.py:103`) mean a minutes-long Voxtral load blocks Kokoro previews,
  `model_status`, downloads, everything. GPU serialization is correct;
  serializing control traffic behind it is not. Fix: DF-24 (protocol v2).
- **DF-4 Worker slot lock held across pip installs.** `worker_for_engine`
  (`commands.rs:206`) holds the slot mutex while `ensure_worker_environment`
  may run a multi-minute `uv pip install`. Fix: install outside the lock;
  lock only for swap.
- **DF-5 Job model allows overlapping jobs.** Two `queue_generation` calls
  (or a manual job plus the anchor fill job) run concurrently and interleave
  sentences through the single gate; there is no per-book or global
  scheduler owning priority. Fix: one generation scheduler per app owning
  the worker, a priority queue (playhead distance, then reading order), and
  job requests that enqueue rather than spawn.
- **DF-6 Cancellation only between sentences.** `cancel_generation` sets a
  flag checked per fragment (`commands.rs:1322`); a 40-second Voxtral
  sentence cannot be interrupted, and the frame loop
  (`inference.py:211`) has no cancellation check. Fix: worker `cancel`
  message checked per frame; Rust maps job cancel → in-flight request
  cancel.
- **DF-7 `stop_voxtral_runtime` races running jobs.** It takes the worker
  (`commands.rs:1222`) while a job still holds an `Arc` to it; the job's
  next request fails as "worker exited unexpectedly." Fix: scheduler owns
  worker lifecycle; stop = cancel jobs, drain, then shutdown.
- **DF-8 The wait is a black box.** The worker emits granular states
  ("loading-int4", "quantized backbone layer 12/26", "compiling-balanced");
  `run_generation` forwards them; `ReaderStudio` shows a static
  "Preparing narration from this sentence…" (`ReaderStudio.tsx:151`) and
  polls blindly. Fix: surface the live state string in the waiting UI; the
  180 s deadline must not fire while progress events are flowing.
- **DF-9 Balanced-by-default pays the compile stall on the very first
  click.** Default profile is `balanced` (`model.rs:236`); first-ever
  `torch.compile` runs minutes before the inductor cache exists
  (`runtime.py:257`). The old plan §7 already resolved "first click may mix
  profiles (Compatibility first sentence, Balanced buffer)" — never
  implemented. Fix: implement exactly that resolution inside the scheduler.

### B. Playback engine

- **DF-10 Per-sentence `HTMLAudioElement` playback cannot be gapless.** Each
  sentence constructs `new Audio(objectUrl)` after the previous `onended`
  (`ReaderStudio.tsx:191,209`), adding fetch + decode + element-spinup gaps
  between every sentence. Fix: Web Audio API — decode segments to
  `AudioBuffer`s, schedule back-to-back on an `AudioContext` timeline with a
  lookahead queue; prefetch the next N segments while the current one plays.
- **DF-11 `pause_after_ms` is honored nowhere in live playback.** The
  planner computes per-fragment pauses (`narration.rs:178`), hashes them
  into the cache key, and export presumably uses them — live playback chains
  `onended` immediately. Live pacing and exported pacing differ. Fix: the
  Web Audio scheduler inserts `pause_after_ms` of silence between segments;
  add a parity test between live schedule and export concat order/pauses.
- **DF-12 Readiness by polling.** `playFragment` polls `generatedSegment`
  every 600 ms (`ReaderStudio.tsx:160-168`). `run_generation` already emits
  per-fragment completion events — the frontend just doesn't key off them.
  Fix: emit `segment-ready {fragment_id, profile_id}`; the player awaits the
  event with the poll kept only as a degraded fallback.
- **DF-13 Progress saved at 4 Hz over IPC.** Every `timeupdate` fires a
  Tauri invoke + SQLite write (`ReaderStudio.tsx:222`), plus one per
  `relocated`. Fix: coalesce to ≥2 s intervals plus flush on pause, sentence
  change, and window blur/close.
- **DF-14 Stale closures in the auto-advance chain.** `onended` chains into
  the `playFragment` captured at render time: mid-play volume changes revert
  on the next sentence, chapter switches keep advancing the old list. Fix:
  the player becomes a class/ref (like `EpubReader`) holding current state;
  React subscribes to it, not vice versa.
- **DF-15 No client-side rate control.** Voxtral generates at fixed speed,
  but `audio.playbackRate` (or `AudioBufferSourceNode.playbackRate`) gives
  the listener 0.8-2.0x for free, per-engine-agnostic, without regeneration.
  Fix: transport speed control applied at playback; keep generation speed a
  narrator property for engines that support it.
- **DF-16 Word highlighting pretends estimates are timings.** Voxtral and
  Maya1 report no word timings (`voxtral.py:47`); the worker fabricates
  length-proportional ones, then the frontend rescales indices on count
  mismatch (`voices.ts:65`). The drifting highlight reads as "broken." Fix:
  honest modes — engines with real timings (Kokoro) get word highlight;
  others get sentence highlight plus a smooth progress underline, no fake
  word jumps. Real Voxtral timings (frame-boundary alignment) are a separate
  investigation under M15.

### C. Reader

- **DF-17 `chapterIndex` never follows page turns.** Only sidebar clicks and
  resume set it (`ReaderStudio.tsx:79,380`); fragments load per active
  chapter (`:54`), so after paging across a chapter boundary every sentence
  is unbound: clicks dead, highlight dead, transport shows the wrong
  chapter. Fix: derive the current chapter from the `relocated` event's href
  and load fragments for every rendered section, not just the active
  chapter.
- **DF-18 Keyboard dies inside the iframe.** Arrow/space listeners sit on
  the host window (`ReaderStudio.tsx:108`); epub.js content is an iframe
  that swallows keys once focused. Fix: attach the same handler to each
  rendered content document in `bindFragments` (the hook point already
  exists), or use epub.js's rendition keydown relay.
- **DF-19 `scrollIntoView` corrupts paginated layout.** `setCurrent` scrolls
  the highlighted element inside the multi-column iframe (`reader.ts:133`),
  desyncing epub.js's page position — the "page scrolling is all weird"
  symptom. Fix: paginated flow navigates via `rendition.display(cfi)` (CFI
  from the marker's range); `scrollIntoView` only in scrolled-doc flow.
- **DF-20 Fragment binding is fuzzy text search; the locator contract is
  fictional.** The planner emits `css_selector: Some("#ag-block-<hash>")`
  (`narration.rs:57`) but no such anchors are ever injected into the EPUB
  DOM, `cfi` is always `None`, and the reader ignores locators entirely,
  re-finding sentences by compacted-text scan per block (`reader.ts:40`).
  Sentences spanning block boundaries or differing after normalization
  silently fail to bind. Fix (binding v2): bind by `(href, text_occurrence,
  source_text_hash)` against a per-section text index built once per render;
  compute and persist the real CFI on first successful bind (schema already
  has the column); log every unbound fragment with chapter and text prefix
  so failures are visible instead of silent; delete the fictional selector
  or make it real.
- **DF-21 Linked mode force-flips pages.** Every played sentence calls
  `reader.goTo` (`ReaderStudio.tsx:197`), yanking the view even when the
  sentence is already visible. Fix: navigate only when the current sentence
  is outside the visible range (foliate-style visible-range check, or
  epub.js `location` comparison).
- **DF-22 epub.js is end-of-life.** v0.3.93, unmaintained, known pagination
  and highlight defects; word-span DOM mutation (`reader.ts:152`) inside its
  CSS-multicolumn layout compounds the fragility. The `EpubReader` class is
  already the seam. Fix: keep the seam, fix within epub.js first (M9), run a
  timeboxed foliate-js spike (M14) before considering migration —
  foliate-js shares the multicolumn strategy but has accurate visible-range
  bisection and active maintenance.

### D. Narration data model

- **DF-23 Narrators are per-book.** `NarrationProfile.book_id` is
  load-bearing: every import creates a fresh default profile
  (`commands.rs:710`), and profiles can only be created against a book
  (`commands.rs:800`). Users rebuild the same narrator per book. Fix: global
  `narrators` table (name, engine, voice, speed, engine options); books get
  an assignment (active narrator + per-book overrides if ever needed).
  Migration: dedupe existing per-book profiles by (engine, voice, speed)
  into global narrators, remap `generated_segments.profile_id`, keep cache
  keys stable (they hash voice parameters, not profile ids — verified in
  `cache.rs`). One-way SQLite migration with a schema-version bump and a
  pre-migration backup copy of `library.sqlite3`.
- **DF-24 Voice catalog is hardcoded and validated too late.** UI offers 5
  of 20 installed Voxtral voices (`voices.ts:17`) while the backend already
  returns the real on-disk list (`commands.rs:968`); `validate_voice`
  accepts any string for voxtral citing a server that does not exist
  (`commands.rs:600`), so a bad voice fails minutes later inside the worker.
  Fix: narrator editor consumes `list_engine_status` voices; group by
  language with readable labels (`fr_female` → "French · female"); validate
  at creation against the installed set; warn when book language ≠ voice
  language; use `voxtral_default_voice` (currently dead: set in settings,
  ignored by `ReaderStudio`'s draft at `:290`).
- **DF-25 Default narrator can reference an uninstalled engine.** Import
  hardcodes Kokoro `af_heart` (`commands.rs:716`) even when Kokoro is not
  installed; first generation fails with "model not installed." Fix: default
  to the user's default narrator (M11) or the first installed engine; if
  none, the import review says so.
- **DF-26 Pronunciation fixes dead-end.** Saving a rule tells the user to
  regenerate the chapter (`ReaderStudio.tsx:303`) with no affordance. Fix:
  "apply and regenerate affected sentences" action — normalization changes
  the cache key, so affected fragments are exactly those whose spoken_text
  changed; regenerate only those.

### E. Worker and engine contract

- **DF-27 Cache-identity logic is split across layers.** Core computes
  `segment_cache_key`; the desktop shell bolts on `extend_cache_key` with a
  hand-written discriminator string (`commands.rs:531`). Engine identity
  belongs in one place. Fix: fold engine-specific identity (profile,
  flow steps, seed, postprocess version, quant recipe, install-time model
  checksum) into core's key via an `EngineIdentity` input; delete
  `extend_cache_key`.
- **DF-28 Compatibility profile can truncate maximal fragments.** Planner
  caps fragments at 520 chars (`narration.rs:10`); at ~15-17 chars/s of
  speech that is 30-35 s of audio. Compatibility's 350-frame ceiling at 12.5
  frames/s is 28 s (`runtime.py:18-23`) — a max-length fragment truncates
  mid-sentence with no error. Balanced (500 → 40 s) has little margin. Fix:
  compute the ceiling from the fragment's estimated duration (chars/rate,
  clamped), or raise compatibility's ceiling / lower MAX_FRAGMENT_CHARS so
  planner and engine agree; emit a typed `truncated` warning if the frame
  loop hits max_frames without EOS.
- **DF-29 Error taxonomy collapses to strings.** The worker sends a `code`
  field; Rust formats it into a display string (`worker.rs:190`); commands
  flatten everything to `String`; the frontend shows raw error text. Fix:
  typed error enum (worker code → Rust enum → serialized to the frontend)
  with user-actionable copy per code (`cuda_oom` → "Stop other GPU apps or
  switch profile", `unknown_voice` → names the voice and the installed
  list).
- **DF-30 Worker stderr is invisible.** `Stdio::inherit` (`worker.rs:102`)
  sends tracebacks to a terminal nobody sees in production. Fix: capture
  stderr into a ring buffer, expose the tail in a Models-page diagnostics
  panel and in every worker-failure error.

### F. Storage and data layer

- **DF-31 Hot caches live on the cold disk.** The 3.65 GB quant cache and
  the inductor cache sit next to the weights on the HDD; both are
  regenerable and read on every worker start. Fix: `runtime_cache_root`
  setting defaulting to the app data dir (SSD here); weights stay on the
  models root. Detect rotational models roots and suggest the split.
- **DF-32 Unbounded WAV cache.** 48 kHz mono PCM16 ≈ 5.8 MB/min ≈ 3.5 GB per
  10-hour book per narrator (Kokoro half that); preview WAVs
  (`preview-*.wav`) are never deleted. Fix: per-book cache accounting
  surfaced in the library UI, LRU eviction of segments for non-active
  narrators, delete previews on app start, and evaluate FLAC for the cache
  (lossless, ~50-60% size, decode cost negligible) with WAV kept only as
  the worker output format.
- **DF-33 Single `Mutex<Connection>` called from async commands.** Every
  command runs rusqlite directly on the tokio runtime (`db.rs:14`),
  blocking executor threads and contending with 4 Hz progress writes. Fix
  after DF-13 lands (which removes most write pressure): route DB access
  through `spawn_blocking` or a dedicated DB thread; keep WAL.

### G. Verification (why none of this was caught)

- **DF-34 Benchmarks bypass the product.** `benchmark_voxtral.py` imports
  the runtime directly, so "cold start 5 s" and "RTF 0.45" exclude the Rust
  path where the 83 s regression lived. Fix: M8 harness measures through
  the real commands.
- **DF-35 No product-path E2E.** The only E2E (`voxtral-worker-e2e`
  worksheet) exercises the worker protocol. Nothing clicks a sentence and
  asserts audio. Fix: M8.
- **DF-36 Maya1 is shipping unverified.** Memory and worksheets agree real
  Maya1 inference was never proven on this machine. Fix: verify-or-demote
  (M16): one real generation gate; failing that, label it experimental in
  the Models UI rather than presenting three equal engines.

## 3. Milestones

Numbering continues the Voxtral speed plan. M5 (ConvRot) and M6 (intra-
sentence streaming) keep their reserved slots and gates there.

### M7 — First sound, fast (DF-1, DF-8, DF-9, DF-24-late-validation)

Status 2026-07-16: DF-1, DF-8, DF-9, and DF-24-late-validation landed (see
`docs/Log.md`). Outstanding: the done-criteria latency numbers below have not
been measured end to end on real hardware through the M8 harness yet (the
harness itself only proves the mechanism with the mock engine so far).

The user-facing emergency. Smallest changes that make "press play" work.

- Replace the per-job weights hash with the install-time checksum recorded
  once (populate `model_sha256`, delete `extend_cache_key` usage per DF-27's
  end state; an interim stored-checksum discriminator is acceptable if M27
  refactor is deferred).
- Stream worker progress states into the waiting UI; the deadline becomes
  "no progress event for 120 s", not "180 s total".
- First-click profile mixing: scheduler generates the clicked sentence with
  `compatibility`, switches to the configured profile for the buffer
  (implements old plan §7). Cache keys already distinguish profiles.
- Validate voxtral voice names at profile creation against the installed
  embedding list.
- Done criteria: cold-worker click-to-first-audio < 45 s on this machine
  (HDD quant-cache read dominates; DF-31 in M13 improves it further); warm-
  worker cold-sentence < 5 s; cached sentence < 500 ms; zero "taking too
  long" messages while progress is flowing. All measured by the M8 harness,
  which therefore lands with or before M7.

### M8 — Product-path harness and E2E gate (DF-34, DF-35; absorbs old B2)

Status 2026-07-16: the foundation landed —
`commands::product_path_tests::mock_generation_reaches_every_fragment_through_the_real_commands`
drives `run_generation` (generic over `tauri::Runtime` now) against
`tauri::test::MockRuntime` and the worker's mock engines: import fixture EPUB
→ create narrator → queue generation → assert every fragment gets a cached
segment. Still outstanding: click-sentence latency assertions, the gapless
20-sentence chain check, progress-event-shape assertions, the four-latency
benchmark ledger extension, the B2 listening gate, and CI wiring.

- Headless harness driving the real Tauri commands (mock engine in CI, real
  engines locally): import fixture EPUB → create narrator → click sentence →
  assert first-audio latency, gapless chain of 20 sentences, progress event
  stream shape.
- Extend the benchmark ledger with the four product latencies (cold app,
  cold worker, warm worker cold sentence, cached) reported per run;
  `SPEEDLOG.md` entries without them are incomplete by definition.
- Fold in the outstanding B2 blinded listening gate from the old plan: fixed
  sentence corpus, before/after WAV pairs, documented pass procedure.
- Done: CI runs the mock-engine E2E; a GPU run of the full harness is
  recorded in `reports/benchmarks/` and referenced from `docs/Log.md`.

### M9 — Reader correctness (DF-17, DF-18, DF-19, DF-20, DF-21, DF-13)

- Chapter follows `relocated`; fragments load for all rendered sections.
- Paginated navigation via `display(cfi)`; `scrollIntoView` only in
  scrolled flow; linked mode navigates only when the sentence is off-screen.
- Keyboard handler attached inside rendered content documents.
- Binding v2 per DF-20 with per-fragment bind logging and persisted CFIs.
- Progress saves coalesced (≥2 s + flush on pause/blur/sentence-change).
- Done: harness asserts click-to-play works on a sentence in the chapter
  after a page-turn boundary crossing; bind-failure log is empty on the
  fixture book (or failures are enumerated and understood); zero saves/sec
  during steady playback except the coalesced tick.

### M10 — Playback engine v2 (DF-10, DF-11, DF-12, DF-14, DF-15, DF-16)

- `NarrationPlayer` (plain class, React-free): Web Audio scheduled queue,
  prefetch window (default 3 segments), `pause_after_ms` silences, event-
  driven segment readiness, playback-rate control, sentence/word highlight
  callbacks driven by the audio clock.
- Honest highlight modes per engine capability (word for Kokoro, sentence +
  progress underline for Voxtral/Maya1).
- Live-vs-export pacing parity test.
- Done: 20-sentence chain with zero audible gaps (harness measures inter-
  segment silence ≤ pause_after_ms + 30 ms scheduling tolerance); volume and
  rate changes persist across sentence boundaries; no stale-closure class of
  bug remains (player state lives outside React).

### M11 — Narrator library (DF-23, DF-25, DF-26; finishes DF-24)

- Global narrators table + migration per DF-23 (backup, dedupe, remap,
  schema version bump).
- Narrator editor consumes live voice catalogs; language-grouped labels;
  book-language mismatch warning; `voxtral_default_voice` honored.
- Import assigns the user's default narrator or first installed engine.
- "Apply and regenerate affected sentences" for pronunciation rules.
- Done: creating one narrator makes it available to every book; migration
  round-trips the existing library (verified against a copy of the real
  `library.sqlite3`); export/sync paths still resolve profiles.

### M12 — Worker protocol v2 and scheduler (DF-2 … DF-7, DF-29, DF-30)

- Python worker: reader thread + control plane (ping/status/capabilities
  answered immediately) + single GPU executor thread; `cancel` message
  checked per frame in the generation loops.
- Rust: one global generation scheduler owning worker lifecycle and a
  playhead-priority queue; jobs enqueue; `stop` cancels-drains-shuts-down;
  per-request progress-reset deadlines; typed error codes end to end;
  stderr ring buffer + diagnostics panel.
- Done: kill -STOP the worker mid-generation → UI shows a typed timeout and
  the worker respawns; Kokoro preview completes while a Voxtral sentence is
  mid-generation (control plane proves out); cancel interrupts within one
  frame (~80 ms); the DF-7 race is unreproducible under a stress loop.

### M13 — Storage governance (DF-31, DF-32, DF-33)

- `runtime_cache_root` (default app data dir) for quant + inductor caches;
  models root keeps weights. Migration: copy-if-present, else rebuild.
- Cache accounting in the library UI; preview cleanup on start; LRU eviction
  for non-active narrators; FLAC-cache decision recorded after a measured
  spike (size delta, decode cost on WebKitGTK).
- DB access moved off the async runtime (after M9's write-pressure fix).
- Done: cold worker start on this machine reflects SSD cache reads
  (target < 15 s); library shows per-book audio storage; a 3-narrator book
  respects the eviction policy.

### M14 — Reader engine decision gate (DF-22)

Timeboxed spike, not a commitment: foliate-js behind the existing
`EpubReader` interface on the fixture shelf (reflowable, fixed-layout,
mixed, RTL, footnote-heavy). Compare binding accuracy, pagination
correctness under DOM mutation, visible-range queries, maintenance risk.
Output: a decision record — migrate, wrap, or stay — with the M9 fixes as
the fallback position. No migration work inside this milestone.

### M15 — Voxtral quality follow-ons (keeps old-plan gates)

- ConvRot (old M5) stays plan-only pending the re-profile, unchanged.
- Real word timings investigation: the decoder emits 12.5 frames/s; text
  tokens are consumed in a known order during prefill — investigate mapping
  frame indices to token spans for coarse but honest word timings. Ship only
  if alignment error < ~150 ms median on a hand-checked corpus; otherwise
  keep sentence-mode highlighting (DF-16) permanently.
- DF-28 frame-ceiling fix lands here (engine-side) with a planner-side
  guard, whichever is cheaper after measurement.

### M16 — Maya1: verify or demote (DF-36)

One real generation on this machine through the product path. Pass: keep it
as a peer engine and add it to the M8 GPU harness. Fail or unfixable within
the timebox: label "experimental" in Models UI, exclude from narrator
defaults, file the repair as its own plan.

## 4. Sequencing

```text
M8 harness ──┬─ gates M7 done-criteria (land together, M8 first or same PR)
M7 first-sound-fast   (unblocks daily use)
M9 reader correctness (independent of M7; highest UX value after M7)
M10 playback v2       (depends on M9's event plumbing; supersedes DF-12/13 patches)
M11 narrator library  (independent; schema migration — do not overlap M13's DB move)
M12 protocol v2       (subsumes M7's interim deadline patch; scheduler absorbs anchor_job)
M13 storage           (after M12 so the scheduler owns cache paths; DB move after M9)
M14 reader spike      (any time after M9; informs long-term reader investment)
M15 / M16             (opportunistic; gated as written)
```

Rule carried over from the speed program, now generalized: **no milestone is
"landed" until the M8 harness numbers for it appear in the ledger and the
worksheet links them.** Benchmarks that bypass `commands.rs` no longer count
as evidence for user-facing claims.

## 5. Measurement protocol (applies to every milestone)

Report these five numbers, measured through the harness on this machine,
in every performance-relevant worksheet:

1. Cold-app click-to-first-audio (app just launched, worker not running).
2. Cold-worker click-to-first-audio (worker restarted, caches warm).
3. Warm-worker, cold-sentence click-to-first-audio.
4. Cached-sentence click-to-first-audio.
5. Buffer margin: min(seconds buffered ahead of playhead) over a full
   chapter — must never reach 0 after the first sentence.

Plus the existing decode-loop metrics from the speed plan where relevant.

## 6. Non-goals (unchanged discipline)

- No Whisper/UTMOS/forced-alignment model infrastructure in the repo.
- No cloud inference, telemetry, accounts, DRM handling, PDF/OCR.
- No uniform ConvRot; no vLLM dependency; no HTTP servers.
- No second reader implementation maintained in parallel (M14 decides, once).
- No speculative multi-GPU or >12 GB assumptions.

## 7. Open decisions (resolve in-milestone, record here)

1. FLAC vs WAV segment cache (M13 spike decides; default stays WAV).
2. foliate-js migrate/wrap/stay (M14 decision record).
3. Frame-to-token word timings ship/no-ship (M15 gate).
4. Maya1 peer-or-experimental (M16 gate).
5. Whether per-book narrator overrides survive the M11 migration or the
   assignment is a plain foreign key (default: plain assignment; overrides
   only if a concrete need appears).
