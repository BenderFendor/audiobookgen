# Adapted from TheMHD1/voxtral-int4 commit 93d3e21 (MIT per upstream README).
"""Optimized selective-INT4 Voxtral generation primitives."""

import time
import sys
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

from .audio import GeneratedAudio, postprocess_audio, trim_warmup_frames
from .errors import (
    CodecFailure,
    EmptyWaveform,
    NoEndOfAudio,
    NonFiniteWaveform,
    VoxtralError,
    classify_cuda_error,
)
from .model import apply_rotary_emb


# ─── Static BF16 KV Cache ──────────────────────────────────────────────


class StaticGQAAttention(nn.Module):
    """GQAAttention with pre-allocated BF16 KV buffers + padding mask.

    Decode path uses STATIC tensor shapes (full buffer + mask) for CUDA graph compatibility.
    Prefill path uses standard causal attention (dynamic shape, not graphed).
    """

    def __init__(self, original_attn, max_seq_len=700):
        super().__init__()
        self.n_heads = original_attn.n_heads
        self.n_kv_heads = original_attn.n_kv_heads
        self.head_dim = original_attn.head_dim
        self.n_rep = original_attn.n_rep
        self.wq = original_attn.wq
        self.wk = original_attn.wk
        self.wv = original_attn.wv
        self.wo = original_attn.wo
        self.max_seq_len = max_seq_len
        self._k_buf = None
        self._v_buf = None

    def _ensure_buffers(self, device, dtype):
        if self._k_buf is None:
            self._k_buf = torch.zeros(
                1,
                self.n_kv_heads,
                self.max_seq_len,
                self.head_dim,
                device=device,
                dtype=dtype,
            )
            self._v_buf = torch.zeros(
                1,
                self.n_kv_heads,
                self.max_seq_len,
                self.head_dim,
                device=device,
                dtype=dtype,
            )

    def reset(self):
        self._k_buf = None
        self._v_buf = None

    def forward(self, x, freqs_cis=None, mask=None, cache=None, pos=0):
        B, T, _ = x.shape
        q = self.wq(x).view(B, T, self.n_heads, self.head_dim).transpose(1, 2)
        k = self.wk(x).view(B, T, self.n_kv_heads, self.head_dim).transpose(1, 2)
        v = self.wv(x).view(B, T, self.n_kv_heads, self.head_dim).transpose(1, 2)

        if freqs_cis is not None:
            q, k = apply_rotary_emb(q, k, freqs_cis[pos : pos + T])

        # Always write to buffer so decode steps can read prefill KV
        self._ensure_buffers(x.device, x.dtype)
        self._k_buf[:, :, pos : pos + T] = k
        self._v_buf[:, :, pos : pos + T] = v

        if cache is not None:
            # Decode: read from buffer up to current position
            k = self._k_buf[:, :, : pos + T]
            v = self._v_buf[:, :, : pos + T]

        is_causal = mask is None and cache is None and T > 1

        new_cache = True  # Sentinel — cache is managed internally

        if self.n_rep > 1:
            k = k.repeat_interleave(self.n_rep, dim=1)
            v = v.repeat_interleave(self.n_rep, dim=1)

        out = F.scaled_dot_product_attention(
            q, k, v, attn_mask=mask, is_causal=is_causal
        )

        out = out.transpose(1, 2).contiguous().view(B, T, -1)
        return self.wo(out), new_cache


def enable_static_cache(model, max_seq_len=700):
    """Replace GQAAttention with StaticGQAAttention in all backbone layers."""
    for layer in model.backbone.layers:
        old_attn = layer.attention
        if not isinstance(old_attn, StaticGQAAttention):
            layer.attention = StaticGQAAttention(old_attn, max_seq_len)


def reset_static_cache(model):
    """Reset static cache buffers for new generation."""
    for layer in model.backbone.layers:
        if isinstance(layer.attention, StaticGQAAttention):
            layer.attention.reset()


