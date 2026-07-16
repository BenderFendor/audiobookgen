# Voxtral speed ledger

One row per landed optimization, measured with
`services/tts-worker/scripts/benchmark_voxtral.py --suite` on the fixed
five-sentence corpus (voice `neutral_female`, seed 0) on the RTX 3060 12 GB.
Wall RTF = generation wall seconds / audio seconds (lower is better; < 1 is
faster than realtime). Decode FPS counts model frames (12.5 frames = 1 audio
second). This ledger is the single answer to "what did each change buy."

| date | change | commit | profile | mean FPS | mean wall RTF | cold start s | notes |
|---|---|---|---|---|---|---|---|
| 2026-07-16 | baseline (M1) | 5464e73 | compatibility | 16.1 | 1.04 | 128.1 | decode RTF 0.78; frame time: backbone 50% / acoustic 50%; cold start 128 s with warm page cache, 282 s first-ever |
| 2026-07-16 | baseline (M1) | 5464e73 | quality | 7.6 | 1.71 | 128.1 | decode RTF 1.66; acoustic solver 77% of frame time (8 flow steps) |
| 2026-07-16 | baseline (M1) | 5464e73 | balanced | 23.2 | 1.94 | 128.1 | decode RTF 0.54; wall RTF skewed by 17 s + 9 s prefill shape recompiles on first two sentences (steady-state wall RTF ~0.55-0.62); 20.4 s one-time compile warmup |

| 2026-07-16 | M2 quantized-weight disk cache | (this commit) | compatibility | 15.1 | — | 5.4 | generation untouched; WAV bitwise identical to slow path (sha256 d6314f7a); cache is 3.5 GB in the model dir |

Full baseline detail: `voxtral-baseline-suite-2026-07-16.md`. Codec decode is
negligible (~0.03 s/sentence after warmup); loop overhead ~0. Rows are
appended by each optimization PR; the M1 baseline is row zero.
