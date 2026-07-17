# Worksheet: product audit and hardening plan

**Goal**: Audit why the app feels broken end to end (Voxtral never starts,
highlighting/scrolling/keys glitchy, narrator model redundant, voices
missing) without changing code, then extend the existing Voxtral speed plan
into a full product-hardening plan.

**Files changed**:
- `docs/plans/product-hardening.md` (new — DF-1..DF-36 register, M7-M16)
- `docs/plans/voxtral-streaming-optimization.md` (pointer line to successor)
- `docs/Log.md` (audit + plan entry)

**Commands run** (read-only evidence):
- `time sha256sum` of `consolidated.safetensors` → 83 s (warm-ish cache)
- `lsblk` → models volume is rotational (sdc2)
- Worker venv check → torch 2.13.0+cu130, torchao 0.17.0, CUDA available
- Quant-cache meta vs environment → current, fast load path valid
- Model dir listing → 20 voice embeddings vs 5 hardcoded in UI

**Tests added**: none (audit/plan only; M8 defines the harness that becomes
the evidence bar).

**Assumptions**:
- 12.5 frames/audio-second for Voxtral frame-ceiling math (model README).
- "5 s cold start" in the speed plan was measured via the direct-runtime
  benchmark, not the desktop path.
- Cache keys hash voice parameters, not profile ids, so the M11 narrator
  migration preserves generated audio (verified in `cache.rs`).

**Risk tier**: low (documentation only; no runtime behavior changed).

**Rollback**: delete `docs/plans/product-hardening.md`, revert the two doc
edits.

**Status**: done. Follow-up work starts at M7+M8 in the new plan.
