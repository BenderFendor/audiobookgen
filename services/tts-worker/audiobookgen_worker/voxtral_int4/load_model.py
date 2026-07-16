# Adapted from TheMHD1/voxtral-int4 commit 93d3e21 (MIT per upstream README).
"""Assign official Voxtral safetensor weights to the reconstructed model."""

import torch
from safetensors.torch import load_file

from .model import VoxtralConfig, VoxtralTTS


def load_original_model(model_dir: str, device: str = "cuda") -> VoxtralTTS:
    """Load the original BF16 model for diagnostics."""
    state_dict = load_file(f"{model_dir}/consolidated.safetensors")
    model = VoxtralTTS(VoxtralConfig())
    _assign_weights(model, state_dict)
    return model.to(device=device, dtype=torch.bfloat16).eval()


def _assign_weights(model: VoxtralTTS, state_dict: dict):
    """Map original Voxtral weight keys to our model structure."""
    failures: list[str] = []
    for key, tensor in state_dict.items():
        try:
            _set_weight(model, key, tensor)
        except Exception as error:
            failures.append(f"{key}: {error}")
    if failures:
        sample = "; ".join(failures[:5])
        raise RuntimeError(
            f"could not assign {len(failures)} Voxtral weights: {sample}"
        )


def _set_weight(model: VoxtralTTS, key: str, tensor: torch.Tensor):
    """Set a single weight in the model by its original key name."""

    # LLM backbone embeddings
    if key == "mm_audio_embeddings.tok_embeddings.weight":
        model.backbone.tok_embeddings.weight.data = tensor
        return
    if key == "mm_audio_embeddings.audio_codebook_embeddings.embeddings.weight":
        model.audio_codebook_embeddings.weight.data = tensor
        return

    # LLM backbone layers
    if key.startswith("layers."):
        parts = key.split(".")
        layer_idx = int(parts[1])
        layer = model.backbone.layers[layer_idx]

        if parts[2] == "attention":
            attn = layer.attention
            if parts[3] == "wq":
                attn.wq.weight.data = tensor
            elif parts[3] == "wk":
                attn.wk.weight.data = tensor
            elif parts[3] == "wv":
                attn.wv.weight.data = tensor
            elif parts[3] == "wo":
                attn.wo.weight.data = tensor
        elif parts[2] == "attention_norm":
            layer.attention_norm.weight.data = tensor
        elif parts[2] == "feed_forward":
            ff = layer.feed_forward
            if parts[3] == "w1":
                ff.w1.weight.data = tensor
            elif parts[3] == "w2":
                ff.w2.weight.data = tensor
            elif parts[3] == "w3":
                ff.w3.weight.data = tensor
        elif parts[2] == "ffn_norm":
            layer.ffn_norm.weight.data = tensor
        return

    # LLM final norm
    if key == "norm.weight":
        model.backbone.norm.weight.data = tensor
        return

    # Acoustic transformer
    if key.startswith("acoustic_transformer."):
        remainder = key[len("acoustic_transformer.") :]

        if remainder.startswith("layers."):
            parts = remainder.split(".")
            layer_idx = int(parts[1])
            layer = model.acoustic.layers[layer_idx]

            if parts[2] == "attention":
                attn = layer.attention
                if parts[3] == "wq":
                    attn.wq.weight.data = tensor
                elif parts[3] == "wk":
                    attn.wk.weight.data = tensor
                elif parts[3] == "wv":
                    attn.wv.weight.data = tensor
                elif parts[3] == "wo":
                    attn.wo.weight.data = tensor
            elif parts[2] == "attention_norm":
                layer.attention_norm.weight.data = tensor
            elif parts[2] == "feed_forward":
                ff = layer.feed_forward
                if parts[3] == "w1":
                    ff.w1.weight.data = tensor
                elif parts[3] == "w2":
                    ff.w2.weight.data = tensor
                elif parts[3] == "w3":
                    ff.w3.weight.data = tensor
            elif parts[2] == "ffn_norm":
                layer.ffn_norm.weight.data = tensor
        elif remainder == "norm.weight":
            model.acoustic.norm.weight.data = tensor
        elif remainder == "input_projection.weight":
            model.acoustic.input_projection.weight.data = tensor
        elif remainder == "time_projection.weight":
            model.acoustic.time_projection.weight.data = tensor
        elif remainder == "llm_projection.weight":
            model.acoustic.llm_projection.weight.data = tensor
        elif remainder == "semantic_codebook_output.weight":
            model.acoustic.semantic_codebook_output.weight.data = tensor
        elif remainder == "semantic_codebook_output.bias":
            model.acoustic.semantic_codebook_output.bias.data = tensor
        elif remainder == "acoustic_codebook_output.weight":
            model.acoustic.acoustic_codebook_output.weight.data = tensor
        return

    # Codec decoder
    if key.startswith("audio_tokenizer."):
        remainder = key[len("audio_tokenizer.") :]

        if remainder.startswith("quantizer.semantic_codebook.embedding_sum"):
            model.codec.semantic_embedding_sum.data = tensor
            return
        if remainder.startswith("quantizer.semantic_codebook.cluster_usage"):
            model.codec.semantic_cluster_usage.data = tensor
            return

        if remainder.startswith("decoder_blocks."):
            parts = remainder.split(".")
            block_idx = int(parts[1])

            # Input conv (block 0)
            if block_idx == 0:
                if "original0" in remainder:
                    model.codec.input_conv.weight_g.data = tensor
                elif "original1" in remainder:
                    model.codec.input_conv.weight_v.data = tensor
                return

            # Upsample convs (blocks 2, 4, 6)
            if block_idx in [2, 4, 6]:
                conv_idx = [2, 4, 6].index(block_idx)
                if "original0" in remainder:
                    model.codec.upsample_convs[conv_idx].weight_g.data = tensor
                elif "original1" in remainder:
                    model.codec.upsample_convs[conv_idx].weight_v.data = tensor
                return

            # Transformer stages (blocks 1, 3, 5, 7)
            if block_idx in [1, 3, 5, 7]:
                stage_idx = [1, 3, 5, 7].index(block_idx)
                stage = model.codec.transformer_stages[stage_idx]

                layer_idx = int(parts[3])
                layer = stage[layer_idx]

                submodule = parts[4]
                if submodule == "attention":
                    attn = layer.attention
                    param = parts[5]
                    if param == "wq":
                        attn.wq.weight.data = tensor
                    elif param == "wk":
                        attn.wk.weight.data = tensor
                    elif param == "wv":
                        attn.wv.weight.data = tensor
                    elif param == "wo":
                        attn.wo.weight.data = tensor
                    elif param == "q_norm":
                        attn.qk_norm.q_norm.weight.data = tensor
                    elif param == "k_norm":
                        attn.qk_norm.k_norm.weight.data = tensor
                elif submodule == "attention_norm":
                    layer.attention_norm.weight.data = tensor
                elif submodule == "attention_scale":
                    layer.attention_scale.data = tensor
                elif submodule == "feed_forward":
                    ff = layer.feed_forward
                    param = parts[5]
                    if param == "w1":
                        ff.w1.weight.data = tensor
                    elif param == "w2":
                        ff.w2.weight.data = tensor
                    elif param == "w3":
                        ff.w3.weight.data = tensor
                elif submodule == "ffn_norm":
                    layer.ffn_norm.weight.data = tensor
                elif submodule == "ffn_scale":
                    layer.ffn_scale.data = tensor
                return

        if remainder.startswith("output_proj."):
            if "original0" in remainder:
                model.codec.output_proj.weight_g.data = tensor
            elif "original1" in remainder:
                model.codec.output_proj.weight_v.data = tensor
            return