@torch.no_grad()
def generate_speech_fast(
    model,
    tokenizer,
    text: str,
    voice_name: str = "neutral_female",
    voice_dir: str = None,
    max_frames: int = 500,
    device: str = "cuda",
    flow_steps: int = 3,
    cfg_alpha: float = 1.2,
    engine_profile: str = "balanced",
    seed: int = 0,
    collect_timings: bool = False,
) -> GeneratedAudio:
    """Generate one required sentence; failures are explicit and never partial."""
    if cfg_alpha < 1.2:
        raise ValueError("production Voxtral profiles require cfg_alpha >= 1.2")
    config = model.config
    reset_static_cache(model)
    torch.manual_seed(seed)
    torch.cuda.manual_seed_all(seed)
    timings: dict | None = {} if collect_timings else None
    setup_started = time.monotonic()

    # Tokenize
    text_tokens = tokenizer.encode(text)
    prompt_ids = [config.bos_id, config.begin_audio_id]

    # Load voice embedding
    voice_path = Path(voice_dir or "") / f"{voice_name}.pt"
    if not voice_path.is_file():
        raise VoxtralError(f"voice embedding is missing: {voice_name}")
    voice_embed = torch.load(voice_path, weights_only=True).to(
        device=device, dtype=torch.bfloat16
    )
    n_voice_frames = voice_embed.shape[0]
    prompt_ids.extend([config.audio_id] * n_voice_frames)

    prompt_ids.append(config.inst_end_id)
    prompt_ids.extend(text_tokens)
    prompt_ids.append(config.inst_id)
    prompt_ids.append(config.begin_audio_id)

    # Build embeddings
    prompt_tensor = torch.tensor([prompt_ids], device=device)
    prompt_embed = model.backbone.tok_embeddings(prompt_tensor)

    prompt_embed[0, 2 : 2 + n_voice_frames] = voice_embed

    # Prefill
    if timings is not None:
        timings["setup_seconds"] = time.monotonic() - setup_started
        torch.cuda.synchronize()
    prefill_started = time.monotonic()
    # freqs_cis must keep one fixed shape across sentences: the compiled
    # decode step guards on it, and a per-sentence size (prompt length +
    # max_frames) forces a multi-second recompile on every new length.
    needed_freqs = len(prompt_ids) + max_frames + 1
    if model.backbone.freqs_cis is None or model.backbone.freqs_cis.shape[0] < needed_freqs:
        model.backbone.setup_freqs(max_len=max(1280, needed_freqs), device=device)
    # Prompt length varies per sentence and the backbone specializes shapes,
    # so a compiled backbone recompiles on every new length (multi-second
    # stalls). Prefill therefore always runs the eager module; only the
    # static single-token decode step below uses the compiled wrapper.
    prefill_backbone = getattr(model.backbone, "_orig_mod", model.backbone)
    hidden, caches = prefill_backbone(prompt_embed)
    pos = len(prompt_ids)

    # First decode step: AUDIO token
    audio_tok_embed = model.backbone.tok_embeddings(
        torch.tensor([[config.audio_id]], device=device)
    )
    hidden, caches = model.backbone(audio_tok_embed, caches=caches, pos=pos)
    pos += 1
    h = hidden[:, -1, :]
    if timings is not None:
        torch.cuda.synchronize()
        timings["prefill_seconds"] = time.monotonic() - prefill_started

    # CUDA event pairs isolate acoustic-solver time from backbone-step time
    # inside the frame loop without a per-frame host synchronization.
    acoustic_events: list[tuple] = []
    backbone_events: list[tuple] = []

    all_codes = []
    t0 = time.time()

    reached_end = False
    for _frame_index in range(max_frames):
        try:
            if timings is not None:
                start_event = torch.cuda.Event(enable_timing=True)
                end_event = torch.cuda.Event(enable_timing=True)
                start_event.record()
            # Fast acoustic decode
            codes, is_end = _decode_one_frame_fast(
                model.acoustic, h, config, flow_steps=flow_steps, cfg_alpha=cfg_alpha
            )
            if timings is not None:
                end_event.record()
                acoustic_events.append((start_event, end_event))

            if is_end.any():
                reached_end = True
                break

            all_codes.append(codes)

            # Embed and advance LLM
            if timings is not None:
                start_event = torch.cuda.Event(enable_timing=True)
                end_event = torch.cuda.Event(enable_timing=True)
                start_event.record()
            next_embed = model.embed_audio_codes(codes).unsqueeze(1)
            hidden, caches = model.backbone(next_embed, caches=caches, pos=pos)
            pos += 1
            h = hidden[:, -1, :]
            if timings is not None:
                end_event.record()
                backbone_events.append((start_event, end_event))
        except RuntimeError as error:
            raise classify_cuda_error(error) from error

    gen_time = time.time() - t0
    if timings is not None:
        torch.cuda.synchronize()
        timings["decode_loop_seconds"] = gen_time
        timings["acoustic_seconds"] = sum(
            start.elapsed_time(end) for start, end in acoustic_events
        ) / 1_000.0
        timings["backbone_seconds"] = sum(
            start.elapsed_time(end) for start, end in backbone_events
        ) / 1_000.0
        timings["loop_overhead_seconds"] = max(
            0.0,
            gen_time - timings["acoustic_seconds"] - timings["backbone_seconds"],
        )
    n_frames = len(all_codes)

    if n_frames == 0:
        raise EmptyWaveform("Voxtral generated no audio frames")
    if not reached_end:
        raise NoEndOfAudio(f"Voxtral reached the {max_frames}-frame limit")

    fps = n_frames / gen_time
    duration = n_frames / 12.5
    rtf = gen_time / duration
    print(
        f"Voxtral: {n_frames} frames in {gen_time:.1f}s ({fps:.1f} fps, RTF={rtf:.2f})",
        file=sys.stderr,
        flush=True,
    )

    all_codes = trim_warmup_frames(all_codes)
    n_frames = len(all_codes)
    if n_frames == 0:
        raise EmptyWaveform("warmup trimming removed every generated frame")

    # Decode to audio — sync first to catch any pending CUDA errors from generation
    codec_started = time.monotonic()
    try:
        torch.cuda.synchronize()
        all_codes_tensor = torch.stack(all_codes, dim=1)
        audio = model.codec(all_codes_tensor)
        audio = audio[0].float().cpu().numpy()
    except RuntimeError as error:
        raise CodecFailure(str(classify_cuda_error(error))) from error
    if timings is not None:
        timings["codec_seconds"] = time.monotonic() - codec_started

    if not np.isfinite(audio).all():
        raise NonFiniteWaveform("codec output contains NaN or infinity")
    postprocess_started = time.monotonic()
    audio, sample_rate = postprocess_audio(audio)
    if timings is not None:
        timings["postprocess_seconds"] = time.monotonic() - postprocess_started
    if audio.size == 0 or float(np.abs(audio).max(initial=0.0)) <= 1e-6:
        raise EmptyWaveform("postprocessed waveform is empty or silent")

    return GeneratedAudio(
        samples=audio,
        sample_rate=sample_rate,
        generation_seconds=gen_time,
        frame_count=n_frames,
        text=text,
        voice=voice_name,
        engine_profile=engine_profile,
        timings=timings,
    )


