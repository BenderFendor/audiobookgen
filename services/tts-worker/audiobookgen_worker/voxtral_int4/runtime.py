"""Load, quantize, measure, and serialize one Voxtral GPU model."""

import gc
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any


PROFILES: dict[str, dict[str, Any]] = {
    "balanced": {"flow_steps": 3, "cfg_alpha": 1.2, "compile": True, "max_frames": 500},
    "quality": {"flow_steps": 8, "cfg_alpha": 1.2, "compile": False, "max_frames": 700},
    "compatibility": {
        "flow_steps": 3,
        "cfg_alpha": 1.2,
        "compile": False,
        "max_frames": 350,
    },
}


class VoxtralInt4Runtime:
    def __init__(self, model_dir: Path, progress: Callable[[str], None]) -> None:
        import torch
        from safetensors.torch import load_file
        from torchao.quantization import Int4WeightOnlyConfig, quantize_

        from .inference import enable_static_cache
        from .load_model import _assign_weights
        from .model import VoxtralConfig, VoxtralTTS
        from .tekken import TekkenTokenizer

        if not torch.cuda.is_available():
            raise RuntimeError("Voxtral INT4 requires an NVIDIA CUDA GPU")
        major, minor = torch.cuda.get_device_capability()
        if (major, minor) < (8, 0):
            raise RuntimeError(
                "Voxtral INT4 requires CUDA compute capability 8.0 or newer"
            )

        self.model_dir = model_dir
        self.device = "cuda"
        torch.cuda.reset_peak_memory_stats()
        progress("loading Voxtral BF16 weights on CPU")
        started = time.monotonic()
        state = load_file(str(model_dir / "consolidated.safetensors"), device="cpu")
        model = VoxtralTTS(VoxtralConfig())
        _assign_weights(model, state)
        del state
        gc.collect()

        # Loading the complete BF16 graph on CUDA before quantization can exceed
        # a 12 GB card even though steady-state INT4 inference fits. Quantize one
        # backbone layer at a time, leaving the quality-sensitive acoustic and
        # codec modules on CPU until the backbone has been compressed.
        config = Int4WeightOnlyConfig(
            group_size=64,
            int4_packing_format="tile_packed_to_4d",
            int4_choose_qparams_algorithm="hqq",
        )
        progress("quantizing the language backbone layer by layer to HQQ INT4")
        for index, layer in enumerate(model.backbone.layers, start=1):
            layer.to(device="cuda", dtype=torch.bfloat16)
            quantize_(layer, config)
            progress(f"quantized backbone layer {index}/{len(model.backbone.layers)}")
        model.backbone.tok_embeddings.to(device="cuda", dtype=torch.bfloat16)
        model.backbone.norm.to(device="cuda", dtype=torch.bfloat16)

        progress("moving the acoustic transformer and codec to CUDA in BF16")
        model.acoustic.to(device="cuda", dtype=torch.bfloat16)
        model.codec.to(device="cuda", dtype=torch.bfloat16)
        model.audio_codebook_embeddings.to(device="cuda", dtype=torch.bfloat16)
        model.eval()
        gc.collect()
        torch.cuda.empty_cache()
        enable_static_cache(model, max_seq_len=900)

        self.model = model
        self.tokenizer = TekkenTokenizer(model_dir / "tekken.json")
        self.voice_dir = model_dir / "voice_embedding"
        self.load_seconds = time.monotonic() - started
        self.allocated_bytes = torch.cuda.memory_allocated()
        self.reserved_bytes = torch.cuda.memory_reserved()
        self.peak_allocated_bytes = torch.cuda.max_memory_allocated()
        self._compiled = False
        progress("Voxtral INT4 is loaded")

    def voices(self) -> list[str]:
        return sorted(path.stem for path in self.voice_dir.glob("*.pt"))

    @property
    def compiled(self) -> bool:
        return self._compiled

    def metrics(self) -> dict[str, Any]:
        import torch

        properties = torch.cuda.get_device_properties(0)
        return {
            "gpu": properties.name,
            "compute_capability": ".".join(
                map(str, torch.cuda.get_device_capability())
            ),
            "total_vram_bytes": properties.total_memory,
            "allocated_vram_bytes": torch.cuda.memory_allocated(),
            "reserved_vram_bytes": torch.cuda.memory_reserved(),
            "peak_allocated_vram_bytes": torch.cuda.max_memory_allocated(),
            "load_seconds": self.load_seconds,
            "torch": torch.__version__,
        }

    def generate(
        self,
        text: str,
        voice: str,
        profile_name: str = "balanced",
        seed: int = 0,
    ):
        import torch

        from .inference import generate_speech_fast

        profile = PROFILES.get(profile_name)
        if profile is None:
            raise ValueError(f"unknown Voxtral profile: {profile_name}")
        if voice not in self.voices():
            raise ValueError(f"unknown Voxtral voice: {voice}")
        if profile["compile"] and not self._compiled:
            self.model.backbone = torch.compile(
                self.model.backbone, mode="default", fullgraph=False
            )
            self.model.acoustic.predict_velocity = torch.compile(
                self.model.acoustic.predict_velocity, mode="default", fullgraph=False
            )
            self._compiled = True
        with torch.inference_mode():
            return generate_speech_fast(
                self.model,
                self.tokenizer,
                text,
                voice_name=voice,
                voice_dir=str(self.voice_dir),
                max_frames=profile["max_frames"],
                flow_steps=profile["flow_steps"],
                cfg_alpha=profile["cfg_alpha"],
                engine_profile=profile_name,
                seed=seed,
            )