def _predict_velocity_cfg(acoustic, x_t, llm_hidden, zeros, t, cfg_alpha):
    """One CFG velocity evaluation as a single batch-2 forward.

    Same CFG equation as two separate conditional/unconditional passes, but
    the two branches share one kernel launch sequence, which matters at the
    tiny batch sizes of per-frame decoding.
    """
    if zeros is None:
        # .clone() prevents CUDA graph buffer reuse conflicts with
        # reduce-overhead compile
        return acoustic.predict_velocity(x_t, llm_hidden, t).clone()
    batched_v = acoustic.predict_velocity(
        torch.cat((x_t, x_t), dim=0),
        torch.cat((llm_hidden, zeros), dim=0),
        t,
    ).clone()
    v_cond, v_uncond = batched_v.chunk(2, dim=0)
    return cfg_alpha * v_cond + (1 - cfg_alpha) * v_uncond


@torch.no_grad()
def _decode_one_frame_fast(acoustic, llm_hidden, config, flow_steps=3, cfg_alpha=1.2):
    """
    Fast acoustic frame decode with reduced steps and optional CFG.

    The midpoint solver makes 2 velocity evaluations per interval; with CFG
    each evaluation is one batch-2 forward, so flow_steps=3 costs 4 acoustic
    forward passes and flow_steps=8 costs 14.
    """
    B = llm_hidden.shape[0]
    device = llm_hidden.device

    # Semantic code prediction (NaN-safe for numerical stability in long sequences)
    logits = acoustic.semantic_codebook_output(llm_hidden)
    logits = torch.nan_to_num(logits, nan=-1e9)
    logits[:, 0] = -1e9
    logits[:, 8194:] = -1e9
    semantic_code = logits.argmax(dim=-1).clamp(0, 8193)

    is_end = semantic_code <= config.end_audio

    # Flow matching with reduced steps and midpoint solver
    x_t = torch.randn(B, config.n_acoustic_codebooks, device=device) * config.sigma_max
    timesteps = torch.linspace(0, 1, flow_steps, device=device)

    use_cfg = cfg_alpha != 1.0
    zeros = torch.zeros_like(llm_hidden) if use_cfg else None

    for i in range(flow_steps - 1):
        t = timesteps[i].item()
        dt = (timesteps[i + 1] - timesteps[i]).item()

        # Midpoint method (2nd order) for better accuracy with fewer steps
        v1 = _predict_velocity_cfg(acoustic, x_t, llm_hidden, zeros, t, cfg_alpha)

        # Midpoint: evaluate at t + dt/2
        x_mid = x_t + v1 * (dt / 2)
        t_mid = t + dt / 2
        v2 = _predict_velocity_cfg(acoustic, x_mid, llm_hidden, zeros, t_mid, cfg_alpha)

        # Update using midpoint velocity
        x_t = x_t + v2 * dt

    # FSQ quantize (NaN guard for numerical stability in long sequences)
    x_t = torch.nan_to_num(x_t, nan=0.0, posinf=1.0, neginf=-1.0)
    x_clamp = x_t.clamp(-1, 1)
    scaled = (x_clamp + 1) / 2 * (config.fsq_levels - 1)
    acoustic_codes = scaled.round().long().clamp(0, config.fsq_levels - 1)
    acoustic_codes = acoustic_codes + config.special_count
    acoustic_codes[is_end] = config.empty_audio
    codes = torch.cat([semantic_code.unsqueeze(1), acoustic_codes], dim=1)
    return codes, is_end
